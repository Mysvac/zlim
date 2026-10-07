//! The [`Template`] trait, and everything needed to build values with a world.
//!
//! A [`Template`] is something that, given the context of a spawn — the entity it is being applied
//! to, its world, and the entity references of the scene it belongs to — produces a
//! [`Template::Output`]. It is what makes it possible to *describe* a value whose construction
//! needs a world: an entity reference, an asset handle, or anything else that has to be looked up,
//! loaded or spawned. A template is therefore:
//!
//! - **Repeatable**: building it does not consume it, so one "baked" description can spawn as many
//!   instances as wanted.
//! - **Cloneable**: [`Template::clone_template`] duplicates it, which is what lets a scene be
//!   patched and cached with copy-on-write semantics.
//! - **Often serializable**: templates are usually plain data, which is what makes them good scene
//!   files.
//!
//! [`IntoTemplate`] is the other half: it names the canonical template of a type, so that a
//! component holding a templateable field can be described by describing its fields.
//!
//! What the built value does with the entity is a [`TemplateEffect`], and that is what makes a template
//! storable in a type-erased form: [`ErasedTemplate`] is the object-safe half of the pair, which is
//! what a crate that keeps a composition of templates stores (`zlim-scene` keeps a resolved scene
//! that way).
//!
//! # Templates that are derived automatically
//!
//! Every [`Clone`] type is its own [`Template`], and every `Clone + Default` type is its own
//! [`IntoTemplate`]: such a type is described by itself, and building it just clones it.
//!
//! Most types therefore already have a template, and a type only needs a template of its own —
//! derived with `#[derive(IntoTemplate)]`, or written by hand — when one of its fields is itself
//! described by a template. See [`IntoTemplate`] for details.
//!
//! # Usage
//!
//! [`Template`] is currently primarily used by the scene system, so you can
//! see concrete applications of it in `zlim_scene`.
//!
//! Put simply, a scene is a sequence of templates, and the top-level templates
//! usually produce values of a component / bundle type. Applying a scene works
//! in three steps: first an empty entity is created, then [`Template::build_template`]
//! produces the components into a `BundleWriter`, and finally all components are
//! written into the entity in one go.
//!
//! Multi-entity scenes are a nesting of the pattern above; see `zlim_scene` for
//! the concrete details.

mod collections;
mod context;
mod effect;
mod entity;
mod erased;
mod function;
mod tuple;
mod value;

pub use collections::{BuiltInTemplate, OptionTemplate, VecTemplate};
pub use context::{EntityReference, EntityReferences};
pub use context::{TemplateContext, TemplateEntityMapper};
pub use effect::{EmptyTemplateEffect, TemplateEffect};
pub use entity::EntityTemplate;
pub use erased::ErasedTemplate;
pub use function::{FnTemplate, template};
pub use tuple::TemplateTuple;
pub use value::ComponentTemplate;

pub use crate::derive::IntoTemplate;

use crate::error::ZlimResult;

// -----------------------------------------------------------------------------
// Template

/// A description of a value that is built with the context of the entity it belongs to.
///
/// See the [module documentation](self) for what a template is and how the blanket
/// implementation relates to [`IntoTemplate`].
pub trait Template {
    /// The type of value this template produces.
    type Output;

    /// Builds the value of this template for the entity of `context`.
    fn build_template(&self, context: &mut TemplateContext) -> ZlimResult<Self::Output>;

    /// Duplicates this template.
    ///
    /// This is deliberately separate from [`Clone`]: the blanket implementation of [`Template`]
    /// covers every `Clone` type, so a template that provides its own implementation cannot also
    /// implement [`Clone`].
    fn clone_template(&self) -> Self;
}

// -----------------------------------------------------------------------------
// IntoTemplate

