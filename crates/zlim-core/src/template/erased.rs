//! The type-erased form a composition stores a template in.

use core::any::Any;

use zlim_error::ZlimResult;
use zlim_reflect::Reflect;

use crate::bundle::BundleWriter;
use crate::template::{Template, TemplateContext, TemplateEffect};

// -----------------------------------------------------------------------------
// ErasedTemplate

/// A [`Template`] whose output is a [`TemplateEffect`], in a form a composition can store.
///
/// This is the object-safe, type-erased half of the pair: a crate that stores a composition keeps
/// its templates as `Box<dyn ErasedTemplate>` — `zlim-scene` does, for a resolved scene — and every
/// [`Template`] whose output is a [`TemplateEffect`] is one through the blanket implementation
/// below.
///
/// [`Any`] is what makes a stored template addressable by type: a canonical template is stored under
/// its own [`TypeId`], so a later part of the composition can replace it.
///
/// [`TypeId`]: core::any::TypeId
pub trait ErasedTemplate: Any + Send + Sync {
    /// Builds the template and applies its output to the entity of `context`, through `writer`.
    fn apply(&self, context: &mut TemplateContext, writer: &mut BundleWriter) -> ZlimResult<()>;

    /// Duplicates this template, which is what lets a cached composition be reused.
    fn clone_template(&self) -> Box<dyn ErasedTemplate>;
}

impl<T> ErasedTemplate for T
where
    T: Template + Send + Sync + 'static,
    T::Output: TemplateEffect,
{
    fn apply(&self, context: &mut TemplateContext, writer: &mut BundleWriter) -> ZlimResult<()> {
        let output = self.build_template(context)?;
        TemplateEffect::apply(output, context, writer);
        Ok(())
    }

    fn clone_template(&self) -> Box<dyn ErasedTemplate> {
        Box::new(Template::clone_template(self))
    }
}

// -----------------------------------------------------------------------------
// ReflectTemplate

/// [`ErasedTemplate`] + [`Reflect`]
pub trait ReflectTemplate: ErasedTemplate {
    /// Gets a shared reflect reference from self.
    fn as_reflect(&self) -> &dyn Reflect;

    /// Gets a mutable shared reflect reference from self.
    fn as_reflect_mut(&mut self) -> &mut dyn Reflect;

    /// Convert self into a reflect value.
    fn into_reflect(self: Box<Self>) -> Box<dyn Reflect>;

    /// Applies this template to the entity of `context`, taking ownership of it.
    fn apply_owned(
        self: Box<Self>,
        context: &mut TemplateContext,
        writer: &mut BundleWriter,
    ) -> ZlimResult<()>;

    fn clone_reflect_template(&self) -> Box<dyn ReflectTemplate>;
}

impl<T: ErasedTemplate + Reflect + Clone> ReflectTemplate for T {
    fn as_reflect(&self) -> &dyn Reflect {
        self
    }

    fn as_reflect_mut(&mut self) -> &mut dyn Reflect {
        self
    }

    fn into_reflect(self: Box<Self>) -> Box<dyn Reflect> {
        self
    }

    fn apply_owned(
        self: Box<Self>,
        context: &mut TemplateContext,
        writer: &mut BundleWriter,
    ) -> ZlimResult<()> {
        <Self as ErasedTemplate>::apply(&*self, context, writer)
    }

    fn clone_reflect_template(&self) -> Box<dyn ReflectTemplate> {
        Box::new(T::clone(self))
    }
}

// -----------------------------------------------------------------------------
