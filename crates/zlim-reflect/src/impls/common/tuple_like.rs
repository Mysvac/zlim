use core::fmt;
use core::hash::{Hash, Hasher};

use crate::Reflect;
use crate::db::TypeDB;
use crate::impls::CLONE_TYPE_ERROR;
use crate::impls::CONVERT_TYPE_ERROR;
use crate::ops::{ApplyError, Tuple};

/// Applies a reflected value to a tuple, field by field in index order.
///
/// 1. If the types are identical, clone-and-assign directly (fast path).
/// 2. If the [`TypeDB`] has a conversion from `other`'s type to `Self`,
///    clone-and-assign directly.
/// 3. Require `other` to also be a [`Tuple`]; fail with
///    [`mismatched_kind`](ApplyError::mismatched_kind) otherwise.
/// 4. Fail with [`mismatched_size`](ApplyError::mismatched_size) if the
///    lengths differ — tuples are fixed-size.
/// 5. Apply each field pair by index in order, propagating the first
///    error.
#[inline(never)]
pub fn tuple_apply(this: &mut dyn Tuple, other: &dyn Reflect) -> Result<(), ApplyError> {
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

    // Phase 3: cast `other` to `&dyn Tuple`.
    let other: &dyn Tuple = other.reflect_ref().as_tuple().map_err(|e| {
        ::core::hint::cold_path();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        ApplyError::mismatched_kind(src, apply, e.expected, e.received)
    })?;

    // Phase 4: validate lengths match (tuples are fixed-size).
    let this_len = this.field_len();
    let other_len = other.field_len();
    if this_len != other_len {
        ::core::hint::cold_path();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        return Err(ApplyError::mismatched_size(src, apply, this_len, other_len));
    }

    // Phase 5: apply each field pair by index.
    for index in 0..this_len {
        const MSG: &str = "the length of tuple should be correct";
        let to = this.field_mut(index).expect(MSG);
        let from = other.field(index).expect(MSG);
        to.reflect_apply(from)?;
    }

    Ok(())
}

/// Compares two tuples for equality.
///
/// 1. Different `TypeId` → not equal.
/// 2. Different lengths → not equal.
/// 3. Compare every field pair by index; all must be reflect-equal.
#[inline(never)]
pub fn tuple_eq(this: &dyn Tuple, other: &dyn Reflect) -> bool {
    if this.type_id() != other.type_id() {
        return false;
    }

    let other: &dyn Tuple = other.reflect_ref().as_tuple().expect("same type");

    if this.field_len() != other.field_len() {
        return false;
    }

    this.iter_fields()
        .zip(other.iter_fields())
        .all(|(x, y)| x.reflect_eq(y))
}

/// Hashes a tuple.
///
/// 1. Hash the `TypeId` for type disambiguation.
/// 2. Hash each field's [`reflect_hash`](crate::Reflect::reflect_hash)
///    in index order.
/// 3. Hash the length for disambiguation.
#[inline(never)]
pub fn tuple_hash(this: &dyn Tuple) -> u64 {
    let mut hasher = crate::impls::reflect_hasher();

    this.type_id().hash(&mut hasher);

    for field in this.iter_fields() {
        hasher.write_u64(field.reflect_hash());
    }

    hasher.write_usize(this.field_len());

    hasher.finish()
}

/// Formats a tuple as `Tuple<Type>((field1, field2, ...))`.
#[inline(never)]
pub fn tuple_debug(this: &dyn Tuple, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Tuple<{}>(", this.reflect_type_path())?;
    f.debug_list().entries(this.iter_fields()).finish()?;
    f.write_str(")")
}

/*
/// Compares two tuples for ordering.
///
/// 1. If `other` is not a [`Tuple`], order by [`ReflectKind`].
/// 2. Order by [`TypeId`] first; different types are never equal.
/// 3. Shorter tuples sort before longer ones.
/// 4. Compare fields pairwise by index lexicographically.
#[inline(never)]
pub fn tuple_cmp(this: &dyn Tuple, other: &dyn Reflect) -> Ordering {
    let ReflectRef::Tuple(other) = other.reflect_ref() else {
        return ReflectKind::Tuple.cmp(&other.reflect_kind());
    };

    let type_ord = this.type_id().cmp(&other.type_id());
    if type_ord != Ordering::Equal {
        return type_ord;
    }

    let len_ord = this.field_len().cmp(&other.field_len());
    if len_ord != Ordering::Equal {
        return len_ord;
    }

    for (x, y) in this.iter_fields().zip(other.iter_fields()) {
        let ret = x.reflect_cmp(y);
        if ret != Ordering::Equal {
            return ret;
        }
    }

    Ordering::Equal
}
*/
