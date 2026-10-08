use std::sync::OnceLock;

use zlim_reflect_derive::impl_reflect;
use zlim_utils::mem::Global;

use crate::impls::impl_simple_type_path;
use crate::info::{NamedField, StructInfo, TupleInfo, TypeInfo, Typed, UnnamedField};

use glam::*;

macro_rules! impl_simple_typed {
    ($ty:ty, $field:ty, $($name:literal, $index:literal),* $(,)?) => {
        impl Typed for $ty {
            fn type_info() -> &'static TypeInfo {
                static ONCE: OnceLock<TypeInfo> = OnceLock::new();

                ONCE.get_or_init(|| {
                    TypeInfo::Struct(
                        StructInfo::new::<Self>(&[
                            $(NamedField::new::<$field>($name)),*
                        ])
                        .with_serde_info({
                            let info = TypeInfo::Tuple(
                                TupleInfo::dynamic::<Self>(&[
                                    $(UnnamedField::new::<$field>($index)),*
                                ])
                            );
                            Global::alloc_static(info)
                        }),
                    )
                })
            }
        }
    };
}

macro_rules! impl_mat_typed {
    (
        $ty:ty,
        $vec:ty,
        $scalar:ty,
        [$($name:literal),* $(,)?],
        [$($elem:literal),* $(,)?] $(,)?
    ) => {
        impl Typed for $ty {
            fn type_info() -> &'static TypeInfo {
                static ONCE: OnceLock<TypeInfo> = OnceLock::new();

                ONCE.get_or_init(|| {
                    TypeInfo::Struct(
                        StructInfo::new::<Self>(&[
                            $(NamedField::new::<$vec>($name)),*
                        ])
                        .with_serde_info({
                            let info = TypeInfo::Tuple(
                                TupleInfo::dynamic::<Self>(&[
                                    $(UnnamedField::new::<$scalar>($elem)),*
                                ])
                            );
                            Global::alloc_static(info)
                        }),
                    )
                })
            }
        }
    };
}

macro_rules! impl_affine_typed {
    (
        $ty:ty,
        $scalar:ty,
        [$($name:literal, $vec:ty),* $(,)?],
        [$($elem:literal),* $(,)?] $(,)?
    ) => {
        impl Typed for $ty {
            fn type_info() -> &'static TypeInfo {
                static ONCE: OnceLock<TypeInfo> = OnceLock::new();

                ONCE.get_or_init(|| {
                    TypeInfo::Struct(
                        StructInfo::new::<Self>(&[
                            $(NamedField::new::<$vec>($name)),*
                        ])
                        .with_serde_info({
                            let info = TypeInfo::Tuple(
                                TupleInfo::dynamic::<Self>(&[
                                    $(UnnamedField::new::<$scalar>($elem)),*
                                ])
                            );
                            Global::alloc_static(info)
                        }),
                    )
                })
            }
        }
    };
}

// -----------------------------------------------------------------------------
// I8Vec ( i8 Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(I8Vec2: "glam", "I8Vec2");
impl_simple_typed!(I8Vec2, i8, "x", 0, "y", 1);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I8Vec2 {
        x: i8,
        y: i8,
    }
);

impl_simple_type_path!(I8Vec3: "glam", "I8Vec3");
impl_simple_typed!(I8Vec3, i8, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I8Vec3 {
        x: i8,
        y: i8,
        z: i8,
    }
);

impl_simple_type_path!(I8Vec4: "glam", "I8Vec4");
impl_simple_typed!(I8Vec4, i8, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I8Vec4 {
        x: i8,
        y: i8,
        z: i8,
        w: i8,
    }
);

// -----------------------------------------------------------------------------
// I16Vec ( i16 Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(I16Vec2: "glam", "I16Vec2");
impl_simple_typed!(I16Vec2, i16, "x", 0, "y", 1);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I16Vec2 {
        x: i16,
        y: i16,
    }
);

impl_simple_type_path!(I16Vec3: "glam", "I16Vec3");
impl_simple_typed!(I16Vec3, i16, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I16Vec3 {
        x: i16,
        y: i16,
        z: i16,
    }
);

impl_simple_type_path!(I16Vec4: "glam", "I16Vec4");
impl_simple_typed!(I16Vec4, i16, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I16Vec4 {
        x: i16,
        y: i16,
        z: i16,
        w: i16,
    }
);

// -----------------------------------------------------------------------------
// IVec ( i32 Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(IVec2: "glam", "IVec2");
impl_simple_typed!(IVec2, i32, "x", 0, "y", 1);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct IVec2 {
        x: i32,
        y: i32,
    }
);

