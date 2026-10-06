//! Centralized path management for derive macros.
//!
//! This module keeps every reference to `zlim_core`'s internal layout in one
//! place.  When items move within `zlim_core`, only the helpers in this module
//! need updating — the derive macros themselves remain unchanged.
//!
//! # Organisation
//!
//! - Re-export full-path marker types from [`zlim_derive_utils`].
//! - [`zlim_core_path`] — resolves the canonical `syn::Path` to `zlim_core`.
//! - Token-stream helpers — each takes a `&Path` to `zlim_core` and emits
//!   an absolute path into that crate.

use proc_macro2::TokenStream;
use quote::quote;
use syn::Path;

// -----------------------------------------------------------------------------
// Crate path resolver

/// Resolve the canonical path to the `zlim_core` crate.
///
/// The result depends on the caller's `Cargo.toml`:
/// - If `zlim` is a dependency → `::zlim::core`
/// - If `zlim_core` is a direct dependency → `::zlim_core`
/// - If `zlim` is a dev-dependency → `::zlim::core`
/// - Otherwise falls back to `::zlim_core`
#[inline]
pub fn zlim_core_path() -> Path {
    zlim_derive_utils::crate_path("zlim_core")
}

// -----------------------------------------------------------------------------
// Token-stream helpers — zlim_core

macro_rules! def_path_fn {
    ($name:ident, $($seg:ident)::+) => {
        #[inline]
        pub(crate) fn $name(zlim_core: &Path) -> TokenStream {
            quote!( #zlim_core :: $($seg)::+ )
        }
    };
}

def_path_fn!(zlim_error, error::ZlimError);
def_path_fn!(bundle_, bundle::Bundle);
def_path_fn!(bundle_writer_, bundle::BundleWriter);
def_path_fn!(component_collector_, component::ComponentCollector);
def_path_fn!(component_writer_, component::ComponentWriter);
def_path_fn!(components_, component::Components);
def_path_fn!(owning_ptr_, __macro_exports__::OwningPtr);
def_path_fn!(type_database_, __macro_exports__::TypeDatabase);
def_path_fn!(type_path_, __macro_exports__::TypePath);
def_path_fn!(resource_, resource::Resource);
def_path_fn!(resource_db_, resource::ResourceDB);
def_path_fn!(resource_register_reflect_, resource::register_reflect);
def_path_fn!(component_, component::Component);
def_path_fn!(component_db_, component::ComponentDB);
def_path_fn!(component_hook_, component::ComponentHook);
def_path_fn!(component_cloner_, clone::ComponentCloner);
def_path_fn!(required_, component::Required);
def_path_fn!(component_register_reflect_, component::register_reflect);
def_path_fn!(component_register_serialize_, component::register_serialize);
def_path_fn!(map_entities_, entity::MapEntities);
def_path_fn!(entity_mapper_, entity::EntityMapper);

def_path_fn!(world_, world::World);
def_path_fn!(world_cell_, world::WorldCell);
def_path_fn!(deferred_world_, world::DeferredWorld);
def_path_fn!(system_param_, system::SystemParam);
def_path_fn!(system_param_error_, system::SystemParamError);
def_path_fn!(access_table_, system::AccessTable);
def_path_fn!(tick_, tick::Tick);
def_path_fn!(into_job_, job::IntoJob);
def_path_fn!(job_trait_, job::Job);
def_path_fn!(job_db_, job::JobDB);
def_path_fn!(job_label_, job::JobLabel);
def_path_fn!(job_group_, job::JobGroup);
def_path_fn!(job_group_label_, job::JobGroupLabel);
def_path_fn!(job_group_reg_, job::__JobGroupReg__);
def_path_fn!(job_reg_, job::__JobReg__);
def_path_fn!(debug_location_, __macro_exports__::DebugLocation);
def_path_fn!(type_path_derive_, __macro_exports__::TypePathDerive);
def_path_fn!(intern_str_, __macro_exports__::intern_str);
def_path_fn!(submit_, __macro_exports__::submit);
def_path_fn!(slice_pool_, utils::SlicePool);
def_path_fn!(schedule_label_, schedule::ScheduleLabel);
def_path_fn!(schedule_stage_, schedule::ScheduleStage);
def_path_fn!(message_, message::Message);
def_path_fn!(query_data_, query::QueryData);
def_path_fn!(readonly_query_data_, query::ReadOnlyQueryData);
def_path_fn!(query_slice_, query::QuerySlice);
def_path_fn!(component_access_, system::ComponentAccess);
def_path_fn!(filter_param_builder_, system::FilterParamBuilder);
def_path_fn!(table_, table::Table);
def_path_fn!(table_row_, table::TableRow);
def_path_fn!(entity_id_, entity::EntityId);
def_path_fn!(template_, template::Template);
def_path_fn!(into_template_, template::IntoTemplate);
def_path_fn!(template_context_, template::TemplateContext);
def_path_fn!(built_in_template_, template::BuiltInTemplate);
def_path_fn!(specialize_from_template_, template::SpecializeTemplate);
def_path_fn!(zlim_result_, error::ZlimResult);
def_path_fn!(entity_label_, entity::EntityLabel);
