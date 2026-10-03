use super::{IntoTemplate, Template, TemplateContext};
use crate::error::ZlimResult;

// -----------------------------------------------------------------------------
// BuiltInTemplate

/// The template a container falls back to when it is used in a template.
///
/// [`Option`] and [`Vec`] are [`Clone`] and [`Default`], so they already have templates of their
/// own — themselves — which is right as long as their element type is not itself described by a
/// template. For an element that is, such as an [`Option<Handle<T>>`], the template has to be the
/// one of the element, and spelling out `OptionTemplate<HandleTemplate<T>>` by hand gets tedious.
///
/// This trait names that template, and `#[template(built_in)]` on a field is what asks the derive
/// to use it. See [`IntoTemplate`] document for details.
///
/// [`Option<Handle<T>>`]: crate::template
pub trait BuiltInTemplate: Sized {
    /// The template considered built in for this type.
    type Template: Template<Output = Self>;

    fn built_in_template(self) -> Self::Template;
}

impl<T: IntoTemplate> BuiltInTemplate for Option<T> {
    type Template = OptionTemplate<T::Template>;

    fn built_in_template(self) -> Self::Template {
        OptionTemplate(self.map(IntoTemplate::into_template))
    }
}

impl<T: IntoTemplate> BuiltInTemplate for Vec<T> {
    type Template = VecTemplate<T::Template>;

    fn built_in_template(self) -> Self::Template {
        VecTemplate(self.into_iter().map(IntoTemplate::into_template).collect())
    }
}

// -----------------------------------------------------------------------------
// OptionTemplate

/// A built-in [`Template`] of a [`Vec`].
#[repr(transparent)]
pub struct OptionTemplate<T>(pub Option<T>);

impl<T> Default for OptionTemplate<T> {
    #[inline]
    fn default() -> Self {
        Self(None)
    }
}

impl<T> From<Option<T>> for OptionTemplate<T> {
    #[inline]
    fn from(value: Option<T>) -> Self {
        Self(value)
    }
}

impl<T: Template> Template for OptionTemplate<T> {
    type Output = Option<T::Output>;

    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn build_template(&self, context: &mut TemplateContext) -> ZlimResult<Self::Output> {
        match &self.0 {
            Some(template) => Ok(Some(template.build_template(context)?)),
            None => Ok(None),
        }
    }

    #[inline]
    fn clone_template(&self) -> Self {
        match &self.0 {
            Some(template) => Self(Some(template.clone_template())),
            None => Self(None),
        }
    }
}

// -----------------------------------------------------------------------------
// VecTemplate

/// A built-in [`Template`] of a [`Vec`].
#[repr(transparent)]
pub struct VecTemplate<T>(pub Vec<T>);

impl<T> Default for VecTemplate<T> {
    #[inline]
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> From<Vec<T>> for VecTemplate<T> {
    #[inline]
    fn from(value: Vec<T>) -> Self {
        Self(value)
    }
}

impl<T: Template> Template for VecTemplate<T> {
    type Output = Vec<T::Output>;

    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn build_template(&self, context: &mut TemplateContext) -> ZlimResult<Self::Output> {
        let mut output = Vec::with_capacity(self.0.len());
        for template in &self.0 {
            output.push(template.build_template(context)?);
        }
        Ok(output)
    }

    fn clone_template(&self) -> Self {
        Self(self.0.iter().map(Template::clone_template).collect())
    }
}

// -----------------------------------------------------------------------------
