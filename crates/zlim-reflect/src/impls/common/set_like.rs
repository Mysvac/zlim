use core::fmt;
use core::hash::{Hash, Hasher};

use crate::Reflect;
use crate::db::TypeDB;
use crate::impls::CLONE_TYPE_ERROR;
use crate::impls::CONVERT_TYPE_ERROR;
use crate::ops::{ApplyError, Set};

/// Applies a reflected value to a set by draining and rebuilding it.
///
/// 1. If the types are identical, clone-and-assign directly (fast path).
/// 2. If the [`TypeDB`] has a conversion from `other`'s type to `Self`,
///    clone-and-assign directly.
/// 3. Require `other` to also be a [`Set`]; fail with
///    [`mismatched_kind`](ApplyError::mismatched_kind) otherwise.
/// 4. Drain the destination, saving the removed elements for potential
///    recovery.
/// 5. For each source element: clone it, then insert it into the
///    destination. On clone or insert failure, restore the original
///    elements via the recovery function and return the error.
#[inline(never)]
pub fn set_apply(this: &mut dyn Set, other: &dyn Reflect) -> Result<(), ApplyError> {
    let this_type = this.type_id();
    let other_type = other.type_id();

    // Phase 1: fast path — same type, clone and assign.
    if this_type == other_type
        && let Ok(cloned) = other.reflect_clone()
    {
        this.reflect_assign(cloned).expect(CLONE_TYPE_ERROR);
        return Ok(());
    }

    // Phase 2: fast path — TypeDB conversion exists, clone and assign.
    if let Some(db) = TypeDB::get_by_type(other_type)
        && db.contains_convertor(this_type)
        && let Ok(cloned) = other.reflect_clone()
        && let Ok(converted) = db.convert(cloned, this_type)
    {
        this.reflect_assign(converted).expect(CONVERT_TYPE_ERROR);
        return Ok(());
    }

    // Phase 3: cast `other` to `&dyn Set`.
    let other: &dyn Set = other.reflect_ref().as_set().map_err(|e| {
        ::core::hint::cold_path();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        ApplyError::mismatched_kind(src, apply, e.expected, e.received)
    })?;

    // Phase 4: drain destination; define recovery for rollback on failure.
    let removed: Vec<Box<dyn Reflect>> = this.drain_all();

    #[cold]
    #[inline(never)]
    fn recovery(this: &mut dyn Set, mut removed: Vec<Box<dyn Reflect>>) {
        let _ = this.drain_all();
        while let Some(v) = removed.pop() {
            this.insert_value(v)
                .expect("elements originally from this set should be insertable");
        }
    }

    // Phase 5: clone each source element and insert into destination.
    for value in other.iter_values() {
        // Clone the element; on failure, restore original set and return error.
        let v = match value.reflect_clone() {
            Ok(v) => v,
            Err(e) => {
                ::core::hint::cold_path();
                let src = this.reflect_type_path();
                let apply = other.reflect_type_path();
                let item_name = format!("{value:?}");
                recovery(this, removed);
                return Err(ApplyError::clone_error(src, apply, &item_name, e));
            }
        };

        // Insert the clone; on failure, restore original set and return error.
        if let Err(v) = this.insert_value(v) {
            ::core::hint::cold_path();
            let src = this.reflect_type_path();
            let apply = other.reflect_type_path();
            let item_name = format!("{value:?}");
            let expected = match this.reflect_type_info().as_set() {
                Ok(l) => l.value_info().type_path(),
                Err(_) => "Unknown",
            };
            let received = v.reflect_type_path();
            recovery(this, removed);
            return Err(ApplyError::mismatched_item(
                src, apply, &item_name, expected, received,
            ));
        }
    }

    Ok(())
}

/// Compares two sets for equality.
///
/// 1. Different `TypeId` → not equal.
/// 2. Different sizes → not equal.
/// 3. For each element in `this`, verify it exists in `other` via the
///    set's lookup semantics (which internally use `reflect_eq`). No
///    redundant second comparison is needed.
#[inline(never)]
pub fn set_eq(this: &dyn Set, other: &dyn Reflect) -> bool {
    if this.type_id() != other.type_id() {
        return false;
    }

    let other: &dyn Set = other.reflect_ref().as_set().expect("same type");

    if this.value_len() != other.value_len() {
        return false;
    }

    for value in this.iter_values() {
        let Some(_other_value) = other.value(value) else {
            return false;
        };
    }

    true
}

/*
/// Compares two sets for ordering.
///
/// 1. If `other` is not a [`Set`], order by [`ReflectKind`].
/// 2. Order by [`TypeId`] first; different types are never equal.
/// 3. Smaller sets sort before larger ones.
/// 4. For each element in `this`, verify it exists in `other` via the
///    set's lookup semantics. A missing element makes `this` greater.
#[inline(never)]
pub fn set_cmp(this: &dyn Set, other: &dyn Reflect) -> Ordering {
    let ReflectRef::Set(other) = other.reflect_ref() else {
        return ReflectKind::Set.cmp(&other.reflect_kind());
    };

    let type_ord = this.type_id().cmp(&other.type_id());
    if type_ord != Ordering::Equal {
        return type_ord;
    }

    let len_ord = this.value_len().cmp(&other.value_len());
    if len_ord != Ordering::Equal {
        return len_ord;
    }

    for this_val in this.iter_values() {
        let Some(_other_val) = other.value(this_val) else {
            return Ordering::Greater;
        };
    }

    Ordering::Equal
}
*/

/// Hashes a set.
///
/// 1. Hash the `TypeId` for type disambiguation.
/// 2. Hash each element's [`reflect_hash`](crate::Reflect::reflect_hash).
///    Element order does not affect the hash — the hash is accumulated
///    via sequential writes; recipients should combine with a commutative
///    operator (e.g. XOR) if order-independence is needed.
/// 3. Hash the size for disambiguation.
#[inline(never)]
pub fn set_hash(this: &dyn Set) -> u64 {
    let mut hasher = crate::impls::reflect_hasher();

    this.type_id().hash(&mut hasher);

    for v in this.iter_values() {
        hasher.write_u64(v.reflect_hash());
    }

    hasher.write_usize(this.value_len());

    hasher.finish()
}

/// Formats a set as `Set<Type>({elem1, elem2, ...})`.
#[inline(never)]
pub fn set_debug(this: &dyn Set, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Set<{}>(", this.reflect_type_path())?;

    f.debug_set().entries(this.iter_values()).finish()?;

    f.write_str(")")
}