/// The canonical [`Template`] of a type.
///
/// Types that can be produced by a template name it here, which is what lets
/// a type be described through the templates of its fields. It is best thought
/// of as an alternative to [`Default`] for types whose construction needs a
/// world: see the [module documentation](self).
///
/// # Derive Macro
///
/// [`IntoTemplate`] can be derived for types whose fields _also_ implement [`IntoTemplate`]:
///
/// ```rust, ignore
/// #[derive(IntoTemplate)]
/// struct Player {
///     image: Handle<Image>
/// }
/// ```
///
/// Deriving [`IntoTemplate`] will generate a [`Template`] type for the deriving type.
/// The example above would generate a `PlayerTemplate` like this:
///
/// ```rust, ignore
/// struct PlayerTemplate {
///     image: HandleTemplate<Image>,
/// }
///
/// impl IntoTemplate for Player {
///     type Template = PlayerTemplate;
///
///     fn into_template(self) -> Self::Template {
///         PlayerTemplate { image: self.image.into_template() }
///     }
/// }
///
/// impl Template for PlayerTemplate {
///     type Output = Player;
///
///     fn build_template(&self, context: &mut TemplateContext) -> Result<Self::Output> {
///         Ok(Player { image: self.image.build_template(context)?, })
///     }
///
///     fn clone_template(&self) -> Self {
///         PlayerTemplate { image: self.image.clone_template(), }
///     }
/// }
/// ```
///
/// # Macro Attributes
///
/// | Field | `into_template` uses |
/// |-------|----------------------|
/// | *(none)* | the field type's own [`IntoTemplate`] |
/// | `#[template(built_in)]` | the field type's [`BuiltInTemplate`] |
/// | `#[template(SomeTemplate)]` | `Into<SomeTemplate>` for the field type |
/// | `#[template(into = path)]` | Custom `Into::into` implementation |
///
/// ## Custom Template
///
/// [`IntoTemplate`] derives can specify custom templates to use instead of a
/// canonical [`IntoTemplate`]:
///
/// ```rust, ignore
/// #[derive(IntoTemplate)]
/// struct Counter {
///     #[template(Always10)]
///     count: usize
/// }
///
/// #[derive(Default)]
/// struct Always10;
///
/// // The field converts into the named template, so `into_template` needs this.
/// impl From<usize> for Always10 {
///     fn from(_: usize) -> Self { Self }
/// }
///
/// impl Template for Always10 {
///     type Output = usize;
///
///     fn build_template(&self, ctx: &mut TemplateContext) -> Result<Self::Output> { Ok(10) }
///     fn clone_template(&self) -> Self { Always10 }
/// }
/// ```
///
/// ## BuiltIn Template
///
/// [`IntoTemplate`] is automatically implemented for anything that is [`Default`] and [`Clone`].
/// "Built in" collection types like [`Option`] and [`Vec`] pick up this "blanket" implementation,
/// which is generally a good thing because it means these collection types work with [`IntoTemplate`]
/// derives by default.
///
/// However if the items in the collection have a custom [`IntoTemplate`] impl (ex: a manual implementation
/// like `Handle<T>` for assets or an explicit [`IntoTemplate`] derive), then relying on a [`Default`] /
/// [`Clone`] implementation doesn't work, as that won't run the template logic!
///
/// ```rust, ignore
/// type T = <Handle<Image> as IntoTemplate>::template;
/// // ↑ T = HandleTemplate<Image> ✅️
///
/// type T = <Option<Handle<Image>> as IntoTemplate>::template;
/// // ↑ T = Option<Handle<Image>> ❌️
/// ```
///
/// This is where [`BuiltInTemplate`] comes in:
///
/// ```rust, ignore
/// type T = <Option<Handle<Image>> as BuiltInTemplate>::template;
/// // ↑ T = OptionTemplate<HandleTemplate<Image>> ✅️
/// ```
///
/// If you are deriving [`IntoTemplate`] and you have a "built in" type like [`Option<Handle<T>>`]
/// which has custom template logic, annotate it with the `template(built_in)` attribute to use
/// [`BuiltInTemplate`] instead of [`IntoTemplate`]:
///
/// ```rust, ignore
/// #[derive(IntoTemplate)]
/// struct Widget {
///     #[template(built_in)]
///     image: Option<Handle<Image>>
/// }
/// ```
///
/// ### It rewrites one layer only
///
/// `built_in` applies to a container with a *single* layer of nesting: [`Option<T>`] and [`Vec<T>`].
/// It rewrites exactly that layer, turning the field into `OptionTemplate<T::Template>` and
/// `VecTemplate<T::Template>` respectively:
///
/// ```rust, ignore
/// // Option<T>  ->  OptionTemplate<T::Template>
/// #[template(built_in)] field: Option<Handle<Image>>
/// //                        -> OptionTemplate<HandleTemplate<Image>> ✅️
///
/// // Vec<T>  ->  VecTemplate<T::Template>
/// #[template(built_in)] field: Vec<Handle<Image>>
/// //                        -> VecTemplate<HandleTemplate<Image>> ✅️
/// ```
///
/// It does **not** reach further down, so a doubly-nested container is only half rewritten: the
/// element is converted with its own [`IntoTemplate`], and a collection's [`IntoTemplate`] is the
/// blanket one, whose template is the collection itself.
///
/// ```rust, ignore
/// #[template(built_in)] field: Option<Option<Handle<Image>>>
/// //                        -> OptionTemplate<Option<Handle<Image>>> ❌️
/// ```
///
/// At present, we use [`SpecializeTemplate`] to constrain it, the nested un-specialized
/// type cannot be annotated with `built_in`. The declaration is similar to :
///
/// `impl<T: IntoTemplate + SpecializeTemplate> BuiltInTemplate for Option<T> {}`.
///
/// For a type nested more deeply than that, name the target template explicitly and, when the
/// conversion the derive would use does not produce it, name the function too with `into`:
///
/// ```rust, ignore
/// #[derive(IntoTemplate)]
/// struct Widget {
///     #[template(OptionTemplate<OptionTemplate<HandleTemplate<Image>>>, into = nest_image)]
///     image: Option<Option<Handle<Image>>>,
/// }
///
/// fn nest_image(image: Option<Option<Handle<Image>>>) -> OptionTemplate<OptionTemplate<HandleTemplate<Image>>> {
///     // ...
/// }
/// ```
pub trait IntoTemplate: Sized {
    /// The template that produces this type.
    type Template: Template<Output = Self>;

