use zlim_reflect_derive::impl_reflect;

use crate::impls::impl_simple_type_path;

use glam::*;

// -----------------------------------------------------------------------------
// I8Vec ( i8 Vec )
// -----------------------------------------------------------------------------

impl_simple_type_path!(I8Vec2: "glam", "I8Vec2");

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I8Vec2 {
        x: i8,
        y: i8,
    }
);

impl_simple_type_path!(I8Vec3: "glam", "I8Vec3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I8Vec3 {
        x: i8,
        y: i8,
        z: i8,
    }
);

impl_simple_type_path!(I8Vec4: "glam", "I8Vec4");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I16Vec2 {
        x: i16,
        y: i16,
    }
);

impl_simple_type_path!(I16Vec3: "glam", "I16Vec3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I16Vec3 {
        x: i16,
        y: i16,
        z: i16,
    }
);

impl_simple_type_path!(I16Vec4: "glam", "I16Vec4");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct IVec2 {
        x: i32,
        y: i32,
    }
);

impl_simple_type_path!(IVec3: "glam", "IVec3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct IVec3 {
        x: i32,
        y: i32,
        z: i32,
    }
);

impl_simple_type_path!(IVec4: "glam", "IVec4");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I64Vec2 {
        x: i64,
        y: i64,
    }
);

impl_simple_type_path!(I64Vec3: "glam", "I64Vec3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I64Vec3 {
        x: i64,
        y: i64,
        z: i64,
    }
);

impl_simple_type_path!(I64Vec4: "glam", "I64Vec4");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U8Vec2 {
        x: u8,
        y: u8,
    }
);

impl_simple_type_path!(U8Vec3: "glam", "U8Vec3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U8Vec3 {
        x: u8,
        y: u8,
        z: u8,
    }
);

impl_simple_type_path!(U8Vec4: "glam", "U8Vec4");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U16Vec2 {
        x: u16,
        y: u16,
    }
);

impl_simple_type_path!(U16Vec3: "glam", "U16Vec3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U16Vec3 {
        x: u16,
        y: u16,
        z: u16,
    }
);

impl_simple_type_path!(U16Vec4: "glam", "U16Vec4");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct UVec2 {
        x: u32,
        y: u32,
    }
);

impl_simple_type_path!(UVec3: "glam", "UVec3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct UVec3 {
        x: u32,
        y: u32,
        z: u32,
    }
);

impl_simple_type_path!(UVec4: "glam", "UVec4");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U64Vec2 {
        x: u64,
        y: u64,
    }
);

impl_simple_type_path!(U64Vec3: "glam", "U64Vec3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U64Vec3 {
        x: u64,
        y: u64,
        z: u64,
    }
);

impl_simple_type_path!(U64Vec4: "glam", "U64Vec4");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Vec2 {
        x: f32,
        y: f32,
    }
);

impl_simple_type_path!(Vec3: "glam", "Vec3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Vec3 {
        x: f32,
        y: f32,
        z: f32,
    }
);

impl_simple_type_path!(Vec4: "glam", "Vec4");

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Vec4 {
        x: f32,
        y: f32,
        z: f32,
        w: f32,
    }
);

impl_simple_type_path!(Vec3A: "glam", "Vec3A");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DVec2 {
        x: f64,
        y: f64,
    }
);

impl_simple_type_path!(DVec3: "glam", "DVec3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DVec3 {
        x: f64,
        y: f64,
        z: f64,
    }
);

impl_simple_type_path!(DVec4: "glam", "DVec4");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct BVec2 {
        x: bool,
        y: bool,
    }
);

impl_simple_type_path!(BVec3: "glam", "BVec3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct BVec3 {
        x: bool,
        y: bool,
        z: bool,
    }
);

impl_simple_type_path!(BVec4: "glam", "BVec4");

impl_reflect!(
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct BVec4 {
        x: bool,
        y: bool,
        z: bool,
        w: bool,
    }
);

// impl_reflect!(
//     #[type_path = "glam::BVec3A"]
//     #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
//     struct BVec3A {
//         x: bool,
//         y: bool,
//         z: bool,
//     }
// );

// impl_reflect!(
//     #[type_path = "glam::BVec4A"]
//     #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
//     struct BVec4A {
//         x: bool,
//         y: bool,
//         z: bool,
//         w: bool,
//     }
// );

// -----------------------------------------------------------------------------
// Mat ( f32 * f32 )
// -----------------------------------------------------------------------------

impl_simple_type_path!(Mat2: "glam", "Mat2");

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Mat2 {
        x_axis: Vec2,
        y_axis: Vec2,
    }
);

impl_simple_type_path!(Mat3: "glam", "Mat3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Mat3 {
        x_axis: Vec3,
        y_axis: Vec3,
        z_axis: Vec3,
    }
);

impl_simple_type_path!(Mat4: "glam", "Mat4");

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Mat4 {
        x_axis: Vec4,
        y_axis: Vec4,
        z_axis: Vec4,
        w_axis: Vec4,
    }
);

impl_simple_type_path!(Mat3A: "glam", "Mat3A");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DMat2 {
        x_axis: DVec2,
        y_axis: DVec2,
    }
);

impl_simple_type_path!(DMat3: "glam", "DMat3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DMat3 {
        x_axis: DVec3,
        y_axis: DVec3,
        z_axis: DVec3,
    }
);

impl_simple_type_path!(DMat4: "glam", "DMat4");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Affine2 {
        matrix2: Mat2,
        translation: Vec2,
    }
);

impl_simple_type_path!(Affine3: "glam", "Affine3");

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Affine3 {
        matrix3: Mat3,
        translation: Vec3,
    }
);

impl_simple_type_path!(Affine3A: "glam", "Affine3A");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct DAffine2 {
        matrix2: DMat2,
        translation: DVec2,
    }
);

impl_simple_type_path!(DAffine3: "glam", "DAffine3");

impl_reflect!(
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

impl_reflect!(
    #[reflect(Default, Clone, Debug, /* Eq, */ Deserialize, Serialize)]
    struct Quat {
        x: f32,
        y: f32,
        z: f32,
        w: f32,
    }
);

impl_simple_type_path!(DQuat: "glam", "DQuat");

impl_reflect!(
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
