use super::{FromTemplate, Template, TemplateContext};
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
/// to use it. See [`FromTemplate`] document for details.
///
/// [`Option<Handle<T>>`]: crate::template
pub trait BuiltInTemplate: Sized {
    /// The template considered built in for this type.
    type Template: Template;
}

impl<T: FromTemplate> BuiltInTemplate for Option<T> {
    type Template = OptionTemplate<T::Template>;
}

impl<T: FromTemplate> BuiltInTemplate for Vec<T> {
    type Template = VecTemplate<T::Template>;
}

// -----------------------------------------------------------------------------
// OptionTemplate

/// A [`Template`] of an [`Option`].
#[derive(Default)]
pub enum OptionTemplate<T> {
    /// The template of an absent value, which builds [`None`].
    #[default]
    None,

    /// The template of the value.
    Some(T),
}

impl<T> From<Option<T>> for OptionTemplate<T> {
    #[inline]
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Some(value),
            None => Self::None,
        }
    }
}

impl<T> From<T> for OptionTemplate<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self::Some(value)
    }
}

impl<T: Template> Template for OptionTemplate<T> {
    type Output = Option<T::Output>;

    #[inline]
    #[cfg_attr(any(debug_assertions, feature = "debug"), track_caller)]
    fn build_template(&self, context: &mut TemplateContext) -> ZlimResult<Self::Output> {
        match self {
            Self::Some(template) => Ok(Some(template.build_template(context)?)),
            Self::None => Ok(None),
        }
    }

    #[inline]
    fn clone_template(&self) -> Self {
        match self {
            Self::Some(template) => Self::Some(template.clone_template()),
            Self::None => Self::None,
        }
    }
}

// -----------------------------------------------------------------------------
// VecTemplate

/// A [`Template`] of a [`Vec`].
pub struct VecTemplate<T>(pub Vec<T>);

impl<T> Default for VecTemplate<T> {
    #[inline]
    fn default() -> Self {
        Self(Vec::new())
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
