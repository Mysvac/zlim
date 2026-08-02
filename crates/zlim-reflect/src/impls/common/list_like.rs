use core::fmt;
use core::hash::{Hash, Hasher};

use zlim_utils::format_smol;

use crate::Reflect;
use crate::db::TypeDB;
use crate::impls::CLONE_TYPE_ERROR;
use crate::impls::CONVERT_TYPE_ERROR;
use crate::ops::{ApplyError, List};

/// Applies a reflected value to a list by draining and rebuilding it.
///
/// 1. If the types are identical, clone-and-assign directly (fast path).
/// 2. If the [`TypeDB`] has a conversion from `other`'s type to `Self`,
///    clone-and-assign directly.
/// 3. Require `other` to also be a [`List`]; fail with
///    [`mismatched_kind`](ApplyError::mismatched_kind) otherwise.
/// 4. Drain the destination, saving the removed elements for potential
///    recovery.
/// 5. For each source element: clone it, then push it into the
///    destination. On clone or push failure, restore the original
///    elements via the recovery function and return the error.
#[inline(never)]
pub fn list_apply(this: &mut dyn List, other: &dyn Reflect) -> Result<(), ApplyError> {
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

    // Phase 3: cast `other` to `&dyn List`.
    let other: &dyn List = other.reflect_ref().as_list().map_err(|e| {
        ::core::hint::cold_path();
        let src = this.reflect_type_path();
        let apply = other.reflect_type_path();
        ApplyError::mismatched_kind(src, apply, e.expected, e.received)
    })?;

    // Phase 4: drain destination; define recovery for rollback on failure.
    let removed = this.drain_all();

    #[cold]
    #[inline(never)]
    fn recovery(this: &mut dyn List, removed: Vec<Box<dyn Reflect>>) {
        let _ = this.drain_all();
        for item in removed {
            this.push_back(item)
                .expect("elements originally from this list should be pushable");
        }
    }

    // Phase 5: clone each source element and push into destination.
    let other_len = other.item_len();
    for index in 0..other_len {
        const MSG: &str = "the length of list should be correct";
        let item = other.item(index).expect(MSG);

        // Clone the element; on failure, restore original list and return error.
        let v = match item.reflect_clone() {
            Ok(v) => v,
            Err(e) => {
                ::core::hint::cold_path();
                let src = this.reflect_type_path();
                let apply = other.reflect_type_path();
                let item_name = format_smol!("{index}");
                recovery(this, removed);
                return Err(ApplyError::clone_error(src, apply, &item_name, e));
            }
        };

        // Push the clone; on failure, restore original list and return error.
        if let Err(v) = this.push_back(v) {
            ::core::hint::cold_path();
            let src = this.reflect_type_path();
            let apply = other.reflect_type_path();
            let item_name = format_smol!("{index}");
            let received = v.reflect_type_path();
            let expected = match this.reflect_type_info().as_list() {
                Ok(l) => l.item_info().type_path(),
                Err(_) => "Unknown",
            };
            recovery(this, removed);
            return Err(ApplyError::mismatched_item(
                src, apply, &item_name, expected, received,
            ));
        }
    }

    Ok(())
}

/// Compares two lists for equality.
///
/// 1. Different `TypeId` → not equal.
/// 2. Different lengths → not equal.
/// 3. Compare every element pair by index; all must be reflect-equal.
#[inline(never)]
pub fn list_eq(this: &dyn List, other: &dyn Reflect) -> bool {
    if this.type_id() != other.type_id() {
        return false;
    }

    let other: &dyn List = other.reflect_ref().as_list().expect("same type");

    if this.item_len() != other.item_len() {
        return false;
    }

    this.iter_items()
        .zip(other.iter_items())
        .all(|(x, y)| x.reflect_eq(y))
}

/// Hashes a list.
///
/// 1. Hash the `TypeId` for type disambiguation.
/// 2. Hash each element's [`reflect_hash`](crate::Reflect::reflect_hash)
///    in order.
/// 3. Hash the length for disambiguation.
#[inline(never)]
pub fn list_hash(this: &dyn List) -> u64 {
    let mut hasher = crate::impls::reflect_hasher();

    this.type_id().hash(&mut hasher);

    for item in this.iter_items() {
        hasher.write_u64(item.reflect_hash());
    }

    hasher.write_usize(this.item_len());

    hasher.finish()
}

/// Formats a list as `List<Type>([elem1, elem2, ...])`.
#[inline(never)]
pub fn list_debug(this: &dyn List, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "List<{}>(", this.reflect_type_path())?;
    f.debug_list().entries(this.iter_items()).finish()?;
    f.write_str(")")
}

/*
/// Compares two lists for ordering.
///
/// 1. If `other` is not a [`List`], order by [`ReflectKind`].
/// 2. Order by [`TypeId`] first; different types are never equal.
/// 3. Shorter lists sort before longer ones.
/// 4. Compare elements pairwise by index lexicographically.
#[inline(never)]
pub fn list_cmp(this: &dyn List, other: &dyn Reflect) -> Ordering {
    let ReflectRef::List(other) = other.reflect_ref() else {
        return ReflectKind::List.cmp(&other.reflect_kind());
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
