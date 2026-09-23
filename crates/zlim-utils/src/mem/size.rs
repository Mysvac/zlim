//! Types for representing the size of objects in memory.

use core::fmt::Display;

/// The size of an object in memory, in bytes.
///
/// The helper methods on this type use powers of 2 for conversions between units,
/// consistent with the standards for memory reporting.
///
/// While it would technically be more correct to use e.g.
/// "kibibytes" instead of "kilobytes", "mebibytes" instead of "megabytes", etc.,
/// the more common terms are used here for familiarity and discoverability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MemorySize(pub u64);

impl MemorySize {
    /// Creates a new [`MemorySize`] from the given number of bytes.
    ///
    /// This method is provided for convenience,
    /// as many other APIs in Rust use `usize` for sizes and counts.
    /// To initialize this type with a `u64`, just call `MemorySize(bytes)` directly.
    #[inline]
    pub const fn new(bytes: usize) -> Self {
        MemorySize(bytes as u64)
    }

    /// Returns the size in bytes, as a `usize`.
    ///
    /// This method is provided for convenience,
    /// as many other APIs in Rust use `usize` for sizes and counts.
    /// To access the value as a `u64`, just use the `.0` field directly.
    ///
    /// 1 byte = 8 bits.
    #[inline]
    pub const fn as_bytes(self) -> usize {
        self.0 as usize
    }

    /// Returns the size in kilobytes.
    ///
    /// 1 kilobyte = 1024 bytes.
    #[inline]
    pub const fn as_kilobytes(self) -> f64 {
        self.0 as f64 / 1024.0
    }

    /// Returns the size in megabytes.
    ///
    /// 1 megabyte = 1024 kilobytes.
    #[inline]
    pub const fn as_megabytes(self) -> f64 {
        self.0 as f64 / const { 1024.0 * 1024.0 }
    }

    /// Returns the size in gigabytes.
    ///
    /// 1 gigabyte = 1024 megabytes.
    #[inline]
    pub const fn as_gigabytes(self) -> f64 {
        self.0 as f64 / const { 1024.0 * 1024.0 * 1024.0 }
    }

    /// Returns the size in terabytes.
    ///
    /// 1 terabyte = 1024 gigabytes.
    #[inline]
    pub const fn as_terabytes(self) -> f64 {
        self.0 as f64 / const { 1024.0 * 1024.0 * 1024.0 * 1024.0 }
    }

    /// Determine the appropriate unit for displaying the memory size.
    ///
    /// Units are chosen such that the value is at least 1 in that unit.
    ///
    /// This is used for formatting the memory size in a human-readable way,
    /// such as in the [`Display`] implementation for this type.
    pub const fn appropriate_unit(self) -> MemoryUnit {
        if self.0 < 1024 {
            MemoryUnit::Bytes
        } else if self.0 < 1024 * 1024 {
            MemoryUnit::Kilobytes
        } else if self.0 < 1024 * 1024 * 1024 {
            MemoryUnit::Megabytes
        } else if self.0 < 1024 * 1024 * 1024 * 1024 {
            MemoryUnit::Gigabytes
        } else {
            MemoryUnit::Terabytes
        }
    }
}

impl Display for MemorySize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.appropriate_unit() {
            MemoryUnit::Bytes => write!(f, "{} B", self.as_bytes()),
            MemoryUnit::Kilobytes => write!(f, "{:.2} KiB", self.as_kilobytes()),
            MemoryUnit::Megabytes => write!(f, "{:.2} MiB", self.as_megabytes()),
            MemoryUnit::Gigabytes => write!(f, "{:.2} GiB", self.as_gigabytes()),
            MemoryUnit::Terabytes => write!(f, "{:.2} TiB", self.as_terabytes()),
        }
    }
}

/// Common units for representing memory size.
///
/// Used for determining the most appropriate unit to display a [`MemorySize`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryUnit {
    /// 8 bits
    Bytes,
    /// 1 kilobyte = 1024 bytes
    Kilobytes,
    /// 1 megabyte = 1024 kilobytes
    Megabytes,
    /// 1 gigabyte = 1024 megabytes
    Gigabytes,
    /// 1 terabyte = 1024 gigabytes
    Terabytes,
}
