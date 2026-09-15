//! Per-asset change tracking: the [`AssetChanged`] query filter and the resource behind it.
//!
//! [`Assets<A>`] does not touch the ECS on every change: additions, modifications and removals
//! are queued as [`AssetEvent`]s, and the `asset_events` job turns them into ticks in a
//! per-asset-type `AssetChanges` resource while flushing the events into their message queue.
//! [`AssetChanged<A>`] is the query filter built on that table — it keeps the assets whose
//! component was touched since the query's last run.
//!
//! `A` is an [`AssetComponent`] — a component that carries the [`AssetId`] of the asset it refers
//! to — so the filter matches the entities that *reference* an asset, not the asset storage
//! itself. The resource and the filter are created per asset type by the asset plugin; using the
//! filter for a type that was never registered logs an error and then matches nothing.
//!
//! [`Assets<A>`]: crate::assets::Assets
//! [`AssetEvent`]: crate::event::AssetEvent
//! [`AssetComponent`]: crate::asset::AssetComponent
//! [`AssetId`]: crate::ident::AssetId
#![expect(unsafe_code, reason = "impl QueryFilter")]

use core::any::TypeId;
use core::marker::PhantomData;
use core::ptr::NonNull;

use zlim_core::borrow::ResMut;
use zlim_core::derive::Resource;
use zlim_core::entity::EntityId;
use zlim_core::job_fn;
use zlim_core::message::{ClampTickSignal, MessageReader};
use zlim_core::query::QueryFilter;
use zlim_core::resource::ResourceDB;
use zlim_core::system::{AccessTable, ComponentAccess, FilterParamBuilder, If};
use zlim_core::table::{Table, TableRow};
use zlim_core::tick::Tick;
use zlim_core::world::{World, WorldCell};
use zlim_path::TypePath;
use zlim_utils::hash::HashMap;

use crate::asset::{Asset, AssetComponent};
use crate::ident::AssetId;

// -----------------------------------------------------------------------------
// AssetChanges

/// Change ticks of the assets of type `A`, keyed by [`AssetId`].
///
/// The table is filled by the `asset_events` job and consumed by [`AssetChanged`].
/// `last_change` records the tick of the most recent insertion, so the filter can reject a whole
/// table in one comparison before looking up the individual ids.
#[derive(TypePath, Resource)]
pub(crate) struct AssetChanges<A: Asset> {
    changed: HashMap<AssetId<A>, Tick>,
    last_change: Tick,
}

impl<A: Asset> AssetChanges<A> {
    /// Marks `asset_id` as changed at `tick`.
    pub(crate) fn insert(&mut self, asset_id: AssetId<A>, tick: Tick) {
        self.last_change = tick;
        self.changed.insert(asset_id, tick);
    }

    /// Forgets the change tick of `asset_id`.
    ///
    /// Dropping an entry is not a change: a `Removed` or `Unused` event only stops the filter
    /// from matching the asset, it does not make other assets of the same type match.
    pub(crate) fn remove(&mut self, asset_id: &AssetId<A>) {
        self.changed.remove(asset_id);
    }
}

impl<A: Asset> Default for AssetChanges<A> {
    fn default() -> Self {
        Self {
            changed: HashMap::new(),
            last_change: Tick::new(0),
        }
    }
}

// -----------------------------------------------------------------------------
// ClampAssetChangesTick

/// Clamps every recorded change tick — including `last_change` — against the `now` of each
/// [`ClampTickSignal`], so that the ticks kept in an `AssetChanges` table cannot grow without
/// bound.
#[job_fn(type = ClampAssetChangesTick<A: Asset>)]
fn clamp_asset_changes<A: Asset>(
    mut changes: If<ResMut<AssetChanges<A>>>,
    mut queue: MessageReader<ClampTickSignal>,
) {
    for ClampTickSignal { now } in queue.read() {
        let now = *now;
        let changes: &mut AssetChanges<A> = &mut changes;
        changes.last_change.clamp_with(now);
        changes.changed.values_mut().for_each(|x| x.clamp_with(now));
    }
}

// -----------------------------------------------------------------------------
// AssetChanged