impl_simple_type_path!(IVec3: "glam", "IVec3");
impl_simple_typed!(IVec3, i32, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct IVec3 {
        x: i32,
        y: i32,
        z: i32,
    }
);

impl_simple_type_path!(IVec4: "glam", "IVec4");
impl_simple_typed!(IVec4, i32, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct IVec4 {
        x: i32,
        y: i32,
        z: i32,
        w: i32,
    }
);

// -----------------------------------------------------------------------------
// I64Vec ( i64 Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(I64Vec2: "glam", "I64Vec2");
impl_simple_typed!(I64Vec2, i64, "x", 0, "y", 1);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I64Vec2 {
        x: i64,
        y: i64,
    }
);

impl_simple_type_path!(I64Vec3: "glam", "I64Vec3");
impl_simple_typed!(I64Vec3, i64, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I64Vec3 {
        x: i64,
        y: i64,
        z: i64,
    }
);

impl_simple_type_path!(I64Vec4: "glam", "I64Vec4");
impl_simple_typed!(I64Vec4, i64, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I64Vec4 {
        x: i64,
        y: i64,
        z: i64,
        w: i64,
    }
);

// -----------------------------------------------------------------------------
// U8Vec ( u8 Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(U8Vec2: "glam", "U8Vec2");
impl_simple_typed!(U8Vec2, u8, "x", 0, "y", 1);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U8Vec2 {
        x: u8,
        y: u8,
    }
);

impl_simple_type_path!(U8Vec3: "glam", "U8Vec3");
impl_simple_typed!(U8Vec3, u8, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U8Vec3 {
        x: u8,
        y: u8,
        z: u8,
    }
);

impl_simple_type_path!(U8Vec4: "glam", "U8Vec4");
impl_simple_typed!(U8Vec4, u8, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U8Vec4 {
        x: u8,
        y: u8,
        z: u8,
        w: u8,
    }
);

// -----------------------------------------------------------------------------
// U16Vec ( u16 Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(U16Vec2: "glam", "U16Vec2");
impl_simple_typed!(U16Vec2, u16, "x", 0, "y", 1);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U16Vec2 {
        x: u16,
        y: u16,
    }
);

impl_simple_type_path!(U16Vec3: "glam", "U16Vec3");
impl_simple_typed!(U16Vec3, u16, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U16Vec3 {
        x: u16,
        y: u16,
        z: u16,
    }
);

impl_simple_type_path!(U16Vec4: "glam", "U16Vec4");
impl_simple_typed!(U16Vec4, u16, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U16Vec4 {
        x: u16,
        y: u16,
        z: u16,
        w: u16,
    }
);

// -----------------------------------------------------------------------------
// UVec ( u32 Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(UVec2: "glam", "UVec2");
impl_simple_typed!(UVec2, u32, "x", 0, "y", 1);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct UVec2 {
        x: u32,
        y: u32,
    }
);

impl_simple_type_path!(UVec3: "glam", "UVec3");
impl_simple_typed!(UVec3, u32, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct UVec3 {
        x: u32,
        y: u32,
        z: u32,
    }
);

impl_simple_type_path!(UVec4: "glam", "UVec4");
impl_simple_typed!(UVec4, u32, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct UVec4 {
        x: u32,
        y: u32,
        z: u32,
        w: u32,
    }
);

// -----------------------------------------------------------------------------
// U64Vec ( u64 Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(U64Vec2: "glam", "U64Vec2");
impl_simple_typed!(U64Vec2, u64, "x", 0, "y", 1);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U64Vec2 {
        x: u64,
        y: u64,
    }
);

impl_simple_type_path!(U64Vec3: "glam", "U64Vec3");
impl_simple_typed!(U64Vec3, u64, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U64Vec3 {
        x: u64,
        y: u64,
        z: u64,
    }
);

impl_simple_type_path!(U64Vec4: "glam", "U64Vec4");
impl_simple_typed!(U64Vec4, u64, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U64Vec4 {
        x: u64,
        y: u64,
        z: u64,
        w: u64,
    }
);

// -----------------------------------------------------------------------------
// Vec ( f32 Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(Vec2: "glam", "Vec2");
impl_simple_typed!(Vec2, f32, "x", 0, "y", 1);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Vec2 {
        x: f32,
        y: f32,
    }
);

