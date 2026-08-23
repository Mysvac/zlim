use zlim_core::label::Interned;

zlim_core::define_label!(
    /// A strongly-typed class of labels used to identify an [`App`].
    ///
    /// Prefer defining your own label enums/structs with
    /// `#[derive(AppLabel)]` for stable, explicit schedule routing.
    #[diagnostic::on_unimplemented(
        note = "consider annotating `{Self}` with `#[derive(AppLabel)]`"
    )]
    AppLabel,
    APP_LABEL_INTERNER
);

/// A shorthand for `Interned<dyn AppLabel>`.
pub type InternedAppLabel = Interned<dyn AppLabel>;