    /// Describes `self` with its canonical template.
    ///
    /// This is the conversion that a derived template performs field by field:
    /// - a field whose type has a template of its own is converted with its own [`IntoTemplate`],
    /// - a `#[template(built_in)]` field with its type's [`BuiltInTemplate`],
    /// - and a field naming a template explicitly with [`Into`].
    fn into_template(self) -> Self::Template;
}

/// Marks a type whose template is *not* itself.
///
/// # 1. As a marker: `IntoTemplate::Template` is not `Self`
///
/// [`Template`] is implemented for every [`Clone`] type and [`IntoTemplate`] for every
/// `Clone + Default` one, so a type that provides either by hand has to stay out of those
/// implementations — exactly one of [`Clone`] or [`Unpin`] must not hold for it.
///
/// Implementing this trait for a type says that its hand-written [`IntoTemplate`] produces a
/// template other than the type itself — `X` is described by `XTemplate`, not by `X`. That is what
/// a type which decomposes into fields does, and it is what makes the type usable as the element of
/// a [`BuiltInTemplate`] container: rewriting `Option<X>` into `OptionTemplate<XTemplate>` is only
/// meaningful when `XTemplate` is not `X`.
///
/// # 2. As the `Unpin` opt-out
///
/// A type that has to be [`Clone`] — an [`EntityTemplate`], or a type with a derived template — is
/// instead made not [`Unpin`], with
///
/// ```ignore
/// impl Unpin for MyTemplate where for<'a> [()]: SpecializeTemplate {}
/// ```
///
/// whose condition never holds, because this trait is not implemented for `[()]`. The type is then
/// considered not [`Unpin`], and the hand-written implementation stands on its own.
///
/// This is a use of the trait's *absence*, and it says nothing about the type's template: a type
/// that only needs the opt-out leaves the trait unimplemented. Nothing implements it for the sake of
/// the condition — the condition exists so that an unsatisfied one is reported with a useful
/// message.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no template of its own: it is described by the template every \
               `Clone + Default` type gets, which is the type itself",
    label = "this type is its own template",
    note = "implement `SpecializeTemplate` for a type whose `IntoTemplate` produces a template \
            other than the type itself — that is what a type which decomposes into fields does. \
            `#[derive(IntoTemplate)]` records it, and so must a hand-written `IntoTemplate`: \
            `impl SpecializeTemplate for MyType ()`",
    note = "the trait is not needed to opt a type out of the blanket implementations. That use is \
            the `Unpin` condition `for<'a> [()]: SpecializeTemplate`, which relies on the trait \
            *not* being implemented, so a type that only needs the opt-out leaves it alone",
    note = "`#[template(built_in)]` requires it of the container's inner type: `Option<T>` and \
            `Vec<T>` are only rewritten into `OptionTemplate<T::Template>` and \
            `VecTemplate<T::Template>` when `T` has a dedicated template, so `built_in` means \
            nothing without one. If the field holds plain values, drop `built_in` and let the \
            container be its own template"
)]
pub trait SpecializeTemplate: Sized {}

