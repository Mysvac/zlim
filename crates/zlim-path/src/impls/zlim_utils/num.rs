use super::impl_simple_type_path;
use crate::path::TypePath;
use zlim_utils::num::*;

impl_simple_type_path!(NonMaxU8:    "zlim_utils", "num", "NonMaxU8");
impl_simple_type_path!(NonMaxU16:   "zlim_utils", "num", "NonMaxU16");
impl_simple_type_path!(NonMaxU32:   "zlim_utils", "num", "NonMaxU32");
impl_simple_type_path!(NonMaxU64:   "zlim_utils", "num", "NonMaxU64");
impl_simple_type_path!(NonMaxU128:  "zlim_utils", "num", "NonMaxU128");
impl_simple_type_path!(NonMaxUsize: "zlim_utils", "num", "NonMaxUsize");
impl_simple_type_path!(NonMaxI8:    "zlim_utils", "num", "NonMaxI8");
impl_simple_type_path!(NonMaxI16:   "zlim_utils", "num", "NonMaxI16");
impl_simple_type_path!(NonMaxI32:   "zlim_utils", "num", "NonMaxI32");
impl_simple_type_path!(NonMaxI64:   "zlim_utils", "num", "NonMaxI64");
impl_simple_type_path!(NonMaxI128:  "zlim_utils", "num", "NonMaxI128");
impl_simple_type_path!(NonMaxIsize: "zlim_utils", "num", "NonMaxIsize");

// -----------------------------------------------------------------------------
// tests

#[cfg(test)]
mod tests {
    use crate::path::TypePath;
    use zlim_utils::num::*;

    #[test]
    fn non_max() {
        assert_eq!(NonMaxU8::type_path(), "zlim_utils::num::NonMaxU8");
        assert_eq!(NonMaxU8::type_name(), "NonMaxU8");
        assert_eq!(NonMaxU8::IDENT, "NonMaxU8");
        assert_eq!(NonMaxU8::CRATE, Some("zlim_utils"));
        assert_eq!(NonMaxU8::MODULE, Some("zlim_utils::num"));
        assert_eq!(NonMaxIsize::type_path(), "zlim_utils::num::NonMaxIsize");
        assert_eq!(NonMaxUsize::type_path(), "zlim_utils::num::NonMaxUsize");
    }
}