/// Query filter that matches the entities whose [`AssetComponent`] references an asset of type
/// `A` that changed since the previous run of the query.
///
/// The filter mirrors [`Changed`], except that the ticks live in the `AssetChanges<A>` resource
/// instead of in the component column: an asset is considered changed as soon as an `Added` or
/// `Modified` event was drained for it since the last run. A `Removed` or `Unused` event only
/// drops the entry, so it does not by itself make the filter match.
///
/// [`Changed`]: zlim_core::query::Changed
/// [`AssetComponent`]: crate::asset::AssetComponent
pub struct AssetChanged<A: AssetComponent>(PhantomData<A>);

// -----------------------------------------------------------------------------
// QueryFilter

mod seal {
    use core::cell::UnsafeCell;
    use core::ptr::NonNull;

    use zlim_core::component::ComponentId;
    use zlim_core::resource::ResourceCell;
    use zlim_core::table::Column;
    use zlim_core::tick::Tick;

    /// Per-execution cache for `AssetChanged`: the current table's component column plus the
    /// tick window of the current system run.
    ///
    /// This mirrors the cache of `&A` in `zlim-core`: the column pointer is kept alive by the
    /// `query` module's guarantee that the table outlives the query, while the tick window is
    /// needed because the change ticks are stored per asset rather than per component column.
    pub struct AssetChangedView {
        pub(super) data: Option<NonNull<Column>>,
        pub(super) last_run: Tick,
        pub(super) this_run: Tick,
    }

    /// Static state of `AssetChanged`: where the asset components live and where the change
    /// ticks are kept.
    ///
    /// The resource is held as a raw cell because the state is built once, while the resource
    /// may be removed from the world at any later point; the filter is the only place that
    /// dereferences it, and it re-checks the cell on every call.
    #[derive(Clone, Copy)]
    pub struct AssetChangedState {
        pub(super) component: ComponentId,
        pub(super) resource: Option<&'static UnsafeCell<ResourceCell>>,
    }

    unsafe impl Sync for AssetChangedState {}
    unsafe impl Send for AssetChangedState {}
}

use crate::change::seal::{AssetChangedState, AssetChangedView};

unsafe impl<A: AssetComponent> QueryFilter for AssetChanged<A> {
    type State = AssetChangedState;
    type Cache<'world> = AssetChangedView;

    const ENABLE_ENTITY_FILTER: bool = true;

    fn build_state(w: &World) -> Self::State {
        let component = w.components().get::<A>().id;
        let ty = TypeId::of::<AssetChanges<A::Asset>>();
        let resource = w.resources().get_cell(ty);
        if resource.is_none() {
            zlim_log::error!(
                "Using the `AssetChanged<{}>` query parameter, but the corresponding \
                `AssetChanges<{}>` resource does not exist, which causes this query to always \
                fail. This may be because the asset type was not registered, or the query was \
                invoked before registration.",
                <A as TypePath>::type_name(),
                <A::Asset as TypePath>::type_name(),
            );
        }
        AssetChangedState {
            component,
            resource,
        }
    }

    unsafe fn build_cache<'w>(
        _: &Self::State,
        _: WorldCell<'w>,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Cache<'w> {
        AssetChangedView {
            data: None,
            last_run,
            this_run,
        }
    }

    fn register_filter(state: &Self::State, out: &mut Vec<FilterParamBuilder>) {
        // A filter must *push* its own branch:
        // `QueryData` only decorates the builders that filters created, so mutating
        // `out` here would register nothing and the query would match no table at all.
        let mut builder = FilterParamBuilder::new();
        builder.with(state.component);
        out.push(builder);
    }

    fn register_access(state: &Self::State, out: &mut ComponentAccess) {
        // Reading the changed-tick metadata of `A`; tick reads never conflict
        // with data access, so it is force-registered.
        out.force_reading(state.component);
    }

    fn modify_access_table(_: &Self::State, table: &mut AccessTable, strict: bool) -> bool {
        let id = ResourceDB::of::<AssetChanges<A::Asset>>().id;
        table.register_reading_res(id, strict)
    }

