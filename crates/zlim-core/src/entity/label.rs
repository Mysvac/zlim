use crate::{define_label, label::Interned};

define_label! {
    /// A strongly-typed class of labels used to identify an [`Entity`].
    ///
    /// Prefer defining your own label enums/structs with
    /// `#[derive(EntityLabel)]` for stable, explicit schedule routing.
    ///
    /// [`Entity`]: crate::entity
    #[diagnostic::on_unimplemented(
        note = "consider annotating `{Self}` with `#[derive(EntityLabel)]`"
    )]
    EntityLabel,
    ENTITY_LABEL_INTERNER
}

/// A shorthand for `Interned<dyn EntityLabel>`.
pub type InternedEntityLabel = Interned<dyn EntityLabel>;
