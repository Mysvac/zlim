use core::fmt;
use core::hash::{Hash, Hasher};

use crate::Reflect;
use crate::db::TypeDB;
use crate::impls::CLONE_TYPE_ERROR;
use crate::impls::CONVERT_TYPE_ERROR;
use crate::ops::{ApplyError, Map};

/// Applies a reflected value to a map by draining and rebuilding it.
///
/// 1. If the types are identical, clone-and-assign directly (fast path).
/// 2. If the [`TypeDB`] has a conversion from `other`'s type to `Self`,
///    clone-and-assign directly.
/// 3. Require `other` to also be a [`Map`]; fail with
///    [`mismatched_kind`](ApplyError::mismatched_kind) otherwise.
/// 4. Drain the destination, saving the removed entries for potential
///    recovery.
/// 5. For each source entry: clone the key and value, then insert them
///    into the destination. On clone or insert failure, restore the
///    original entries via the recovery function and return the error.
#[inline(never)]
pub fn map_apply(this: &mut dyn Map, other: &dyn Reflect) -> Result<(), ApplyError> {
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

    // Phase 3: cast `other` to `&dyn Map`.
    let other: &dyn Map = other.reflect_ref().as_map().map_err(|e| {
        ::core::hint::cold_path();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        ApplyError::mismatched_kind(src, apply, e.expected, e.received)
    })?;

    // Phase 4: drain destination; define recovery for rollback on failure.
    let removed: Vec<(Box<dyn Reflect>, Box<dyn Reflect>)> = this.drain_all();

    #[cold]
    #[inline(never)]
    fn recovery(this: &mut dyn Map, mut removed: Vec<(Box<dyn Reflect>, Box<dyn Reflect>)>) {
        let _ = this.drain_all();
        while let Some((k, v)) = removed.pop() {
            this.insert_entry(k, v)
                .expect("entries originally from this map should be insertable");
        }
    }

    // Phase 5: clone each source key-value pair and insert into destination.
    for (key, value) in other.iter_entries() {
        // Clone the key; on failure, restore original map and return error.
        let k = match key.reflect_clone() {
            Ok(k) => k,
            Err(e) => {
                ::core::hint::cold_path();
                let src = this.reflect_type_path();
                let apply = other.reflect_type_path();
                let item_name = format!("{key:?}");
                recovery(this, removed);
                return Err(ApplyError::clone_error(src, apply, &item_name, e));
            }
        };
        // Clone the value; on failure, restore original map and return error.
        let v = match value.reflect_clone() {
            Ok(v) => v,
            Err(e) => {
                ::core::hint::cold_path();
                let src = this.reflect_type_path();
                let apply = other.reflect_type_path();
                let item_name = format!("{key:?}");
                recovery(this, removed);
                return Err(ApplyError::clone_error(src, apply, &item_name, e));
            }
        };

        // Insert the cloned pair; on failure, restore original map and return error.
        if let Err((k, v)) = this.insert_entry(k, v) {
            ::core::hint::cold_path();
            let src = this.reflect_type_path();
            let apply = other.reflect_type_path();
            let item_name = format!("{key:?}");
            let expected = match this.reflect_type_info().as_map() {
                Ok(l) => {
                    let kname = l.key_info().type_path();
                    let vname = l.value_info().type_path();
                    format!("{kname} -> {vname}")
                }
                Err(_) => String::from("Unknown"),
            };
            let kname = k.reflect_type_path();
            let vname = v.reflect_type_path();
            let received = format!("{kname} -> {vname}");
            recovery(this, removed);
            return Err(ApplyError::mismatched_item(
                src, apply, &item_name, &expected, &received,
            ));
        }
    }

    Ok(())
}

/// Compares two maps for content-based equality.
///
/// 1. Different `TypeId` → not equal.
/// 2. Different sizes → not equal.
/// 3. For each entry in `this`, look up the key in `other` and compare
///    the associated values with `reflect_eq`. A missing key or a
///    value mismatch makes the maps unequal.
#[inline(never)]
pub fn map_eq(this: &dyn Map, other: &dyn Reflect) -> bool {
    if this.type_id() != other.type_id() {
        return false;
    }

    let other: &dyn Map = other.reflect_ref().as_map().expect("same type");

    if this.entry_len() != other.entry_len() {
        return false;
    }

    for (this_key, this_val) in this.iter_entries() {
        let Some(other_val) = other.value(this_key) else {
            return false;
        };

        if !this_val.reflect_eq(other_val) {
            return false;
        }
    }

    true
}

/// Hashes a map.
///
/// 1. Hash the `TypeId` for type disambiguation.
/// 2. Hash each key-value pair's [`reflect_hash`](crate::Reflect::reflect_hash)
///    in iteration order.
/// 3. Hash the entry count for disambiguation.
#[inline(never)]
pub fn map_hash(this: &dyn Map) -> u64 {
    let mut hasher = crate::impls::reflect_hasher();

    this.type_id().hash(&mut hasher);

    for (k, v) in this.iter_entries() {
        hasher.write_u64(k.reflect_hash());
        hasher.write_u64(v.reflect_hash());
    }

    hasher.write_usize(this.entry_len());

    hasher.finish()
}

/// Formats a map as `Map<Type>({key1: val1, key2: val2, ...})`.
#[inline(never)]
pub fn map_debug(this: &dyn Map, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Map<{}>(", this.reflect_type_path())?;

    f.debug_map().entries(this.iter_entries()).finish()?;

    f.write_str(")")
}

/*
/// Compares two maps for ordering.
///
/// 1. If `other` is not a [`Map`], order by [`ReflectKind`].
/// 2. Order by [`TypeId`] first; different types are never equal.
/// 3. Fewer entries sort before more entries.
/// 4. For each entry in `this`, look up the key in `other`:
///    a missing key makes `this` greater; matching keys compare their
///    values lexicographically.
#[inline(never)]
pub fn map_cmp(this: &dyn Map, other: &dyn Reflect) -> Ordering {
    let ReflectRef::Map(other) = other.reflect_ref() else {
        return ReflectKind::Map.cmp(&other.reflect_kind());
    };

    let type_ord = this.type_id().cmp(&other.type_id());
    if type_ord != Ordering::Equal {
        return type_ord;
    }

    let len_ord = this.entry_len().cmp(&other.entry_len());
    if len_ord != Ordering::Equal {
        return len_ord;
    }

    for (this_key, this_val) in this.iter_entries() {
        let Some(other_val) = other.value(this_key) else {
            return Ordering::Greater;
        };

        let val_ord = this_val.reflect_cmp(other_val);

        if val_ord != Ordering::Equal {
            return val_ord;
        }
    }

    Ordering::Equal
}
*/
