//! What a template's output does when the template is applied.

use crate::bundle::{Bundle, BundleWriter};
use crate::template::TemplateContext;

// -----------------------------------------------------------------------------
// TemplateEffect

/// What a template does to the entity it was built for.
///
/// A template produces a value; the effect is what turns that value into part of the entity.
/// The common case is a [`Component`], which is simply written into the entity:
///
/// A [`Bundle`] hands every component it carries to the writer, so a description can spell out
/// several components at once without the bundle being a component itself.
///
/// A template whose output is neither can implement this directly, which is how a description
/// expresses something that is not data — a name to register, an observer to attach, and so on.
/// Such an effect is also the place to reach for the world: the context holds the entity the
/// template is being applied to.
///
/// [`Component`]: crate::component::Component
pub trait TemplateEffect {
    /// Applies this effect to the entity of `context`.
    ///
    /// The `writer` is the one that will be written to the entity once every template has been
    /// applied, so effects that produce component data should push it there rather than insert it
    /// themselves.
    ///
    /// This is deliberately not a method: only the crate that applies a composition applies an
    /// effect, and a method would put `apply` in the completion of every component there is.
    /// Implementing the trait is the public half; call it as [`TemplateEffect::apply`], not with
    /// method syntax.
    fn apply(this: Self, context: &mut TemplateContext, writer: &mut BundleWriter);
}

/// A [`TemplateEffect`] that does nothing.
///
/// This is the output of a template that acts through the context rather
/// than through the entity's data — one that attaches an observer, for instance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmptyTemplateEffect;

impl TemplateEffect for EmptyTemplateEffect {
    #[inline]
    fn apply(_this: Self, _context: &mut TemplateContext, _writer: &mut BundleWriter) {}
}

impl<B: Bundle> TemplateEffect for B {
    #[inline]
    fn apply(this: Self, context: &mut TemplateContext, writer: &mut BundleWriter) {
        writer.push(this, Some(context.entity.world().components()));
    }
}
