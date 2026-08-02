/// Parallel helpers for slice-like batch operations.
///
/// When the `multi_thread` cfg path is enabled, methods process the slice in
/// chunks across worker threads using the global [`MainTaskPool`]. When
/// multi-threading is disabled (single-threaded or WASM builds), methods fall
/// back to the equivalent sequential iterator behavior.
///
/// All closures are required to implement `Clone + Send` because each worker
/// receives its own cloned closure instance.
///
/// [`MainTaskPool`]: crate::MainTaskPool
pub trait ParallelSlice {
    type Item;

    /// Returns `true` if the slice contains the given value.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_task::ParallelSlice;
    ///
    /// let data = [3, 7, 9, 12];
    /// let r1 = data.par_contains(&3);
    /// let r2 = data.par_contains(&4);
    ///
    /// assert_eq!(r1, true);
    /// assert_eq!(r2, false);
    /// ```
    fn par_contains(&self, val: &Self::Item) -> bool
    where
        Self::Item: PartialEq;

    /// Returns the index of first element that satisfies `f`.
    ///
    /// This mirrors [`Iterator::position`] semantics in the sequential path.
    /// In the parallel path, if multiple elements satisfy `f`, which matching
    /// index is returned is not guaranteed.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_task::ParallelSlice;
    ///
    /// let data = [3, 7, 9, 12];
    /// let index = data.par_position(|v| *v % 3 == 0);
    ///
    /// assert_eq!(index, Some(0));
    /// ```
    fn par_position(&self, f: impl FnMut(&Self::Item) -> bool + Clone + Send) -> Option<usize>;

    /// Applies `f` to each element.
    ///
    /// Uses parallel chunk execution when possible, otherwise falls back to
    /// `iter().for_each(f)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::sync::atomic::{AtomicU32, Ordering};
    /// use zlim_task::ParallelSlice;
    ///
    /// let data = [1u32, 2, 3, 4];
    /// let sum = AtomicU32::new(0);
    ///
    /// data.par_each(|v| {
    ///     sum.fetch_add(*v, Ordering::Relaxed);
    /// });
    ///
    /// assert_eq!(sum.load(Ordering::Relaxed), 10);
    /// ```
    fn par_each(&self, f: impl FnMut(&Self::Item) + Clone + Send);

    /// Applies `f` to each element mutably.
    ///
    /// Uses parallel chunk execution when possible, otherwise falls back to
    /// `iter_mut().for_each(f)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_task::ParallelSlice;
    ///
    /// let mut data = [1, 2, 3, 4];
    /// data.par_each_mut(|v| *v *= 2);
    ///
    /// assert_eq!(data, [2, 4, 6, 8]);
    /// ```
    fn par_each_mut(&mut self, f: impl FnMut(&mut Self::Item) + Clone + Send);

    /// Maps each element to a new value and collects into a [`Vec`].
    ///
    /// Uses parallel chunk execution when possible, otherwise falls back to
    /// `iter().map(f).collect()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_task::ParallelSlice;
    ///
    /// let data = [1, 2, 3, 4];
    /// let doubled = data.par_map(|v| v * 2);
    ///
    /// assert_eq!(doubled, vec![2, 4, 6, 8]);
    /// ```
    fn par_map<R: Send + 'static>(&self, f: impl FnMut(&Self::Item) -> R + Clone + Send) -> Vec<R>;

    /// Maps each element mutably to a new value and collects into a [`Vec`].
    ///
    /// Uses parallel chunk execution when possible, otherwise falls back to
    /// `iter_mut().map(f).collect()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_task::ParallelSlice;
    ///
    /// let mut data = [1, 2, 3];
    /// let old_values = data.par_map_mut(|v| {
    ///     let old = *v;
    ///     *v += 10;
    ///     old
    /// });
    ///
    /// assert_eq!(old_values, vec![1, 2, 3]);
    /// assert_eq!(data, [11, 12, 13]);
    /// ```
    fn par_map_mut<R: Send + 'static>(
        &mut self,
        f: impl FnMut(&mut Self::Item) -> R + Clone + Send,
    ) -> Vec<R>;

    /// Applies `f` to each chunk (splat) and collects one value per chunk.
    ///
    /// Uses parallel chunk execution when possible, otherwise falls back to
    /// a single call over the full slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_task::ParallelSlice;
    ///
    /// let data = [1, 2, 3, 4];
    /// let chunk_sums = data.par_splat_map(|chunk| chunk.iter().sum::<i32>());
    ///
    /// // Number of chunks depends on runtime task-pool setup.
    /// assert_eq!(chunk_sums.iter().sum::<i32>(), 10);
    /// ```
    fn par_splat_map<R: Send + 'static>(
        &self,
        f: impl FnMut(&[Self::Item]) -> R + Clone + Send,
    ) -> Vec<R>;

    /// Applies `f` to each mutable chunk (splat) and collects one value per chunk.
    ///
    /// Uses parallel chunk execution when possible, otherwise falls back to
    /// a single call over the full mutable slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use zlim_task::ParallelSlice;
    ///
    /// let mut data = [1, 2, 3, 4];
    /// let old_chunk_sums = data.par_splat_map_mut(|chunk| {
    ///     let sum = chunk.iter().sum::<i32>();
    ///     for v in chunk.iter_mut() {
    ///         *v *= 2;
    ///     }
    ///     sum
    /// });
    ///
    /// // Number of chunks depends on runtime task-pool setup.
    /// assert_eq!(old_chunk_sums.iter().sum::<i32>(), 10);
    /// assert_eq!(data, [2, 4, 6, 8]);
    /// ```
    fn par_splat_map_mut<R: Send + 'static>(
        &mut self,
        f: impl FnMut(&mut [Self::Item]) -> R + Clone + Send,
    ) -> Vec<R>;
}