impl_simple_type_path!(Vec3: "glam", "Vec3");
impl_simple_typed!(Vec3, f32, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Vec3 {
        x: f32,
        y: f32,
        z: f32,
    }
);

impl_simple_type_path!(Vec4: "glam", "Vec4");
impl_simple_typed!(Vec4, f32, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Vec4 {
        x: f32,
        y: f32,
        z: f32,
        w: f32,
    }
);

impl_simple_type_path!(Vec3A: "glam", "Vec3A");
impl_simple_typed!(Vec3A, f32, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Vec3A {
        x: f32,
        y: f32,
        z: f32,
    }
);

// -----------------------------------------------------------------------------
// DVec ( f64 Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(DVec2: "glam", "DVec2");
impl_simple_typed!(DVec2, f64, "x", 0, "y", 1);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DVec2 {
        x: f64,
        y: f64,
    }
);

impl_simple_type_path!(DVec3: "glam", "DVec3");
impl_simple_typed!(DVec3, f64, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DVec3 {
        x: f64,
        y: f64,
        z: f64,
    }
);

impl_simple_type_path!(DVec4: "glam", "DVec4");
impl_simple_typed!(DVec4, f64, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DVec4 {
        x: f64,
        y: f64,
        z: f64,
        w: f64,
    }
);

// -----------------------------------------------------------------------------
// BVec ( bool Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(BVec2: "glam", "BVec2");
impl_simple_typed!(BVec2, bool, "x", 0, "y", 1);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct BVec2 {
        x: bool,
        y: bool,
    }
);

impl_simple_type_path!(BVec3: "glam", "BVec3");
impl_simple_typed!(BVec3, bool, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct BVec3 {
        x: bool,
        y: bool,
        z: bool,
    }
);

impl_simple_type_path!(BVec4: "glam", "BVec4");
impl_simple_typed!(BVec4, bool, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct BVec4 {
        x: bool,
        y: bool,
        z: bool,
        w: bool,
    }
);

/* ↓ does not support deref ↓

impl_simple_type_path!(BVec3A: "glam", "BVec3A");
impl_simple_typed!(BVec3A, bool, "x", 0, "y", 1, "z", 2);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct BVec3A {
        x: bool,
        y: bool,
        z: bool,
    }
);

impl_simple_type_path!(BVec4A: "glam", "BVec4A");
impl_simple_typed!(BVec4A, bool, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct BVec4A {
        x: bool,
        y: bool,
        z: bool,
        w: bool,
    }
);
*/

// -----------------------------------------------------------------------------
// Mat ( f32 * f32 )
// -----------------------------------------------------------------------------

impl_simple_type_path!(Mat2: "glam", "Mat2");
impl_mat_typed!(Mat2, Vec2, f32, ["x_axis", "y_axis"], [0, 1, 2, 3],);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Mat2 {
        x_axis: Vec2,
        y_axis: Vec2,
    }
);

impl_simple_type_path!(Mat3: "glam", "Mat3");
impl_mat_typed!(
    Mat3,
    Vec3,
    f32,
    ["x_axis", "y_axis", "z_axis"],
    [0, 1, 2, 3, 4, 5, 6, 7, 8],
);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Mat3 {
        x_axis: Vec3,
        y_axis: Vec3,
        z_axis: Vec3,
    }
);

impl_simple_type_path!(Mat4: "glam", "Mat4");
impl_mat_typed!(
    Mat4,
    Vec4,
    f32,
    ["x_axis", "y_axis", "z_axis", "w_axis"],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Mat4 {
        x_axis: Vec4,
        y_axis: Vec4,
        z_axis: Vec4,
        w_axis: Vec4,
    }
);

impl_simple_type_path!(Mat3A: "glam", "Mat3A");
impl_mat_typed!(
    Mat3A,
    Vec3A,
    f32,
    ["x_axis", "y_axis", "z_axis"],
    [0, 1, 2, 3, 4, 5, 6, 7, 8],
);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Mat3A {
        x_axis: Vec3A,
        y_axis: Vec3A,
        z_axis: Vec3A,
    }
);

// -----------------------------------------------------------------------------
// DMat ( f64 * f64 )
// -----------------------------------------------------------------------------

impl_simple_type_path!(DMat2: "glam", "DMat2");
impl_mat_typed!(DMat2, DVec2, f64, ["x_axis", "y_axis"], [0, 1, 2, 3],);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DMat2 {
        x_axis: DVec2,
        y_axis: DVec2,
    }
);

