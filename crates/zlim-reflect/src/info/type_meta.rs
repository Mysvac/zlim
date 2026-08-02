use core::any::{Any, TypeId};

use crate::path::TypePath;

// ----------------------------------------------------------------------------
// PathTable

/// A vtable providing dynamic access to [`TypePath`] APIs.
#[derive(Clone, Copy)]
pub struct PathTable {
    type_path: fn() -> &'static str,
    type_name: fn() -> &'static str,
    type_ident: &'static str,
    crate_name: Option<&'static str>,
    module_path: Option<&'static str>,
}

impl PathTable {
    /// Creates a new table from a type.
    #[inline]
    pub const fn of<T: TypePath + ?Sized>() -> Self {
        Self {
            type_path: T::type_path,
            type_name: T::type_name,
            type_ident: T::IDENT,
            crate_name: T::CRATE,
            module_path: T::MODULE,
        }
    }

    /// See [`TypePath::type_path`]
    #[inline(always)]
    pub fn type_path(&self) -> &'static str {
        (self.type_path)()
    }

    /// See [`TypePath::type_name`]
    #[inline(always)]
    pub fn type_name(&self) -> &'static str {
        (self.type_name)()
    }

    /// See [`TypePath::IDENT`].
    #[inline(always)]
    pub fn type_ident(&self) -> &'static str {
        self.type_ident
    }

    /// See [`TypePath::CRATE`].
    #[inline]
    pub fn crate_name(&self) -> Option<&'static str> {
        self.crate_name
    }

    /// See [`TypePath::MODULE`].
    #[inline(always)]
    pub fn module_path(&self) -> Option<&'static str> {
        self.module_path
    }
}

impl core::fmt::Debug for PathTable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PathTable")
            .field("type_path", &self.type_path())
            .field("type_name", &self.type_name())
            .field("type_ident", &self.type_ident())
            .field("crate_name", &self.crate_name())
            .field("module_path", &self.module_path())
            .finish()
    }
}

// ----------------------------------------------------------------------------
// Type

/// The base representation of a Rust type.
#[derive(Clone, Copy)]
pub struct Type {
    type_id: TypeId,
    path_table: PathTable,
}

impl Type {
    /// Creates a new [`Type`] from a type that implements [`TypePath`].
    #[inline]
    pub const fn of<T: TypePath + ?Sized>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            path_table: PathTable::of::<T>(),
        }
    }

    /// Returns the [`TypeId`] of the type.
    #[inline(always)]
    pub const fn id(&self) -> TypeId {
        self.type_id
    }

    /// Returns the [`PathTable`] of the type.
    ///
    /// It is usually recommended to directly use the re-exported methods on [`Type`].
    /// Unless it is necessary to copy the `PathTable`.
    #[inline]
    pub const fn path_table(&self) -> PathTable {
        self.path_table
    }

    /// Check if the given type matches this one.
    ///
    /// This only compares the [`TypeId`] of the types.
    #[inline(always)]
    pub fn is<T: Any>(&self) -> bool {
        TypeId::of::<T>() == self.type_id
    }

    /// See [`TypePath::type_path`].
    #[inline]
    pub fn path(&self) -> &'static str {
        self.path_table.type_path()
    }

    /// See [`TypePath::type_name`].
    #[inline]
    pub fn name(&self) -> &'static str {
        self.path_table.type_name()
    }

    /// See [`TypePath::IDENT`].
    #[inline]
    pub fn ident(&self) -> &'static str {
        self.path_table.type_ident()
    }

    /// See [`TypePath::MODULE`].
    #[inline]
    pub fn module_path(&self) -> Option<&'static str> {
        self.path_table.module_path()
    }

    /// See [`TypePath::CRATE`].
    #[inline]
    pub fn crate_name(&self) -> Option<&'static str> {
        self.path_table.crate_name()
    }
}

/// This implementation purely relies on the [`TypeId`] of the type.
impl PartialEq for Type {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id
    }
}

impl Eq for Type {}

/// This implementation purely relies on the [`TypeId`] of the type.
impl core::hash::Hash for Type {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
    }
}

/// This implementation will only output the [`TypePath`] of the type.
impl core::fmt::Debug for Type {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = self.path_table.type_path();
        f.debug_tuple(name).field(&self.type_id).finish()
    }
}

// ----------------------------------------------------------------------------
// Auxiliary macro

macro_rules! impl_type_fn {
    ($field:ident) => {
        /// Returns the underlying `Type`.
        #[inline(always)]
        pub const fn ty(&self) -> &$crate::info::Type {
            &self.$field
        }
        $crate::info::impl_type_fn!();
    };
    ($self:ident => $expr:expr) => {
        /// Returns the underlying `Type`.
        #[inline(never)]
        pub const fn ty($self: &Self) -> &$crate::info::Type {
            $expr
        }
        $crate::info::impl_type_fn!();
    };
    () => {
        /// Returns the `TypeId`.
        #[inline]
        pub const fn type_id(&self) -> ::core::any::TypeId {
            self.ty().id()
        }

        /// Check if the given type matches this one.
        #[inline]
        pub fn type_is<T: ::core::any::Any>(&self) -> bool {
            self.ty().id() == ::core::any::TypeId::of::<T>()
        }

        /// Returns the type path.
        #[inline]
        pub fn type_path(&self) -> &'static str {
            self.ty().path()
        }

        /// Returns the type name.
        #[inline]
        pub fn type_name(&self) -> &'static str {
            self.ty().name()
        }

        /// Returns the type ident.
        #[inline]
        pub fn type_ident(&self) -> &'static str {
            self.ty().ident()
        }

        /// Returns the module path.
        #[inline]
        pub fn module_path(&self) -> Option<&'static str> {
            self.ty().module_path()
        }

        /// Returns the crate name.
        #[inline]
        pub fn crate_name(&self) -> Option<&'static str> {
            self.ty().crate_name()
        }
    };
}

pub(crate) use impl_type_fn;

// ----------------------------------------------------------------------------
