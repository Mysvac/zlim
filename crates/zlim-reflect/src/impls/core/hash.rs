use core::hash::BuildHasherDefault;

use crate::impls::impl_simple_type_path;

impl_simple_type_path!(@BuildHasherDefault<H>: "core", "hash", "BuildHasherDefault");