impl_simple_type_path!(DMat3: "glam", "DMat3");
impl_mat_typed!(
    DMat3,
    DVec3,
    f64,
    ["x_axis", "y_axis", "z_axis"],
    [0, 1, 2, 3, 4, 5, 6, 7, 8],
);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DMat3 {
        x_axis: DVec3,
        y_axis: DVec3,
        z_axis: DVec3,
    }
);

impl_simple_type_path!(DMat4: "glam", "DMat4");
impl_mat_typed!(
    DMat4,
    DVec4,
    f64,
    ["x_axis", "y_axis", "z_axis", "w_axis"],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DMat4 {
        x_axis: DVec4,
        y_axis: DVec4,
        z_axis: DVec4,
        w_axis: DVec4,
    }
);

// -----------------------------------------------------------------------------
// Affine
// -----------------------------------------------------------------------------

impl_simple_type_path!(Affine2: "glam", "Affine2");
impl_affine_typed!(
    Affine2,
    f32,
    ["matrix2", Mat2, "translation", Vec2],
    [0, 1, 2, 3, 4, 5],
);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Affine2 {
        matrix2: Mat2,
        translation: Vec2,
    }
);

impl_simple_type_path!(Affine3: "glam", "Affine3");
impl_affine_typed!(
    Affine3,
    f32,
    ["matrix3", Mat3, "translation", Vec3],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Affine3 {
        matrix3: Mat3,
        translation: Vec3,
    }
);

impl_simple_type_path!(Affine3A: "glam", "Affine3A");
impl_affine_typed!(
    Affine3A,
    f32,
    ["matrix3", Mat3A, "translation", Vec3A],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Affine3A {
        matrix3: Mat3A,
        translation: Vec3A,
    }
);

// -----------------------------------------------------------------------------
// DAffine
// -----------------------------------------------------------------------------

impl_simple_type_path!(DAffine2: "glam", "DAffine2");
impl_affine_typed!(
    DAffine2,
    f64,
    ["matrix2", DMat2, "translation", DVec2],
    [0, 1, 2, 3, 4, 5],
);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DAffine2 {
        matrix2: DMat2,
        translation: DVec2,
    }
);

impl_simple_type_path!(DAffine3: "glam", "DAffine3");
impl_affine_typed!(
    DAffine3,
    f64,
    ["matrix3", DMat3, "translation", DVec3],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DAffine3 {
        matrix3: DMat3,
        translation: DVec3,
    }
);

// -----------------------------------------------------------------------------
// Quat
// -----------------------------------------------------------------------------

impl_simple_type_path!(Quat: "glam", "Quat");
impl_simple_typed!(Quat, f32, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Quat {
        x: f32,
        y: f32,
        z: f32,
        w: f32,
    }
);

impl_simple_type_path!(DQuat: "glam", "DQuat");
impl_simple_typed!(DQuat, f64, "x", 0, "y", 1, "z", 2, "w", 3);
impl_reflect!(
    #[reflect(Typed = false)]
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DQuat {
        x: f64,
        y: f64,
        z: f64,
        w: f64,
    }
);

// -----------------------------------------------------------------------------
// EulerRot
// -----------------------------------------------------------------------------

