use zlim_reflect_derive::impl_reflect;

use glam::*;

// -----------------------------------------------------------------------------
// I8Vec ( i8 Vec )
// -----------------------------------------------------------------------------

impl_reflect!(
    #[type_path = "glam::I8Vec2"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I8Vec2 {
        x: i8,
        y: i8,
    }
);

impl_reflect!(
    #[type_path = "glam::I8Vec3"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I8Vec3 {
        x: i8,
        y: i8,
        z: i8,
    }
);

impl_reflect!(
    #[type_path = "glam::I8Vec4"]
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

impl_reflect!(
    #[type_path = "glam::I16Vec2"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I16Vec2 {
        x: i16,
        y: i16,
    }
);

impl_reflect!(
    #[type_path = "glam::I16Vec3"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I16Vec3 {
        x: i16,
        y: i16,
        z: i16,
    }
);

impl_reflect!(
    #[type_path = "glam::I16Vec4"]
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

impl_reflect!(
    #[type_path = "glam::IVec2"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct IVec2 {
        x: i32,
        y: i32,
    }
);

impl_reflect!(
    #[type_path = "glam::IVec3"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct IVec3 {
        x: i32,
        y: i32,
        z: i32,
    }
);

impl_reflect!(
    #[type_path = "glam::IVec4"]
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

impl_reflect!(
    #[type_path = "glam::I64Vec2"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I64Vec2 {
        x: i64,
        y: i64,
    }
);

impl_reflect!(
    #[type_path = "glam::I64Vec3"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct I64Vec3 {
        x: i64,
        y: i64,
        z: i64,
    }
);

impl_reflect!(
    #[type_path = "glam::I64Vec4"]
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

impl_reflect!(
    #[type_path = "glam::U8Vec2"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U8Vec2 {
        x: u8,
        y: u8,
    }
);

impl_reflect!(
    #[type_path = "glam::U8Vec3"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U8Vec3 {
        x: u8,
        y: u8,
        z: u8,
    }
);

impl_reflect!(
    #[type_path = "glam::U8Vec4"]
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

impl_reflect!(
    #[type_path = "glam::U16Vec2"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U16Vec2 {
        x: u16,
        y: u16,
    }
);

impl_reflect!(
    #[type_path = "glam::U16Vec3"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U16Vec3 {
        x: u16,
        y: u16,
        z: u16,
    }
);

impl_reflect!(
    #[type_path = "glam::U16Vec4"]
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

impl_reflect!(
    #[type_path = "glam::UVec2"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct UVec2 {
        x: u32,
        y: u32,
    }
);

impl_reflect!(
    #[type_path = "glam::UVec3"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct UVec3 {
        x: u32,
        y: u32,
        z: u32,
    }
);

impl_reflect!(
    #[type_path = "glam::UVec4"]
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

impl_reflect!(
    #[type_path = "glam::U64Vec2"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U64Vec2 {
        x: u64,
        y: u64,
    }
);

impl_reflect!(
    #[type_path = "glam::U64Vec3"]
    #[reflect(Default, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
    struct U64Vec3 {
        x: u64,
        y: u64,
        z: u64,
    }
);

impl_reflect!(
    #[type_path = "glam::U64Vec4"]
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

impl_reflect!(
    #[type_path = "glam::Vec2"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Vec2 {
        x: f32,
        y: f32,
    }
);

impl_reflect!(
    #[type_path = "glam::Vec3"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Vec3 {
        x: f32,
        y: f32,
        z: f32,
    }
);

impl_reflect!(
    #[type_path = "glam::Vec4"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Vec4 {
        x: f32,
        y: f32,
        z: f32,
        w: f32,
    }
);

impl_reflect!(
    #[type_path = "glam::Vec3A"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Vec3A {
        x: f32,
        y: f32,
        z: f32,
    }
);

// -----------------------------------------------------------------------------
// DVec ( f64 Vec )
// -----------------------------------------------------------------------------

impl_reflect!(
    #[type_path = "glam::DVec2"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct DVec2 {
        x: f64,
        y: f64,
    }
);

impl_reflect!(
    #[type_path = "glam::DVec3"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct DVec3 {
        x: f64,
        y: f64,
        z: f64,
    }
);

impl_reflect!(
    #[type_path = "glam::DVec4"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
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

impl_reflect!(
    #[type_path = "glam::BVec2"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct BVec2 {
        x: bool,
        y: bool,
    }
);

impl_reflect!(
    #[type_path = "glam::BVec3"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct BVec3 {
        x: bool,
        y: bool,
        z: bool,
    }
);

impl_reflect!(
    #[type_path = "glam::BVec4"]
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

impl_reflect!(
    #[type_path = "glam::Mat2"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Mat2 {
        x_axis: Vec2,
        y_axis: Vec2,
    }
);

impl_reflect!(
    #[type_path = "glam::Mat3"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Mat3 {
        x_axis: Vec3,
        y_axis: Vec3,
        z_axis: Vec3,
    }
);

impl_reflect!(
    #[type_path = "glam::Mat4"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Mat4 {
        x_axis: Vec4,
        y_axis: Vec4,
        z_axis: Vec4,
        w_axis: Vec4,
    }
);

impl_reflect!(
    #[type_path = "glam::Mat3A"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Mat3A {
        x_axis: Vec3A,
        y_axis: Vec3A,
        z_axis: Vec3A,
    }
);

// -----------------------------------------------------------------------------
// DMat ( f64 * f64 )
// -----------------------------------------------------------------------------

impl_reflect!(
    #[type_path = "glam::DMat2"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct DMat2 {
        x_axis: DVec2,
        y_axis: DVec2,
    }
);

impl_reflect!(
    #[type_path = "glam::DMat3"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct DMat3 {
        x_axis: DVec3,
        y_axis: DVec3,
        z_axis: DVec3,
    }
);

impl_reflect!(
    #[type_path = "glam::DMat4"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
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

impl_reflect!(
    #[type_path = "glam::Affine2"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Affine2 {
        matrix2: Mat2,
        translation: Vec2,
    }
);

impl_reflect!(
    #[type_path = "glam::Affine3"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Affine3 {
        matrix3: Mat3,
        translation: Vec3,
    }
);

impl_reflect!(
    #[type_path = "glam::Affine3A"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Affine3A {
        matrix3: Mat3A,
        translation: Vec3A,
    }
);

// -----------------------------------------------------------------------------
// DAffine
// -----------------------------------------------------------------------------

impl_reflect!(
    #[type_path = "glam::DAffine2"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct DAffine2 {
        matrix2: DMat2,
        translation: DVec2,
    }
);

impl_reflect!(
    #[type_path = "glam::DAffine3"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct DAffine3 {
        matrix3: DMat3,
        translation: DVec3,
    }
);

// -----------------------------------------------------------------------------
// Quat
// -----------------------------------------------------------------------------

impl_reflect!(
    #[type_path = "glam::Quat"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
    struct Quat {
        x: f32,
        y: f32,
        z: f32,
        w: f32,
    }
);

impl_reflect!(
    #[type_path = "glam::DQuat"]
    #[reflect(Default, Clone, Debug, Eq, Deserialize, Serialize)]
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

impl_reflect!(
    #[type_path = "glam::EulerRot"]
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
