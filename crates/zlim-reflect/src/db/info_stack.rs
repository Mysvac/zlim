use core::fmt::{Debug, Display, Formatter};
use crate::info::TypeInfo;

/// Helper struct for managing a stack of [`TypeInfo`] instances.
pub(super) struct TypeInfoStack {
    stack: Vec<&'static TypeInfo>,
}

impl TypeInfoStack {
    /// Create a new empty [`TypeInfoStack`].
    pub const fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Push a new [`TypeInfo`] onto the stack.
    pub fn push(&mut self, info: &'static TypeInfo) {
        self.stack.push(info);
    }

    /// Pop the last [`TypeInfo`] off the stack.
    pub fn pop(&mut self) {
        self.stack.pop();
    }
}

impl Debug for TypeInfoStack {
    #[inline(never)]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut iter = self.stack.iter();

        if let Some(first) = iter.next() {
            writeln!(f, "\n`{}`", first.type_path())?;
        }

        for info in iter {
            writeln!(f, " -> `{}`", info.type_path())?;
        }

        Ok(())
    }
}

impl Display for TypeInfoStack {
    #[inline(never)]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut iter = self.stack.iter();

        if let Some(first) = iter.next() {
            writeln!(f, "\n`{}`", first.type_name())?;
        }

        for info in iter {
            writeln!(f, " -> `{}`", info.type_name())?;
        }

        Ok(())
    }
}
