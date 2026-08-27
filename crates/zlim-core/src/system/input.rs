/// Trait for types that can be used as system inputs.
///
/// `SystemInput` defines how input values are passed into systems and how they
/// can be composed. It separates the "borrowed" form (`Data`) used during system
/// execution from the "owned" form (`Item`) used for storage and wrapping.
///
/// # Implementations
///
/// The crate provides implementations for:
/// - Unit type `()` (no input)
/// - [`In<T>`] for owned values
/// - [`InRef<'_, T>`] for shared references
/// - [`InMut<'_, T>`] for mutable references
/// - Tuples of types that implement `SystemInput` (up to 8 elements)
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn double(input: In<u32>) -> u32 {
///     input.0 * 2
/// }
///
/// let mut world = World::alloc();
/// assert_eq!(world.invoke_once(double, 21).unwrap(), 42);
/// ```
pub trait SystemInput: Sized {
    /// The borrowed data type passed to system execution.
    type Data<'i>;
    /// The wrapper type that implements `SystemInput` for storage.
    type Item<'i>: SystemInput;

    /// Wraps the borrowed run-time `Data` into its storable `Item` form.
    fn wrap(this: Self::Data<'_>) -> Self::Item<'_>;
}

/// An owned system input value.
///
/// Wrapping a value in `In<T>` makes it the input of a system function: the
/// caller feeds the owned `T` into the system's input parameter.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn double(input: In<u32>) -> u32 {
///     input.0 * 2
/// }
///
/// let mut world = World::alloc();
/// assert_eq!(world.invoke_once(double, 21).unwrap(), 42);
/// ```
#[derive(Debug)]
#[repr(transparent)]
pub struct In<T>(pub T);

impl<T: 'static> SystemInput for In<T> {
    type Data<'i> = T;
    type Item<'i> = In<T>;

    #[inline(always)]
    fn wrap(this: Self::Data<'_>) -> Self::Item<'_> {
        In(this)
    }
}

/// A shared-reference system input value.
///
/// Wrapping a value in `InRef<T>` feeds a `&T` into the system's input
/// parameter.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn len_of(input: InRef<str>) -> usize {
///     input.0.len()
/// }
///
/// let mut world = World::alloc();
/// assert_eq!(world.invoke_once(len_of, "hello").unwrap(), 5);
/// ```
#[derive(Debug)]
#[repr(transparent)]
pub struct InRef<'i, T: ?Sized>(pub &'i T);

impl<T: ?Sized + 'static> SystemInput for InRef<'_, T> {
    type Data<'i> = &'i T;
    type Item<'i> = InRef<'i, T>;

    #[inline(always)]
    fn wrap(this: Self::Data<'_>) -> Self::Item<'_> {
        InRef(this)
    }
}

/// A mutable-reference system input value.
///
/// Wrapping a value in `InMut<T>` feeds a `&mut T` into the system's input
/// parameter.
///
/// # Examples
///
/// ```rust
/// use zlim_core::prelude::*;
///
/// fn increment(input: InMut<u32>) -> u32 {
///     *input.0 += 1;
///     *input.0
/// }
///
/// let mut world = World::alloc();
/// let value: &'static mut u32 = Box::leak(Box::new(5));
/// assert_eq!(world.invoke_once(increment, value).unwrap(), 6);
/// ```
#[derive(Debug)]
#[repr(transparent)]
pub struct InMut<'a, T: ?Sized>(pub &'a mut T);

impl<T: ?Sized + 'static> SystemInput for InMut<'_, T> {
    type Data<'i> = &'i mut T;
    type Item<'i> = InMut<'i, T>;

    #[inline(always)]
    fn wrap(this: Self::Data<'_>) -> Self::Item<'_> {
        InMut(this)
    }
}

macro_rules! impl_tuple {
    (0: []) => {
        impl SystemInput for () {
            type Data<'i> = ();
            type Item<'i> = ();

            #[inline(always)]
            fn wrap(_: Self::Data<'_>) -> Self::Item<'_> {}
        }
    };
    (1 : [ $index:tt : $name:ident ]) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        #[cfg_attr(docsrs, doc = "This trait is implemented for tuples up to 8 items long.")]
        impl<$name: SystemInput> SystemInput for ($name,) {
            type Data<'i> = ( <$name>::Data<'i>, );
            type Item<'i> = ( <$name>::Item<'i>, );

            #[inline(always)]
            fn wrap(this: Self::Data<'_>) -> Self::Item<'_> {
                ( <$name as SystemInput>::wrap(this.0), )
            }
        }
    };
    ($num:literal : [$($index:tt : $name:ident),*]) => {
        #[cfg_attr(docsrs, doc(hidden))]
        impl<$($name: SystemInput),*> SystemInput for ($($name),*) {
            type Data<'i> = ( $( <$name>::Data<'i> ),* );
            type Item<'i> = ( $( <$name>::Item<'i> ),* );

            #[inline(always)]
            fn wrap(this: Self::Data<'_>) -> Self::Item<'_> {
                ( $( <$name as SystemInput>::wrap(this.$index) ),* )
            }
        }
    };
}

zlim_utils::range_invoke!(impl_tuple, 8);
