use core::fmt;
use core::hash::{Hash, Hasher};

use crate::Reflect;
use crate::db::TypeDB;
use crate::impls::CLONE_TYPE_ERROR;
use crate::impls::CONVERT_TYPE_ERROR;
use crate::info::VariantKind;
use crate::ops::{ApplyError, Enum};

/// A helper for implementing [`Reflect::reflect_apply`] on enums.
///
/// # Return value
///
/// - `Ok(Ok(()))` — the apply completed successfully.
/// - `Ok(Err(other))` — the variant name didn't match; the caller should
///   handle the variant change (e.g. by switching to the new variant).
/// - `Err(...)` — an irrecoverable error occurred (kind mismatch, size
///   mismatch, etc.).
///
/// # Execution steps
///
/// 1. If the types are identical, clone-and-assign directly (fast path).
/// 2. If the [`TypeDB`] has a conversion from `other`'s type to `Self`,
///    clone-and-assign directly.
/// 3. Require `other` to be an [`Enum`]; fail otherwise.
/// 4. If the variant name differs, return `Ok(Err(other))` — the caller
///    decides how to handle the variant change.
/// 5. If the variant name matches but the variant kind differs, fail
///    with [`mismatched_variant`](ApplyError::mismatched_variant).
/// 6. Apply fields according to the variant kind:
///    - **Unit**: nothing to apply.
///    - **Tuple**: validate field count, then apply each field by index.
///    - **Struct**: apply each field by name (missing source fields are
///      skipped, same as [`struct_apply`](super::struct_apply)).
///
/// # Example
///
/// ```ignore
/// fn apply(&mut self, value: &dyn Reflect) -> Result<(), ApplyError> {
///     if let Err(y) = enum_try_apply(self, value)? {
///         // variant name mismatch — switch to the new variant
///         self.set_variant(y.variant_name(), ...);
///     }
///     Ok(())
/// }
/// ```
#[inline(never)]
pub fn enum_try_apply<'b>(
    this: &mut dyn Enum,
    other: &'b dyn Reflect,
) -> Result<Result<(), &'b dyn Enum>, ApplyError> {
    let this_type = this.type_id();
    let other_type = other.type_id();

    // Phase 1: fast path — same type, clone and assign.
    if this_type == other_type
        && let Ok(cloned) = other.reflect_clone()
    {
        this.reflect_assign(cloned).expect(CLONE_TYPE_ERROR);
        return Ok(Ok(()));
    }

    // Phase 2: fast path — TypeDB conversion exists, clone and assign.
    if let Some(db) = TypeDB::get_by_type(other_type)
        && db.contains_convertor(this_type)
        && let Ok(cloned) = other.reflect_clone()
        && let Ok(converted) = db.convert(cloned, this_type)
    {
        this.reflect_assign(converted).expect(CONVERT_TYPE_ERROR);
        return Ok(Ok(()));
    }

    // Phase 3: cast `other` to `&dyn Enum`.
    let other: &dyn Enum = other.reflect_ref().as_enum().map_err(|e| {
        ::core::hint::cold_path();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        ApplyError::mismatched_kind(src, apply, e.expected, e.received)
    })?;

    // Phase 4: check variant name; return `Ok(Err(other))` on mismatch
    // so the caller can handle the variant change.
    if this.variant_name() != other.variant_name() {
        return Ok(Err(other));
    }

    // Phase 5: validate variant kind consistency.
    let expected = this.variant_kind();
    let received = other.variant_kind();
    if expected != received {
        ::core::hint::cold_path();
        let variant_name = this.variant_name();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        return Err(ApplyError::mismatched_variant(
            src,
            apply,
            variant_name,
            expected,
            received,
        ));
    }

    // Phase 6: apply fields per variant kind.
    const MSG: &str = "the field_len of enum should be correct";
    match expected {
        VariantKind::Struct => {
            // Apply each field by name; missing fields in the source are skipped.
            for index in 0..this.field_len() {
                let name = this.field_name_at(index).expect(MSG);
                if let Some(y) = other.field(name) {
                    let x = this.field_at_mut(index).expect(MSG);
                    x.reflect_apply(y)?;
                }
            }
        }
        VariantKind::Tuple => {
            // Validate field count, then apply each field by index.
            let expected = this.field_len();
            let received = other.field_len();
            if expected != received {
                ::core::hint::cold_path();
                let src = this.reflect_type_path();
                let apply = other.reflect_type_path();
                return Err(ApplyError::mismatched_size(src, apply, expected, received));
            }
            for index in 0..expected {
                let of = other.field_at(index).expect(MSG);
                let tf = this.field_at_mut(index).expect(MSG);
                tf.reflect_apply(of)?;
            }
        }
        VariantKind::Unit => {}
    }

    Ok(Ok(()))
}