impl<T: Send + Sync> ParallelSlice for [T] {
    type Item = T;

    fn par_contains(&self, val: &Self::Item) -> bool
    where
        Self::Item: PartialEq,
    {
        crate::cfg::multi_thread! {
            if {
                let pool = crate::MainTaskPool::get();
                let threads = pool.thread_count().max(1);
                let chunk_size = (self.len() / threads).max(1);
                let results = pool.scope(|scope| {
                    for chunk in self.chunks(chunk_size) {
                        scope.spawn(async move { chunk.contains(val) });
                    }
                });
                results.iter().any(|&r| r)
            } else {
                self.contains(val)
            }
        }
    }

    fn par_position(&self, f: impl FnMut(&Self::Item) -> bool + Clone + Send) -> Option<usize> {
        crate::cfg::multi_thread! {
            if {
                let pool = crate::MainTaskPool::get();
                let threads = pool.thread_count().max(1);
                let chunk_size = (self.len() / threads).max(1);
                let results = pool.scope(|scope| {
                    for (index, chunk) in self.chunks(chunk_size).enumerate() {
                        let func = f.clone();
                        scope.spawn(async move { (index, chunk.iter().position(func)) });
                    }
                });
                for (index, offset) in results {
                    if let Some(offset) = offset {
                        return Some(index * chunk_size + offset);
                    }
                }
                None
            } else {
                self.iter().position(f)
            }
        }
    }

    fn par_each(&self, f: impl FnMut(&Self::Item) + Clone + Send) {
        crate::cfg::multi_thread! {
            if {
                let pool = crate::MainTaskPool::get();
                let threads = pool.thread_count().max(1);
                let chunk_size = (self.len() / threads).max(1);
                pool.scope(|scope| {
                    for chunk in self.chunks(chunk_size) {
                        let func = f.clone();
                        scope.spawn(async move { chunk.iter().for_each(func); });
                    }
                });
            } else {
                self.iter().for_each(f);
            }
        }
    }

    fn par_each_mut(&mut self, f: impl FnMut(&mut Self::Item) + Clone + Send) {
        crate::cfg::multi_thread! {
            if {
                let pool = crate::MainTaskPool::get();
                let threads = pool.thread_count().max(1);
                let chunk_size = (self.len() / threads).max(1);
                pool.scope(|scope| {
                    for chunk in self.chunks_mut(chunk_size) {
                        let func = f.clone();
                        scope.spawn(async move { chunk.iter_mut().for_each(func); });
                    }
                });
            } else {
                self.iter_mut().for_each(f);
            }
        }
    }

    fn par_map<R: Send + 'static>(&self, f: impl FnMut(&Self::Item) -> R + Clone + Send) -> Vec<R> {
        crate::cfg::multi_thread! {
            if {
                let pool = crate::MainTaskPool::get();
                let threads = pool.thread_count().max(1);
                let chunk_size = (self.len() / threads).max(1);
                let mut results = pool.scope(|scope| {
                    for chunk in self.chunks(chunk_size) {
                        let func = f.clone();
                        scope.spawn(async move { chunk.iter().map(func).collect::<Vec<R>>() });
                    }
                });
                let mut result = Vec::<R>::with_capacity(self.len());
                for items in results.iter_mut() {
                    result.append(items);
                }
                result
            } else {
                self.iter().map(f).collect()
            }
        }
    }

    fn par_map_mut<R: Send + 'static>(
        &mut self,
        f: impl FnMut(&mut Self::Item) -> R + Clone + Send,
    ) -> Vec<R> {
        crate::cfg::multi_thread! {
            if {
                let pool = crate::MainTaskPool::get();
                let threads = pool.thread_count().max(1);
                let chunk_size = (self.len() / threads).max(1);
                let mut results = pool.scope(|scope| {
                    for chunk in self.chunks_mut(chunk_size) {
                        let func = f.clone();
                        scope.spawn(async move { chunk.iter_mut().map(func).collect::<Vec<R>>() });
                    }
                });
                let mut result = Vec::<R>::with_capacity(self.len());
                for items in results.iter_mut() {
                    result.append(items);
                }
                result
            } else {
                self.iter_mut().map(f).collect()
            }
        }
    }

    fn par_splat_map<R: Send + 'static>(
        &self,
        f: impl FnMut(&[Self::Item]) -> R + Clone + Send,
    ) -> Vec<R> {
        crate::cfg::multi_thread! {
            if {
                let pool = crate::MainTaskPool::get();
                let threads = pool.thread_count().max(1);
                let chunk_size = (self.len() / threads).max(1);

                pool.scope(|scope| {
                    for chunk in self.chunks(chunk_size) {
                        let mut func = f.clone();
                        scope.spawn(async move { func(chunk) });
                    }
                })
            } else {
                let mut f = f; // need mutable
                Vec::from([f(self)])
            }
        }
    }

    fn par_splat_map_mut<R: Send + 'static>(
        &mut self,
        f: impl FnMut(&mut [Self::Item]) -> R + Clone + Send,
    ) -> Vec<R> {
        crate::cfg::multi_thread! {
            if {
                let pool = crate::MainTaskPool::get();
                let threads = pool.thread_count().max(1);
                let chunk_size = (self.len() / threads).max(1);

                pool.scope(|scope| {
                    for chunk in self.chunks_mut(chunk_size) {
                        let mut func = f.clone();
                        scope.spawn(async move { func(chunk) });
                    }
                })
            } else {
                let mut f = f; // need mutable
                Vec::from([f(self)])
            }
        }
    }
}
