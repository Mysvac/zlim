/// Implement `docs` and `with_docs`.
macro_rules! impl_docs_fn {
    ($field:ident) => {
        /// Returns the documentation string for the type, if `reflect_docs` is
        /// enabled and docs are present.
        ///
        /// If `reflect_docs` feature is not enabled, this function always returns `None`.
        /// So you can use this without worrying about compilation options.
        ///
        /// See examples in [`TypeInfo::docs`](crate::info::TypeInfo::docs) .
        #[inline(always)]
        pub const fn docs(&self) -> Option<&'static str> {
            #[cfg(not(feature = "reflect_docs"))]
            return None;
            #[cfg(feature = "reflect_docs")]
            return self.$field;
        }

        /// Replaces docs (overwrite, do not merge).
        ///
        /// No-op if `reflect_docs` feature is disabled.
        #[inline]
        pub fn with_docs(self, _doc_: Option<&'static str>) -> Self {
            #[cfg(feature = "reflect_docs")]
            return Self {
                $field: _doc_,
                ..self
            };

            #[cfg(not(feature = "reflect_docs"))]
            return self;
        }
    };
}

pub(crate) use impl_docs_fn;
