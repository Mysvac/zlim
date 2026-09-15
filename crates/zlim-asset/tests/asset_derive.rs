//! Integration tests for `#[derive(Asset)]` and `#[derive(VisitAssetDependencies)]`.

use zlim_asset::asset::{Asset, VisitAssetDependencies};
use zlim_asset::handle::Handle;
use zlim_asset::ident::{AssetId, ErasedAssetId};
use zlim_asset::uuid::Uuid;
use zlim_path::TypePath;
use zlim_utils::hash::HashSet;

// -----------------------------------------------------------------------------
// Fixtures

/// A leaf asset: an asset without dependencies.
#[derive(TypePath, Asset)]
struct Dependency;

/// Not an asset, but stored inside one — so it derives only the visitor.
#[derive(VisitAssetDependencies)]
struct DependencyRef(#[asset(dependency)] ErasedAssetId);

#[derive(TypePath, Asset)]
struct Named {
    #[asset(dependency)]
    first: ErasedAssetId,
    /// Not marked, so it is skipped even though `ErasedAssetId` is a dependency type.
    skipped: ErasedAssetId,
    #[asset(dependency)]
    second: Option<ErasedAssetId>,
    #[asset(dependency)]
    nested: DependencyRef,
    #[asset(dependency)]
    handles: Vec<Handle<Dependency>>,
    plain: u32,
}

fn dep(seed: u128) -> ErasedAssetId {
    AssetId::<Dependency>::from(Uuid::from_u128(seed)).erased()
}

fn handle(seed: u128) -> Handle<Dependency> {
    Handle::from(Uuid::from_u128(seed))
}

fn visited(value: &impl VisitAssetDependencies) -> Vec<ErasedAssetId> {
    let mut visited = Vec::new();
    value.visit_dependencies(&mut |id| visited.push(id));
    visited
}

// -----------------------------------------------------------------------------
// Impls

#[test]
fn derive_asset_implements_both_traits() {
    fn assert_asset<A: Asset>() {}
    fn assert_visit<V: VisitAssetDependencies>() {}

    assert_asset::<Dependency>();
    assert_asset::<Named>();
    // The visitor can also be derived on its own, for a type that is not an asset.
    assert_visit::<DependencyRef>();
}

/// Checks the order the derive promises: the marked fields are visited in
/// declaration order, with the option, the nested visitor and the handle list
/// contributing what they hold. The unmarked fields are neither visited nor
/// altered.
#[test]
fn visits_marked_fields_in_declaration_order() {
    let value = Named {
        first: dep(1),
        skipped: dep(9),
        second: Some(dep(2)),
        nested: DependencyRef(dep(3)),
        handles: vec![handle(4), handle(5)],
        plain: 7,
    };

    // The unmarked fields keep their values and are never visited.
    assert_eq!(value.skipped, dep(9));
    assert_eq!(value.plain, 7);
    assert_eq!(
        visited(&value),
        vec![dep(1), dep(2), dep(3), dep(4), dep(5)],
    );
}

/// A unit struct has nothing to visit at all, and in a tuple struct only the
/// marked positions are, so the plain fields beside them stay ordinary data.
#[test]
fn visits_nothing_without_marked_fields() {
    #[derive(TypePath, Asset)]
    struct Unit;

    #[derive(TypePath, Asset)]
    struct Tuple(#[asset(dependency)] ErasedAssetId, u32);

    let tuple = Tuple(dep(6), 1);

    assert!(visited(&Unit).is_empty());
    assert_eq!(tuple.1, 1);
    assert_eq!(visited(&tuple), vec![dep(6)]);
}

/// Every variant shape goes through the same visitor: the variants that mark no
/// field contribute nothing, and a tuple variant visits only its marked positions,
/// in order, wherever they sit in the tuple.
#[test]
fn enums_visit_only_the_matched_variant() {
    #[derive(TypePath, Asset)]
    enum Kind {
        Empty,
        Plain,
        Named {
            #[asset(dependency)]
            only: ErasedAssetId,
        },
        Tuple(
            #[asset(dependency)] ErasedAssetId,
            u32,
            #[asset(dependency)] ErasedAssetId,
        ),
    }

    assert!(visited(&Kind::Empty).is_empty());
    assert!(visited(&Kind::Plain).is_empty());
    assert_eq!(visited(&Kind::Named { only: dep(7) }), vec![dep(7)]);

    // The middle field is unmarked: only the fields around it are visited.
    let value = Kind::Tuple(dep(8), 3, dep(9));
    assert!(matches!(value, Kind::Tuple(_, 3, _)));
    assert_eq!(visited(&value), vec![dep(8), dep(9)]);
}

/// An absent optional field contributes nothing, while a present one and a
/// container are descended into, so a dependency held inside a container is
/// reported exactly like a direct one.
#[test]
fn options_and_containers_are_visited_recursively() {
    #[derive(TypePath, Asset)]
    struct Containers {
        #[asset(dependency)]
        missing: Option<ErasedAssetId>,
        #[asset(dependency)]
        present: Option<ErasedAssetId>,
        #[asset(dependency)]
        set: HashSet<Handle<Dependency>>,
    }

    let value = Containers {
        missing: None,
        present: Some(dep(10)),
        set: HashSet::from_iter([handle(11)]),
    };

    assert_eq!(visited(&value), vec![dep(10), dep(11)]);
}

/// The derived visitor forwards to its type parameter instead of stopping at the
/// wrapper, so whatever the inner type reports reaches the caller.
#[test]
fn generic_types_forward_to_the_type_parameter() {
    // The macro adds no bounds, so a generic wrapper writes them on the type itself.
    #[derive(VisitAssetDependencies)]
    struct Wrapper<T: VisitAssetDependencies> {
        #[asset(dependency)]
        inner: T,
    }

    assert_eq!(visited(&Wrapper { inner: dep(12) }), vec![dep(12)]);
}
