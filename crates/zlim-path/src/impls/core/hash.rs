use super::impl_simple_type_path;
use crate::path::{PathCell, TypePath, concat};
use core::hash::BuildHasherDefault;

impl_simple_type_path!(@BuildHasherDefault<H>: "core", "hash", "BuildHasherDefault");