/// Compares two enums for content-based equality.
///
/// 1. Different `TypeId` → not equal.
/// 2. Different variant kind, index, or name → not equal.
/// 3. Different field count → not equal.
/// 4. Unit variants are always equal. Tuple variants compare fields by
///    index. Struct variants compare field names first, then field
///    values.
#[inline(never)]
pub fn enum_eq(this: &dyn Enum, other: &dyn Reflect) -> bool {
    if this.type_id() != other.type_id() {
        return false;
    }

    let other: &dyn Enum = other.reflect_ref().as_enum().expect("same type");

    if this.variant_kind() != other.variant_kind() {
        return false;
    }

    if this.variant_index() != other.variant_index() {
        return false;
    }

    if this.variant_name() != other.variant_name() {
        return false;
    }

    if this.field_len() != other.field_len() {
        return false;
    }

    const MSG1: &str = "the field_len of enum should be correct";
    const MSG2: &str = "the variant_kind of enum should be correct";
    match this.variant_kind() {
        VariantKind::Unit => {}
        VariantKind::Tuple => {
            for index in 0..this.field_len() {
                let xf = this.field_at(index).expect(MSG1);
                let yf = other.field_at(index).expect(MSG1);
                if !xf.reflect_eq(yf) {
                    return false;
                }
            }
        }
        VariantKind::Struct => {
            for index in 0..this.field_len() {
                let xn = this.field_name_at(index).expect(MSG2);
                let yn = other.field_name_at(index).expect(MSG2);
                if xn != yn {
                    return false;
                }
                let xf = this.field_at(index).expect(MSG1);
                let yf = other.field_at(index).expect(MSG1);
                if !xf.reflect_eq(yf) {
                    return false;
                }
            }
        }
    }

    true
}

/// Hashes an enum.
///
/// 1. Hash the `TypeId` for type disambiguation.
/// 2. Hash the variant kind and variant name.
/// 3. For tuple variants: hash each field's
///    [`reflect_hash`](crate::Reflect::reflect_hash) in index order.
///    For struct variants: hash each field's name and value.
/// 4. Hash the field count for disambiguation.
#[inline(never)]
pub fn enum_hash(this: &dyn Enum) -> u64 {
    let mut hasher = crate::impls::reflect_hasher();

    this.type_id().hash(&mut hasher);
    this.variant_kind().hash(&mut hasher);
    this.variant_name().hash(&mut hasher);

    match this.variant_kind() {
        VariantKind::Unit => {}
        VariantKind::Tuple => {
            for field in this.iter_fields() {
                hasher.write_u64(field.reflect_hash());
            }
        }
        VariantKind::Struct => {
            for i in 0..this.field_len() {
                let name = this.field_name_at(i).expect("valid index");
                let field = this.field_at(i).expect("valid index");
                hasher.write(name.as_bytes());
                hasher.write_u64(field.reflect_hash());
            }
        }
    }

    hasher.write_usize(this.field_len());

    hasher.finish()
}

/// Formats an enum as `Enum<Type::Variant>(...)` with variant-appropriate
/// formatting (unit: nothing, tuple: list, struct: map).
#[inline(never)]
pub fn enum_debug(this: &dyn Enum, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
        f,
        "Enum<{}::{}>(",
        this.reflect_type_path(),
        this.variant_name()
    )?;
    match this.variant_kind() {
        VariantKind::Unit => (),
        VariantKind::Tuple => {
            f.debug_list().entries(this.iter_fields()).finish()?;
        }
        VariantKind::Struct => {
            let mut debugger = f.debug_map();
            for (i, field) in this.iter_fields().enumerate() {
                let name = this.field_name_at(i).expect("valid index");
                debugger.entry(&name, &field);
            }
            debugger.finish()?;
        }
    }

    f.write_str(")")
}

/*
/// Compares two enums for ordering.
///
/// 1. If `other` is not an [`Enum`], order by [`ReflectKind`].
/// 2. Order by [`TypeId`] first; different types are never equal.
/// 3. Order by variant kind, then variant index, then variant name.
/// 4. Shorter field lists sort before longer ones.
/// 5. Unit variants are equal. Tuple variants compare fields by index
///    lexicographically. Struct variants compare field names first, then
///    field values.
#[inline(never)]
pub fn enum_cmp(this: &dyn Enum, other: &dyn Reflect) -> Ordering {
    let ReflectRef::Enum(other) = other.reflect_ref() else {
        return ReflectKind::Enum.cmp(&other.reflect_kind());
    };

    let type_ord = this.type_id().cmp(&other.type_id());
    if type_ord != Ordering::Equal {
        return type_ord;
    }

    let var_kind_ord = this.variant_kind().cmp(&other.variant_kind());
    if var_kind_ord != Ordering::Equal {
        return var_kind_ord;
    }

    let var_index_ord = this.variant_index().cmp(&other.variant_index());
    if var_index_ord != Ordering::Equal {
        return var_index_ord;
    }

    let var_name_ord = this.variant_name().cmp(&other.variant_name());
    if var_name_ord != Ordering::Equal {
        return var_name_ord;
    }

    let len_ord = this.field_len().cmp(&other.field_len());
    if len_ord != Ordering::Equal {
        return len_ord;
    }

    const MSG1: &str = "the field_len of enum should be correct";
    const MSG2: &str = "the variant_kind of enum should be correct";
    match this.variant_kind() {
        VariantKind::Unit => {},
        VariantKind::Tuple => {
            for index in 0..this.field_len() {
                let xf = this.field_at(index).expect(MSG1);
                let yf = other.field_at(index).expect(MSG1);
                let field_ord = xf.reflect_cmp(yf);
                if field_ord != Ordering::Equal {
                    return field_ord;
                }
            }
        },
        VariantKind::Struct => {
            for index in 0..this.field_len() {
                let xn = this.field_name_at(index).expect(MSG2);
                let yn = other.field_name_at(index).expect(MSG2);
                let name_ord = xn.cmp(yn);
                if name_ord != Ordering::Equal {
                    return name_ord;
                }
                let xf = this.field_at(index).expect(MSG1);
                let yf = other.field_at(index).expect(MSG1);
                let field_ord = xf.reflect_cmp(yf);
                if field_ord != Ordering::Equal {
                    return field_ord;
                }
            }
        },
    }

    Ordering::Equal
}
*/