    unsafe fn update_table<'w>(state: &Self::State, cache: &mut Self::Cache<'w>, table: &'w Table) {
        if let Some(col) = table.get_table_col(state.component) {
            let column = unsafe { table.get_column(col) };
            cache.data = Some(NonNull::from_ref(column));
        } else {
            cache.data = None;
        }
    }

    unsafe fn filter<'w>(
        state: &Self::State,
        cache: &mut Self::Cache<'w>,
        _entity: EntityId,
        table_row: TableRow,
    ) -> bool {
        let Some(column) = cache.data else {
            return false;
        };

        let Some(cell) = state.resource else {
            return false;
        };

        // SAFETY: `build_state` stored a cell that outlives the world, so reading it here is
        // valid; the cell is re-checked below in case the resource was removed afterwards. The
        // column pointer was set by `update_table` for the table the query is currently
        // visiting, and `table_row` is valid for that table.
        unsafe {
            let cell = &*cell.get();

            let Some(cell) = cell.get_data() else {
                ::core::hint::cold_path();
                zlim_log::error_once!(
                    "The `AssetChanged<{}>` query filter was used, but the corresponding \
                    `AssetChanges<{}>` resource was removed after being inserted, which will \
                    cause subsequent queries to always fail.",
                    <A as TypePath>::type_name(),
                    <A::Asset as TypePath>::type_name(),
                );
                return false;
            };

            let changes = cell.deref::<AssetChanges<A::Asset>>();

            let change = changes.last_change;
            if cache.last_run.is_newer_than(change, cache.this_run) {
                return false; // No changes
            }

            let column = &*column.as_ptr();
            let ptr = column.get_data(table_row.0 as usize);
            let component = ptr.deref::<A>();
            let id: AssetId<A::Asset> = AssetComponent::asset_id(component);

            let Some(tick) = changes.changed.get(&id) else {
                return false;
            };

            tick.is_newer_than(cache.last_run, cache.this_run)
        }
    }
}

// -----------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use zlim_core::component::Component;
    use zlim_core::tick::Tick;
    use zlim_core::world::World;
    use zlim_path::TypePath;

    use super::{AssetChanged, AssetChanges};
    use crate::asset::AssetComponent;
    use crate::ident::AssetId;

    #[derive(TypePath, Component, Clone)]
    struct Unrelated;

    #[derive(TypePath, Component, Clone)]
    struct TestHandle(AssetId<()>);

    impl AssetComponent for TestHandle {
        type Asset = ();

        fn asset_id(&self) -> AssetId<()> {
            self.0
        }
    }

    fn world_with_handle() -> (Box<World>, AssetId<()>) {
        let mut world = World::alloc();
        let id = AssetId::<()>::default();
        world.spawn(TestHandle(id), None);
        world.init_resource::<AssetChanges<()>>();
        (world, id)
    }

    fn insert_change(world: &mut World, id: AssetId<()>, tick: Tick) {
        world.resource_mut::<AssetChanges<()>>().insert(id, tick);
    }

    fn count(world: &World) -> usize {
        world.query::<(), AssetChanged<TestHandle>>().iter().count()
    }

    /// A recorded change only matches in the frame that recorded it: once the trackers are cleared
    /// the same record no longer satisfies the filter.
    #[test]
    fn matches_only_inside_the_query_window() {
        let (mut world, id) = world_with_handle();
        let this_run = world.clear_trackers();

        insert_change(&mut world, id, this_run);
        assert_eq!(count(&world), 1);

        world.clear_trackers();
        assert_eq!(count(&world), 0);
    }

    /// The window is strictly after the baseline, so a change stamped with the tick a system would
    /// have seen last frame is already stale, while one from the current run still matches.
    #[test]
    fn ignores_ticks_at_or_before_the_baseline() {
        let (mut world, id) = world_with_handle();
        let this_run = world.clear_trackers();
        let baseline = world.last_run();

        // The window is `(last_run, this_run]`, so the baseline itself does not count.
        insert_change(&mut world, id, baseline);
        assert_eq!(count(&world), 0);

        insert_change(&mut world, id, this_run);
        assert_eq!(count(&world), 1);
    }

    #[test]
    fn removal_drops_the_recorded_change() {
        let (mut world, id) = world_with_handle();
        let this_run = world.clear_trackers();

        insert_change(&mut world, id, this_run);
        assert_eq!(count(&world), 1);

        world.resource_mut::<AssetChanges<()>>().remove(&id);
        assert_eq!(count(&world), 0);
    }

    #[test]
    fn ignores_entities_without_the_component() {
        let (mut world, id) = world_with_handle();
        world.spawn(Unrelated, None);

        let this_run = world.clear_trackers();
        insert_change(&mut world, id, this_run);
        assert_eq!(count(&world), 1);
    }
}
