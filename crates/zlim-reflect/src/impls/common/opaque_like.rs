use core::fmt;
use core::hash::{Hash, Hasher};

use crate::Reflect;
use crate::db::TypeDB;
use crate::impls::CLONE_TYPE_ERROR;
use crate::impls::CONVERT_TYPE_ERROR;
use crate::ops::{ApplyError, Opaque};

/// Applies a reflected value to an opaque value.
///
/// 1. If the types are identical, clone-and-assign directly (fast path).
/// 2. If the [`TypeDB`] has a conversion from `other`'s type to `Self`,
///    clone-and-assign directly.
/// 3. Require `other` to also be [`Opaque`]; fail with
///    [`mismatched_kind`](ApplyError::mismatched_kind) otherwise.
/// 4. Serialize the source via [`stringify`](Opaque::stringify) and apply the
///    resulting string via [`Opaque::apply_str`].
#[inline(never)]
pub fn opaque_apply(this: &mut dyn Opaque, other: &dyn Reflect) -> Result<(), ApplyError> {
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

    ::core::hint::cold_path();

    // Phase 3: cast `other` to `&dyn Opaque`.
    let other: &dyn Opaque = other.reflect_ref().as_opaque().map_err(|e| {
        ::core::hint::cold_path();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        ApplyError::mismatched_kind(src, apply, e.expected, e.received)
    })?;

    // Phase 4: serialize source and apply to destination.
    this.apply_str(&other.stringify()).map_err(|error| {
        ::core::hint::cold_path();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        ApplyError { src, apply, error }
    })
}

/// Compares two opaque values for equality.
///
/// 1. Different `TypeId` → not equal.
/// 2. Compare serialized string representations.
#[inline(never)]
pub fn opaque_eq(this: &dyn Opaque, other: &dyn Reflect) -> bool {
    if this.type_id() != other.type_id() {
        return false;
    }

    let other: &dyn Opaque = other.reflect_ref().as_opaque().expect("same type");

    this.stringify() == other.stringify()
}

/*
/// Compares two opaque values for ordering.
///
/// 1. If `other` is not [`Opaque`], order by [`ReflectKind`].
/// 2. Order by [`TypeId`] first; different types are never equal.
/// 3. Compare serialized string representations lexicographically.
#[inline(never)]
pub fn opaque_cmp(this: &dyn Opaque, other: &dyn Reflect) -> Ordering {
    let ReflectRef::Opaque(other) = other.reflect_ref() else {
        return ReflectKind::Opaque.cmp(&other.reflect_kind());
    };

    let type_ord = this.type_id().cmp(&other.type_id());
    if type_ord != Ordering::Equal {
        return type_ord;
    }

    this.stringify().cmp(&other.stringify())
}
*/

/// Hashes an opaque value.
///
/// 1. Hash the `TypeId` for type disambiguation.
/// 2. Hash the serialized string representation.
#[inline(never)]
pub fn opaque_hash(this: &dyn Opaque) -> u64 {
    let mut hasher = crate::impls::reflect_hasher();

    this.type_id().hash(&mut hasher);
    this.stringify().hash(&mut hasher);

    hasher.finish()
}

/// Formats an opaque value as `Opaque<Type>(serialized_value)`.
#[inline(never)]
pub fn opaque_debug(this: &dyn Opaque, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
        f,
        "Opaque<{}>({})",
        this.reflect_type_path(),
        this.stringify()
    )
}
