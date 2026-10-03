//! Composing a scene, and editing the templates it carries.

use core::any::TypeId;
use core::marker::PhantomData;

use zlim_core::component::Component;
use zlim_core::error::ZlimResult;
use zlim_core::template::TemplateContext;
use zlim_core::template::{ErasedTemplate, FnTemplate, IntoTemplate, Template, TemplateEffect};

use crate::dependency::SceneDependencies;
use crate::resolved::ResolvedScene;
use crate::scene::{ResolveContext, Scene};
use crate::scene_list::SceneList;

// -----------------------------------------------------------------------------
// SceneScope

/// A scene resolved in an entity scope of its own.
///
/// A scope is what keeps two `#Name`s of the same name apart: in zlim a name is identified by the
/// macro invocation that produced it (see [`EntityReference`]), and a nested scope is a new
/// invocation, so wrapping a scene in this type is how a composition says "this is its own scope".
/// Resolving one is otherwise transparent.
///
/// [`EntityReference`]: zlim_core::template::EntityReference
#[must_use]
pub struct SceneScope<S: Scene>(pub S);

impl<S: Scene> Scene for SceneScope<S> {
    #[inline]
    fn resolve(self, context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        self.0.resolve(context, scene)
    }

    #[inline]
    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        self.0.register_dependencies(dependencies);
    }
}

impl<S: Scene> SceneList for SceneScope<S> {
    #[inline]
    fn resolve_list(
        self,
        context: &mut ResolveContext,
        scenes: &mut Vec<ResolvedScene>,
    ) -> ZlimResult<()> {
        let mut scene = ResolvedScene::new();
        self.resolve(context, &mut scene)?;
        scenes.push(scene);
        Ok(())
    }

    #[inline]
    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        self.0.register_dependencies(dependencies);
    }
}

impl<S: Scene> From<SceneScope<S>> for Option<Box<dyn Scene>> {
    #[inline]
    fn from(value: SceneScope<S>) -> Self {
        Some(Box::new(value))
    }
}

// -----------------------------------------------------------------------------
// SceneListScope

/// A scene list resolved in an entity scope of its own.
///
/// See [`SceneScope`]; this is the list-shaped version of it.
#[must_use]
pub struct SceneListScope<L: SceneList>(pub L);

impl<L: SceneList> SceneList for SceneListScope<L> {
    #[inline]
    fn resolve_list(
        self,
        context: &mut ResolveContext,
        scenes: &mut Vec<ResolvedScene>,
    ) -> ZlimResult<()> {
        self.0.resolve_list(context, scenes)
    }

    #[inline]
    fn register_dependencies(&self, dependencies: &mut SceneDependencies) {
        self.0.register_dependencies(dependencies);
    }
}

impl<L: SceneList> From<SceneListScope<L>> for Option<Box<dyn SceneList>> {
    #[inline]
    fn from(value: SceneListScope<L>) -> Self {
        Some(Box::new(value))
    }
}

// -----------------------------------------------------------------------------
// SceneFunction

/// A [`Scene`] driven by a function.
///
/// This is what a scene that is composed at runtime — by a parser, or by code that patches a
/// description — uses instead of a type of its own.
pub struct SceneFunction<F: FnOnce(&mut ResolveContext, &mut ResolvedScene)>(pub F);

