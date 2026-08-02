#![expect(clippy::module_inception, reason = "better structure")]

use crate::entity::EntityMapper;

#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a `Component`",
    label = "invalid `Component`",
    note = "consider annotating `{Self}` with `#[derive(Component)]`"
)]
pub trait Component: Default + Send + Sync + 'static {
    const NO_ENTITY: bool = false;

    #[inline]
    fn map_entities<E: EntityMapper>(_: &mut Self, _: &mut E) {}
}
