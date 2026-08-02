use core::fmt;
use core::hash::{Hash, Hasher};

use crate::Reflect;
use crate::db::TypeDB;
use crate::impls::CLONE_TYPE_ERROR;
use crate::impls::CONVERT_TYPE_ERROR;
use crate::ops::{ApplyError, Struct};

/// Applies a reflected value to a struct with loose field matching.
///
/// Missing fields in the source are silently skipped; extra fields in
/// the source are ignored. Field order does not matter — matching is
/// by name only.
///
/// 1. If the types are identical, clone-and-assign directly (fast path).
/// 2. If the [`TypeDB`] has a conversion from `other`'s type to `Self`,
///    clone-and-assign directly.
/// 3. Require `other` to also be a [`Struct`]; fail with
///    [`mismatched_kind`](ApplyError::mismatched_kind) otherwise.
/// 4. For each field in `this` (in declaration order), look it up by
///    name in `other`. If present, apply recursively; if absent, skip.
///    Mismatched field types propagate as errors from the inner apply.
#[inline(never)]
pub fn struct_apply(this: &mut dyn Struct, other: &dyn Reflect) -> Result<(), ApplyError> {
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

    // Phase 3: cast `other` to `&dyn Struct`.
    let other: &dyn Struct = other.reflect_ref().as_struct().map_err(|e| {
        ::core::hint::cold_path();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        ApplyError::mismatched_kind(src, apply, e.expected, e.received)
    })?;

    // Phase 4: iterate `this`'s fields; apply matching fields from `other` by name.
    for index in 0..this.field_len() {
        const MSG: &str = "the length of struct should be correct";
        let name = this.name_at(index).expect(MSG);
        if let Some(y) = other.field(name) {
            let x = this.field_at_mut(index).expect(MSG);
            x.reflect_apply(y)?;
        }
    }

    Ok(())
}

/// Compares two structs for equality.
///
/// 1. Different `TypeId` → not equal.
/// 2. Different field counts → not equal.
/// 3. Compare every field by declaration order: field names must match,
///    then field values must be reflect-equal.
#[inline(never)]
pub fn struct_eq(this: &dyn Struct, other: &dyn Reflect) -> bool {
    if this.type_id() != other.type_id() {
        return false;
    }

    let other: &dyn Struct = other.reflect_ref().as_struct().expect("same type");

    if this.field_len() != other.field_len() {
        return false;
    }

    for index in 0..this.field_len() {
        const MSG: &str = "the length of struct should be correct";
        let x_name = this.name_at(index).expect(MSG);
        let y_name = other.name_at(index).expect(MSG);
        if x_name != y_name {
            return false;
        }

        let x_field = this.field_at(index).expect(MSG);
        let y_field = other.field_at(index).expect(MSG);
        if !x_field.reflect_eq(y_field) {
            return false;
        }
    }

    true
}

/// Hashes a struct.
///
/// 1. Hash the `TypeId` for type disambiguation.
/// 2. For each field in declaration order: hash the field name, then
///    the field's [`reflect_hash`](crate::Reflect::reflect_hash).
/// 3. Hash the field count for disambiguation.
#[inline(never)]
pub fn struct_hash(this: &dyn Struct) -> u64 {
    let mut hasher = crate::impls::reflect_hasher();

    this.type_id().hash(&mut hasher);

    for (i, item) in this.iter_fields().enumerate() {
        let name = this.name_at(i).expect("valid index");
        name.hash(&mut hasher);
        hasher.write_u64(item.reflect_hash());
    }

    hasher.write_usize(this.field_len());

    hasher.finish()
}

/// Formats a struct as `Struct<Type>({field1: val1, field2: val2, ...})`.
#[inline(never)]
pub fn struct_debug(this: &dyn Struct, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Struct<{}>(", this.reflect_type_path())?;
    let mut debugger = f.debug_map();

    for (index, item) in this.iter_fields().enumerate() {
        let name = this.name_at(index).expect("valid index");
        debugger.entry(&name, &item);
    }
    debugger.finish()?;

    f.write_str(")")
}

/*
/// Compares two structs for ordering.
///
/// 1. If `other` is not a [`Struct`], order by [`ReflectKind`].
/// 2. Order by [`TypeId`] first; different types are never equal.
/// 3. Fewer fields sort before more fields.
/// 4. Compare fields in declaration order: first by name, then by
///    reflected value.
#[inline(never)]
pub fn struct_cmp(this: &dyn Struct, other: &dyn Reflect) -> Ordering {
    let ReflectRef::Struct(other) = other.reflect_ref() else {
        return ReflectKind::Struct.cmp(&other.reflect_kind());
    };

    let type_ord = this.type_id().cmp(&other.type_id());
    if type_ord != Ordering::Equal {
        return type_ord;
    }

    let len_ord = this.field_len().cmp(&other.field_len());
    if len_ord != Ordering::Equal {
        return len_ord;
    }

    for index in 0..this.field_len() {
        const MSG: &str = "the length of struct should be correct";
        let x_name = this.name_at(index).expect(MSG);
        let y_name = other.name_at(index).expect(MSG);
        let name_ord = x_name.cmp(y_name);
        if name_ord != Ordering::Equal {
            return name_ord;
        }

        let x_field = this.field_at(index).expect(MSG);
        let y_field = other.field_at(index).expect(MSG);
        let field_ord = x_field.reflect_cmp(y_field);
        if field_ord != Ordering::Equal {
            return field_ord;
        }
    }

    Ordering::Equal
}
*/