impl<F> Scene for SceneFunction<F>
where
    F: FnOnce(&mut ResolveContext, &mut ResolvedScene) + Send + Sync + 'static,
{
    #[inline]
    fn resolve(self, context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        (self.0)(context, scene);
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// InsertTemplate

/// A scene that replaces the canonical template of a type.
///
/// Scenes are composed one part at a time, and the parts usually do not know about each other: when
/// two of them describe the same component, the later part has to *replace* what the earlier one
/// stored rather than be applied on top of it. This is the scene-shaped form of
/// [`ResolvedScene::insert_template`].
///
/// A template that is pushed rather than inserted — one that is only applied, never edited — is
/// simply a [`Scene`] of its own, such as [`FnTemplate`].
///
/// [`FnTemplate`]: zlim_core::template::FnTemplate
pub struct InsertTemplate {
    /// The [`TypeId`] the template is stored under.
    pub type_id: TypeId,

    /// The template to store.
    pub template: Box<dyn ErasedTemplate>,
}

impl InsertTemplate {
    /// Creates a scene that stores `template` as the canonical template of its type.
    #[inline]
    pub fn new<T>(template: T) -> Self
    where
        T: Template<Output: TemplateEffect> + Send + Sync + 'static,
    {
        Self {
            type_id: TypeId::of::<T>(),
            template: Box::new(template),
        }
    }
}

impl Scene for InsertTemplate {
    #[inline]
    fn resolve(self, _context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        scene.insert_erased_template(self.type_id, self.template);
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// InitTemplate

/// A scene that makes sure the canonical template of `T` exists, without describing it.
///
/// The template is created from its [`Default`], which is what a later part of the composition — or
/// a patch — edits in place.
pub struct InitTemplate<T>(PhantomData<T>);

impl<T> InitTemplate<T> {
    /// Creates a scene that initialises the template of `T`.
    #[inline]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T> Default for InitTemplate<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Scene for InitTemplate<T>
where
    T: Template<Output: TemplateEffect> + Default + Send + Sync + 'static,
{
    #[inline]
    fn resolve(self, _context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        scene.get_or_insert_template::<T>();
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// TemplatePatch

/// A scene that patches the canonical template of `T` with a function.
///
/// The template is created from its [`Default`] if the composition has not described it already, and
/// the function then edits it in place. This is what the [`PatchIntoTemplate`] and [`PatchTemplate`]
/// traits build.
///
/// ```rust
/// use zlim_scene::{PatchTemplate, ResolveContext, ResolvedScene, Scene};
/// use zlim_core::template::{Template, TemplateContext};
/// use zlim_core::error::ZlimResult;
/// use zlim_core::derive::Component;
///
/// #[derive(Component, Clone, Default, Debug, PartialEq)]
/// struct Scale(f32);
///
/// let mut scene = ResolvedScene::new();
///
/// // `Scale` is `Clone + Default`, so it is its own template; the patch edits a default one.
/// Scale::patch_template(|scale, _context| scale.0 = 2.5)
///     .resolve(&mut ResolveContext::new(), &mut scene)
///     .unwrap();
///
/// assert_eq!(scene.component_templates().len(), 1);
/// ```
pub struct TemplatePatch<F, T>(pub F, pub PhantomData<T>);

impl<F, T> Scene for TemplatePatch<F, T>
where
    F: FnOnce(&mut T, &mut ResolveContext) + Send + Sync + 'static,
    T: Template<Output: TemplateEffect> + Default + Send + Sync + 'static,
{
    #[inline]
    fn resolve(self, context: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        let template = scene.get_or_insert_template::<T>();
        (self.0)(template, context);
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// PatchIntoTemplate

/// Patches the canonical template of a type that has one, through [`IntoTemplate`].
///
/// ```rust
/// use zlim_scene::{PatchIntoTemplate, ResolveContext, ResolvedScene, Scene};
/// use zlim_core::derive::Component;
///
/// #[derive(Component, Clone, Default)]
/// struct Scale(f32);
///
/// let mut scene = ResolvedScene::new();
///
/// // The patch function sees `Scale`'s template, which is `Scale` itself here.
/// Scale::patch(|scale, _context| scale.0 = 3.0)
///     .resolve(&mut ResolveContext::new(), &mut scene)
///     .unwrap();
/// ```
///
/// [`IntoTemplate`]: zlim_core::template::IntoTemplate
pub trait PatchIntoTemplate {
    /// The [`Template`] that the patch edits.
    type Template;

    /// Turns `func` into a [`Scene`] that patches the template of this type.
    fn patch<F>(func: F) -> TemplatePatch<F, Self::Template>
    where
        F: FnOnce(&mut Self::Template, &mut ResolveContext);
}

impl<G: IntoTemplate> PatchIntoTemplate for G {
    type Template = G::Template;

    #[inline]
    fn patch<F>(func: F) -> TemplatePatch<F, Self::Template>
    where
        F: FnOnce(&mut Self::Template, &mut ResolveContext),
    {
        TemplatePatch(func, PhantomData)
    }
}

// -----------------------------------------------------------------------------
// PatchTemplate

/// Patches a template that is written by hand, rather than the one of the type it produces.
pub trait PatchTemplate: Sized {
    /// Turns `func` into a [`Scene`] that patches this template.
    fn patch_template<F>(func: F) -> TemplatePatch<F, Self>
    where
        F: FnOnce(&mut Self, &mut ResolveContext);
}

impl<T: Template> PatchTemplate for T {
    #[inline]
    fn patch_template<F>(func: F) -> TemplatePatch<F, Self>
    where
        F: FnOnce(&mut Self, &mut ResolveContext),
    {
        TemplatePatch(func, PhantomData)
    }
}

// -----------------------------------------------------------------------------
// FnTemplate

impl<F, O> Scene for FnTemplate<F, O>
where
    F: Fn(&mut TemplateContext) -> ZlimResult<O> + Clone + Send + Sync + 'static,
    O: Component,
{
    #[inline]
    fn resolve(self, _: &mut ResolveContext, scene: &mut ResolvedScene) -> ZlimResult<()> {
        scene.push_template(self);
        Ok(())
    }
}

// -----------------------------------------------------------------------------
