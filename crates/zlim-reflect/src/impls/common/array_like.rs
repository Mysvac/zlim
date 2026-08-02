use core::fmt;
use core::hash::{Hash, Hasher};

use crate::Reflect;
use crate::db::TypeDB;
use crate::impls::CLONE_TYPE_ERROR;
use crate::impls::CONVERT_TYPE_ERROR;
use crate::ops::{ApplyError, Array};

/// Applies a reflected value to an array, element by element.
///
/// 1. If the types are identical, clone-and-assign directly (fast path).
/// 2. Require `other` to also be an [`Array`]; fail with
///    [`mismatched_kind`](ApplyError::mismatched_kind) otherwise.
/// 3. Fail with [`mismatched_size`](ApplyError::mismatched_size) if the
///    lengths differ — arrays are fixed-size.
/// 4. Apply each element pair by index in order, propagating the first
///    error.
#[inline(never)]
pub fn array_apply(this: &mut dyn Array, other: &dyn Reflect) -> Result<(), ApplyError> {
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
    if let Some(db) = TypeDB::get_by_type(other.type_id())
        && db.contains_convertor(this_type)
        && let Ok(cloned) = other.reflect_clone()
        && let Ok(converted) = db.convert(cloned, this_type)
    {
        this.reflect_assign(converted).expect(CONVERT_TYPE_ERROR);
        return Ok(());
    }

    // Phase 2: cast `other` to `&dyn Array`.
    let other: &dyn Array = other.reflect_ref().as_array().map_err(|e| {
        ::core::hint::cold_path();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        ApplyError::mismatched_kind(src, apply, e.expected, e.received)
    })?;

    // Phase 3: validate lengths match (arrays are fixed-size).
    let this_len = this.item_len();
    let other_len = other.item_len();
    if this_len != other_len {
        ::core::hint::cold_path();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        return Err(ApplyError::mismatched_size(src, apply, this_len, other_len));
    }

    // Phase 4: apply each element pair by index.
    for index in 0..this_len {
        const MSG: &str = "the length of array should be correct";
        let to = this.item_mut(index).expect(MSG);
        let from = other.item(index).expect(MSG);
        to.reflect_apply(from)?;
    }

    Ok(())
}

/// Compares two arrays for equality.
///
/// 1. Different `TypeId` → not equal.
/// 2. Different lengths → not equal.
/// 3. Compare every element pair by index; all must be reflect-equal.
#[inline(never)]
pub fn array_eq(this: &dyn Array, other: &dyn Reflect) -> bool {
    if this.type_id() != other.type_id() {
        return false;
    }

    let other: &dyn Array = other.reflect_ref().as_array().expect("same type");

    if this.item_len() != other.item_len() {
        return false;
    }

    this.iter_items()
        .zip(other.iter_items())
        .all(|(x, y)| x.reflect_eq(y))
}

/// Hashes an array.
///
/// 1. Hash the `TypeId` for type disambiguation.
/// 2. Hash each element's [`reflect_hash`](crate::Reflect::reflect_hash)
///    in index order.
/// 3. Hash the length for disambiguation.
#[inline(never)]
pub fn array_hash(this: &dyn Array) -> u64 {
    let mut hasher = crate::impls::reflect_hasher();

    this.type_id().hash(&mut hasher);

    for item in this.iter_items() {
        hasher.write_u64(item.reflect_hash());
    }

    hasher.write_usize(this.item_len());

    hasher.finish()
}

/// Formats an array as `Array<Type>([elem1, elem2, ...])`.
#[inline(never)]
pub fn array_debug(this: &dyn Array, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Array<{}>(", this.reflect_type_path())?;
    f.debug_list().entries(this.iter_items()).finish()?;
    f.write_str(")")
}

/*
/// Compares two arrays for ordering.
///
/// 1. If `other` is not an [`Array`], order by [`ReflectKind`].
/// 2. Order by [`TypeId`] first; different types are never equal.
/// 3. Shorter arrays sort before longer ones.
/// 4. Compare elements pairwise by index lexicographically.
#[inline(never)]
pub fn array_cmp(this: &dyn Array, other: &dyn Reflect) -> Ordering {
    let ReflectRef::Array(other) = other.reflect_ref() else {
        return ReflectKind::Array.cmp(&other.reflect_kind());
    };

    let type_ord = this.type_id().cmp(&other.type_id());
    if type_ord != Ordering::Equal {
        return type_ord;
    }

    let len_ord = this.item_len().cmp(&other.item_len());
    if len_ord != Ordering::Equal {
        return len_ord;
    }

    for (x, y) in this.iter_items().zip(other.iter_items()) {
        let ret = x.reflect_cmp(y);
        if ret != Ordering::Equal {
            return ret;
        }
    }

    Ordering::Equal
}
*/