// -----------------------------------------------------------------------------
// Blanket implementations

/// A `Clone` type is its own template, and building it clones it.
///
/// This is what gives most types a template; the types that are kept out
/// of this implementation are documented by [`SpecializeTemplate`].
impl<T: Clone + Unpin> Template for T {
    type Output = T;

    #[inline]
    fn build_template(&self, _context: &mut TemplateContext) -> ZlimResult<Self::Output> {
        Ok(self.clone())
    }

    #[inline]
    fn clone_template(&self) -> Self {
        self.clone()
    }
}

/// A `Default + Clone` type is described by itself:
/// the template of a type without any templateable field is the type.
///
/// A type that is not `Default + Clone`, or that needs a template of
/// its own, has to implement this trait by hand — or derive it, which
/// is what `#[derive(IntoTemplate)]` is for.
impl<T: Clone + Default + Unpin> IntoTemplate for T {
    type Template = T;

    #[inline(always)]
    fn into_template(self) -> Self::Template {
        self
    }
}

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {

    use crate::derive::Resource;
    use crate::entity::{EntityId, EntityMap};
    use crate::error::ZlimResult;
    use crate::world::World;

    use super::Template;
    use super::collections::{BuiltInTemplate, OptionTemplate, VecTemplate};
    use super::context::{EntityReference, EntityReferences, TemplateContext};
    use super::entity::EntityTemplate;
    use super::function::template;
    use super::tuple::TemplateTuple;

    // -----------------------------------------------------------------------------
    // Types

    /// A plain `Default + Clone` type: it has a template of its own.
    #[derive(Clone, Default, Debug, PartialEq)]
    struct Scale(f32);

    /// A resource that a template reads through its context.
    #[derive(Resource, Debug, PartialEq)]
    struct Offset(f32);

    // -----------------------------------------------------------------------------
    // Helpers

    /// Builds `source` in a fresh world, with a fresh table of references.
    fn build<T: Template>(source: &T) -> T::Output {
        let mut world = World::alloc();
        build_in(&mut world, source)
    }

    /// Builds `source` in an existing world, and returns what it produced.
    fn build_in<T: Template>(world: &mut World, source: &T) -> T::Output {
        build_result_in(world, source).expect("the template is expected to build")
    }

    /// Builds `source` for a fresh entity of `world`, with a fresh table of references.
    fn build_result_in<T: Template>(world: &mut World, source: &T) -> ZlimResult<T::Output> {
        let mut references = EntityReferences::new();
        let mut entities = EntityMap::new();
        let mut entity = world.spawn_empty(None);
        let mut context = TemplateContext::new(&mut entity, &mut references, &mut entities);
        source.build_template(&mut context)
    }

    // -----------------------------------------------------------------------------
    // Tests

    /// A `Clone` type is its own template, and building it clones it.
    #[test]
    fn a_clone_type_is_its_own_template() {
        assert_eq!(build(&Scale(3.5)), Scale(3.5));
        assert_eq!(build(&Scale::default()), Scale::default());
    }

    /// A template that names an entity builds that entity, and one that names none fails.
    #[test]
    fn an_entity_template_builds_the_entity_it_names() {
        let mut world = World::alloc();
        let entity = world.spawn_empty(None).id();

        assert_eq!(build(&EntityTemplate::Entity(entity)), entity);
        assert!(build_result_in(&mut world, &EntityTemplate::None).is_err());
    }

    /// References with the same identity are equal, and different ones are not.
    #[test]
    fn references_are_identified_by_their_invocation() {
        let first = EntityReference::new("scene.rs", 12, 4, 0, 0);

        assert_eq!(first, EntityReference::new("scene.rs", 12, 4, 0, 0));
        assert_ne!(first, EntityReference::new("scene.rs", 12, 4, 1, 0));
        assert_ne!(first, EntityReference::new("scene.rs", 12, 4, 0, 1));
        assert_ne!(first, EntityReference::new("other.rs", 12, 4, 0, 0));
    }

    /// A name resolves to the entity it was bound to, and an unbound name is an error rather than a new
    /// entity.
    #[test]
    fn a_name_resolves_to_the_entity_it_was_bound_to() {
        let first = EntityReference::new("scene.rs", 20, 8, 0, 0);
        let same = EntityReference::new("scene.rs", 20, 8, 0, 0);
        let other = EntityReference::new("scene.rs", 20, 8, 1, 0);

        let mut world = World::alloc();
        let mut references = EntityReferences::new();
        let mut entities = EntityMap::new();
        let mut entity = world.spawn_empty(None);
        let declared = entity.id();
        let mut context = TemplateContext::new(&mut entity, &mut references, &mut entities);

        // Nothing declared the name yet.
        assert!(context.resolve_entity(first).is_err());
        assert!(context.references.is_empty());

        // The same name is the same entity, however it is spelled.
        context.references.set(first, declared);
        assert_eq!(context.resolve_entity(first).unwrap(), declared);
        assert_eq!(context.resolve_entity(same).unwrap(), declared);

        // A different name is a different entity, and binding is what makes it known.
        assert!(context.resolve_entity(other).is_err());
        context.references.set(other, declared);
        assert_eq!(context.resolve_entity(other).unwrap(), declared);
        assert_eq!(context.references.len(), 2);
    }

    /// The collection templates build their elements one by one.
    #[test]
    fn a_collection_template_builds_its_elements() {
        let mut world = World::alloc();
        let entity = world.spawn_empty(None).id();

        let some: OptionTemplate<EntityTemplate> = Some(EntityTemplate::Entity(entity)).into();
        assert_eq!(build(&some), Some(entity));

        let none: OptionTemplate<EntityTemplate> = OptionTemplate(None);
        assert_eq!(build(&none), None);

        let many = VecTemplate(vec![
            EntityTemplate::Entity(entity),
            EntityTemplate::Entity(entity),
        ]);
        assert_eq!(build(&many), vec![entity, entity]);

        // An element that cannot be built fails the whole collection.
        let broken = VecTemplate(vec![EntityTemplate::None]);
        assert!(build_result_in(&mut world, &broken).is_err());
    }

    /// The built-in template of a collection is the template of its element type.
    #[test]
    fn the_built_in_template_of_a_collection_is_the_template_of_its_element() {
        fn assert_template<T: Template<Output = Option<EntityId>>>() {}

        assert_template::<<Option<EntityId> as BuiltInTemplate>::Template>();
    }

    /// A tuple template builds a tuple of its outputs.
    #[test]
    fn a_tuple_template_builds_a_tuple() {
        let mut world = World::alloc();
        let entity = world.spawn_empty(None).id();

        let pair = TemplateTuple((
            EntityTemplate::Entity(entity),
            EntityTemplate::Entity(entity),
        ));
        assert!(build(&pair) == (entity, entity));

        // The first element that fails to build fails the whole tuple.
        let broken = TemplateTuple((EntityTemplate::Entity(entity), EntityTemplate::None));
        assert!(build_result_in(&mut world, &broken).is_err());
    }

    /// A function template calls its function for every build.
    #[test]
    fn a_function_template_calls_its_function() {
        let scale = template(|_context| Ok(2.0_f32));
        assert_eq!(build(&scale), 2.0);
    }

    /// The context gives a template access to the world of its entity.
    #[test]
    fn a_template_can_read_a_resource() {
        let mut world = World::alloc();
        world.insert_resource(Offset(1.5));

        let offset = template(|context| Ok(context.resource::<Offset>().0));
        assert_eq!(build_in(&mut world, &offset), 1.5);
    }
}