impl_simple_type_path!(EulerRot: "glam", "EulerRot");
impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Deserialize, Serialize)]
    enum EulerRot {
        ZYX,
        ZXY,
        YXZ,
        YZX,
        XYZ,
        XZY,
        ZYZ,
        ZXZ,
        YXY,
        YZY,
        XYX,
        XZX,
        ZYXEx,
        ZXYEx,
        YXZEx,
        YZXEx,
        XYZEx,
        XZYEx,
        ZYZEx,
        ZXZEx,
        YXYEx,
        YZYEx,
        XYXEx,
        XZXEx,
    }
);

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use crate::info::Typed;

    /// The number of fields a struct-or-tuple `TypeInfo` exposes.
    fn field_len(ty: &'static TypeInfo) -> usize {
        match ty {
            TypeInfo::Struct(info) => info.field_len(),
            TypeInfo::Tuple(info) => info.field_len(),
            other => panic!("expected a struct or tuple info, got {other:?}"),
        }
    }

    /// `Typed = false` must leave the type's own info a named struct (that is
    /// what `Struct` reflection reads) while attaching a tuple serde info that
    /// matches glam's `serialize_tuple_struct` wire format.
    fn check(ty: &'static TypeInfo, fields: usize) {
        let TypeInfo::Struct(info) = ty else {
            panic!("expected the reflected info to be a struct, got {ty:?}");
        };

        let serde_info = info
            .serde_info()
            .expect("a serde info must be attached when Typed = false");
        let TypeInfo::Tuple(tuple) = serde_info else {
            panic!("expected a tuple serde info, got {serde_info:?}");
        };

        assert_eq!(info.field_len(), fields, "reflected field count");
        assert_eq!(tuple.field_len(), fields, "serde field count");
        assert_eq!(field_len(ty), field_len(serde_info));
    }

    #[test]
    fn integer_vectors_reflect_as_structs_with_tuple_serde_info() {
        check(I8Vec3::type_info(), 3);
        check(I16Vec2::type_info(), 2);
        check(IVec4::type_info(), 4);
        check(I64Vec2::type_info(), 2);
        check(U8Vec3::type_info(), 3);
        check(U16Vec3::type_info(), 3);
        check(UVec4::type_info(), 4);
        check(U64Vec2::type_info(), 2);
    }

    #[test]
    fn float_and_bool_vectors_reflect_as_structs_with_tuple_serde_info() {
        check(Vec2::type_info(), 2);
        check(Vec3::type_info(), 3);
        check(Vec4::type_info(), 4);
        check(Vec3A::type_info(), 3);
        check(DVec4::type_info(), 4);
        check(BVec2::type_info(), 2);
        check(BVec3::type_info(), 3);
    }

    #[test]
    fn quaternions_reflect_as_structs_with_tuple_serde_info() {
        check(Quat::type_info(), 4);
        check(DQuat::type_info(), 4);
    }

    /// A matrix is the one case where the two shapes genuinely differ: reflection
    /// exposes one field per column (so `Struct` reflection can read `x_axis`),
    /// while serde writes every scalar, column-major, as one flat tuple — which
    /// is exactly what glam's `serialize_tuple_struct` does.
    fn check_matrix(ty: &'static TypeInfo, columns: usize, scalars: usize) {
        let TypeInfo::Struct(info) = ty else {
            panic!("expected the reflected info to be a struct, got {ty:?}");
        };
        let serde_info = info.serde_info().expect("a matrix carries a serde info");
        let TypeInfo::Tuple(tuple) = serde_info else {
            panic!("expected a tuple serde info, got {serde_info:?}");
        };

        assert_eq!(info.field_len(), columns, "reflected field count");
        assert_eq!(tuple.field_len(), scalars, "serde field count");
        assert_ne!(columns, scalars, "a matrix flattens columns into scalars");

        let indices: Vec<usize> = (0..tuple.field_len())
            .map(|i| tuple.field(i).expect("field at index").index())
            .collect();
        assert_eq!(indices, (0..scalars).collect::<Vec<_>>());
    }

    #[test]
    fn f32_matrices_reflect_columns_and_serde_flattens_scalars() {
        check_matrix(Mat2::type_info(), 2, 4);
        check_matrix(Mat3::type_info(), 3, 9);
        check_matrix(Mat4::type_info(), 4, 16);
        check_matrix(Mat3A::type_info(), 3, 9);
    }

    #[test]
    fn f64_matrices_reflect_columns_and_serde_flattens_scalars() {
        check_matrix(DMat2::type_info(), 2, 4);
        check_matrix(DMat3::type_info(), 3, 9);
        check_matrix(DMat4::type_info(), 4, 16);
    }

    /// The column names are the reflected field names, in order.
    #[test]
    fn matrix_columns_are_named_after_the_axes() {
        let TypeInfo::Struct(info) = Mat3::type_info() else {
            panic!("expected a struct");
        };
        assert_eq!(info.field_names(), &["x_axis", "y_axis", "z_axis"]);

        let TypeInfo::Struct(mat4) = Mat4::type_info() else {
            panic!("expected a struct");
        };
        assert_eq!(
            mat4.field_names(),
            &["x_axis", "y_axis", "z_axis", "w_axis"]
        );
    }

    /// An affine transform is the widest gap between the two shapes: reflection
    /// keeps the matrix and the translation as two fields, while serde writes
    /// every scalar of the layout as one flat tuple.
    fn check_affine(ty: &'static TypeInfo, scalars: usize, column: &str) {
        let TypeInfo::Struct(info) = ty else {
            panic!("expected the reflected info to be a struct, got {ty:?}");
        };
        let serde_info = info
            .serde_info()
            .expect("an affine type carries a serde info");
        let TypeInfo::Tuple(tuple) = serde_info else {
            panic!("expected a tuple serde info, got {serde_info:?}");
        };

        assert_eq!(info.field_len(), 2, "matrix + translation");
        assert_eq!(info.name_at(0), Some(column));
        assert_eq!(info.name_at(1), Some("translation"));
        assert_eq!(tuple.field_len(), scalars, "serde field count");
        assert_eq!(tuple.field(0).expect("first").index(), 0);
        assert_eq!(
            tuple.field(scalars - 1).expect("last").index(),
            scalars - 1,
            "indices are contiguous from zero"
        );
    }

    /// An `Affine3` stores `matrix3` + `translation`, yet the serde impl reads
    /// `self.x_axis`/`self.w_axis`. Those are not fields of `Affine3`: glam
    /// implements `Deref<Target = Cols4<Vec3>>`, and a pointer cast shows the
    /// two layouts are byte-identical (four consecutive `Vec3`). Field access
    /// therefore auto-derefs to the four columns, and `w_axis` *is* the
    /// translation. This test pins that mapping down, so the 12 indices we
    /// declare for the serde info cannot silently drift from it.
    #[test]
    fn affine3_derefs_to_its_four_columns() {
        let affine = Affine3::from_cols(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(7.0, 8.0, 9.0),
            Vec3::new(10.0, 11.0, 12.0),
        );

        // These are not `Affine3` fields; they come from `Deref`.
        assert_eq!(affine.x_axis, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(affine.y_axis, Vec3::new(4.0, 5.0, 6.0));
        assert_eq!(affine.z_axis, Vec3::new(7.0, 8.0, 9.0));
        assert_eq!(affine.w_axis, Vec3::new(10.0, 11.0, 12.0));

        // The four columns are exactly the 9 matrix scalars plus the translation,
        // which is the order the serde tuple writes them in.
        assert_eq!(
            affine.matrix3,
            Mat3::from_cols(affine.x_axis, affine.y_axis, affine.z_axis)
        );
        assert_eq!(affine.translation, affine.w_axis);

        // Byte-for-byte, `Affine3` and its column view are the same 12 scalars.
        assert_eq!(size_of::<Affine3>(), 12 * size_of::<f32>());
        let scalars: [f32; 12] = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        ];
        assert_eq!(affine, Affine3::from_cols_array(&scalars));
    }

    #[test]
    fn affine2_serde_flattens_the_3x2_layout() {
        check_affine(Affine2::type_info(), 6, "matrix2");
        check_affine(DAffine2::type_info(), 6, "matrix2");
    }

    #[test]
    fn affine3_serde_flattens_the_4x3_layout() {
        // glam lays an `Affine3` out column-wise as a 4x3 matrix, putting the
        // translation in `w_axis`, so serde writes 12 scalars, not 9 + 3.
        check_affine(Affine3::type_info(), 12, "matrix3");
        check_affine(Affine3A::type_info(), 12, "matrix3");
        check_affine(DAffine3::type_info(), 12, "matrix3");
    }

    /// The reflected field names are the component names, in order.
    #[test]
    fn reflected_fields_are_named_after_the_components() {
        let TypeInfo::Struct(info) = Vec3::type_info() else {
            panic!("expected a struct");
        };
        assert_eq!(info.field_names(), &["x", "y", "z"]);

        let TypeInfo::Struct(quat) = Quat::type_info() else {
            panic!("expected a struct");
        };
        assert_eq!(quat.field_names(), &["x", "y", "z", "w"]);
    }

    /// The serde tuple indices must be positional, starting at zero — that is
    /// how `serialize_tuple_struct` reads them.
    #[test]
    fn serde_fields_are_indexed_from_zero() {
        let TypeInfo::Struct(info) = Vec3::type_info() else {
            panic!("expected a struct");
        };
        let TypeInfo::Tuple(tuple) = info.serde_info().expect("serde info") else {
            panic!("expected a tuple serde info");
        };

        assert_eq!(tuple.field_len(), 3);

        // The indices are contiguous, in order, and start at zero.
        let indices: Vec<usize> = (0..tuple.field_len())
            .map(|i| tuple.field(i).expect("field at index").index())
            .collect();
        assert_eq!(indices, [0, 1, 2]);

        for position in 0..tuple.field_len() {
            let field = tuple.field(position).expect("field at position");
            assert_eq!(
                field.index(),
                position,
                "field {position} must carry its own index"
            );
            assert!(field.type_is::<f32>(), "every Vec3 component is an f32");
        }

        assert!(tuple.field(3).is_none(), "there is no fourth component");
    }
}
