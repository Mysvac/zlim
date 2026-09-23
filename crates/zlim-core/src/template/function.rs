use crate::error::ZlimResult;

use super::{Template, TemplateContext};

// -----------------------------------------------------------------------------
// FnTemplate

/// A [`Template`] driven by a function or a closure.
///
/// This is what [`template`] creates, for descriptions that are only used once
/// and do not deserve a type of their own.
pub struct FnTemplate<F: Fn(&mut TemplateContext) -> ZlimResult<O>, O>(pub F);

impl<F, O> Template for FnTemplate<F, O>
where
    F: Fn(&mut TemplateContext) -> ZlimResult<O> + Clone,
{
    type Output = O;

    #[inline]
    fn build_template(&self, context: &mut TemplateContext) -> ZlimResult<Self::Output> {
        (self.0)(context)
    }

    #[inline]
    fn clone_template(&self) -> Self {
        Self(self.0.clone())
    }
}

// -----------------------------------------------------------------------------
// Functions

/// Creates a [`Template`] from the given function.
///
/// The function is called with the context of every build,
/// which gives it access to the world, so it can look up
/// whatever the value it produces needs.
///
/// # Examples
///
/// ```
/// use zlim_core::template::template;
///
/// let scale = template(|context| {
///     let _ = context;
///     Ok(2.0_f32)
/// });
/// ```
pub fn template<F, O>(func: F) -> FnTemplate<F, O>
where
    F: Fn(&mut TemplateContext) -> ZlimResult<O> + Clone,
{
    FnTemplate(func)
}
