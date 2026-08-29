use core::ops::Mul;

use serde::{Deserialize, Serialize};
use zlim_core::derive::Component;
use zlim_math::ops;
use zlim_math::{Affine3A, Dir3, Isometry3d, Mat3, Mat4, Quat, Vec3, Vec3A};
use zlim_reflect::Reflect;

// -----------------------------------------------------------------------------
// Transform

/// An affine transformation from entity-local coordinates to worldspace coordinates.
///
/// You cannot directly mutate [`GlobalTransform`]; instead, you change an entity's transform by manipulating
/// its [`Transform`], which indirectly causes Zlim Engine to update its [`GlobalTransform`].
///
/// - To get the global transform of an entity, you should get its [`GlobalTransform`].
///
/// - For transform hierarchies to work correctly, you must have both a [`Transform`] and a [`GlobalTransform`].
///   [`GlobalTransform`] is automatically inserted whenever [`Transform`] is inserted.
#[derive(Debug, PartialEq, Clone, Copy)]
#[derive(Component, Reflect, Serialize, Deserialize)]
#[reflect(Debug, Clone, Default, Serialize, Deserialize)]
#[type_path = "zlim_transform::GlobalTransform"]
pub struct GlobalTransform(Affine3A);

impl From<Affine3A> for GlobalTransform {
    fn from(value: Affine3A) -> Self {
        Self(value)
    }
}

impl From<GlobalTransform> for Affine3A {
    fn from(value: GlobalTransform) -> Self {
        value.0
    }
}

impl GlobalTransform {
    /// An identity [`GlobalTransform`] that maps all points in space to themselves.
    pub const IDENTITY: Self = Self(Affine3A::IDENTITY);
}

impl Default for GlobalTransform {
    /// return [`GlobalTransform::IDENTITY`]
    fn default() -> Self {
        GlobalTransform::IDENTITY
    }
}

// -----------------------------------------------------------------------------
// Transform

/// Describe the position of an entity.
///
/// If the entity has a parent, the position is relative to its parent position.
///
/// - To place or move an entity, you should set its [`Transform`].
///
/// - To get the global transform of an entity, you should get its [`GlobalTransform`].
///
/// - To be displayed, an entity must have both a [`Transform`] and a [`GlobalTransform`].
///
/// [`GlobalTransform`] is automatically inserted whenever [`Transform`] is inserted.
///
/// Transforms compose from right to left: if `t1` and `t2` are transforms, then `t1 * t2`
/// corresponds to applying `t2` *first*, *then* applying `t1`.
///
/// Note: [`Transform`]'s current [change detection] actually reflects changes in change detection itself.
/// This is because we rely on Transform changes to determine whether GlobalTransform needs updating.
///
/// 1. If the user modifies Transform, Transform will be marked as changed — this is normal.
///
/// 2. When operating on entities via APIs like [`reparent_in_place`], although the Transform value
///    may be modified, Transform's change detection is not triggered, preventing subsequent
///    automatic updates to GlobalTransform.
///
/// 3. In the hierarchy, if a parent entity's Transform changes, the propagation phase will mark
///    all child entities as changed (even if their Transform hasn't changed).
///
/// [change detection]: zlim_core::tick::DetectChanges
/// [`reparent_in_place`]: crate::EntityTransformExt::reparent_in_place
#[derive(Debug, PartialEq, Clone, Copy)]
#[derive(Component, Reflect, Serialize, Deserialize)]
#[reflect(Debug, Clone, Default, Serialize, Deserialize)]
#[type_path = "zlim_transform::Transform"]
#[component(serialize)]
#[require(GlobalTransform)]
pub struct Transform {
    /// Position of the entity. In 2d, the last value of the `Vec3` is used for z-ordering.
    pub translation: Vec3,
    /// Rotation of the entity.
    pub rotation: Quat,
    /// Scale of the entity.
    pub scale: Vec3,
}

impl Transform {
    /// An identity [`Transform`] with no translation,
    /// rotation, and a scale of 1 on all axes.
    pub const IDENTITY: Self = Transform {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };
}

impl Default for Transform {
    /// return [`Transform::IDENTITY`]
    fn default() -> Self {
        Transform::IDENTITY
    }
}

// -----------------------------------------------------------------------------
// GlobalTransform

// internal
impl GlobalTransform {
    #[inline]
    #[doc(hidden)]
    pub fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self::from_translation(Vec3::new(x, y, z))
    }

    #[inline]
    #[doc(hidden)]
    pub fn from_translation(translation: Vec3) -> Self {
        GlobalTransform(Affine3A::from_translation(translation))
    }

    #[inline]
    #[doc(hidden)]
    pub fn from_rotation(rotation: Quat) -> Self {
        GlobalTransform(Affine3A::from_rotation_translation(rotation, Vec3::ZERO))
    }

    #[inline]
    #[doc(hidden)]
    pub fn from_scale(scale: Vec3) -> Self {
        GlobalTransform(Affine3A::from_scale(scale))
    }

    #[inline]
    #[doc(hidden)]
    pub fn from_isometry(iso: Isometry3d) -> Self {
        Self(iso.into())
    }
}

macro_rules! impl_local_axis {
    ($pos_name: ident, $neg_name: ident, $axis: ident) => {
        #[doc=core::concat!("Return the local ", core::stringify!($pos_name), " vector (", core::stringify!($axis) ,").")]
        #[inline]
        pub fn $pos_name(&self) -> Dir3 {
            Dir3::new_unchecked((self.0.matrix3 * Vec3::$axis).normalize())
        }

        #[doc=core::concat!("Return the local ", core::stringify!($neg_name), " vector (-", core::stringify!($axis) ,").")]
        #[inline]
        pub fn $neg_name(&self) -> Dir3 {
            -Dir3::new_unchecked((self.0.matrix3 * Vec3::$axis).normalize())
        }
    };
}

impl GlobalTransform {
    impl_local_axis!(right, left, X);
    impl_local_axis!(up, down, Y);
    impl_local_axis!(back, forward, Z);

    /// Returns the 3d affine transformation matrix as an [`Affine3A`].
    #[inline]
    pub fn affine(&self) -> Affine3A {
        self.0
    }

    /// Get an upper bound of the radius from the given `extents`.
    #[inline]
    pub fn radius_vec3a(&self, extents: Vec3A) -> f32 {
        (self.0.matrix3 * extents).length()
    }

    /// Get the translation as a [`Vec3`].
    #[inline]
    pub fn translation(&self) -> Vec3 {
        self.0.translation.into()
    }

    /// Get the translation as a [`Vec3A`].
    #[inline]
    pub fn translation_vec3a(&self) -> Vec3A {
        self.0.translation
    }

    /// Get the scale as a [`Vec3`].
    ///
    /// The transform is expected to be non-degenerate and without shearing,
    /// or the output will be invalid.
    ///
    /// Some of the computations overlap with `to_scale_rotation_translation`,
    /// which means you should use it instead if you also need rotation.
    #[inline]
    pub fn scale(&self) -> Vec3 {
        // Formula based on glam's implementation
        // https://github.com/bitshifter/glam-rs/blob/2e4443e70c709710dfb25958d866d29b11ed3e2b/src/f32/affine3a.rs#L290
        let det = self.0.matrix3.determinant();
        Vec3::new(
            self.0.matrix3.x_axis.length() * ops::copysign(1., det),
            self.0.matrix3.y_axis.length(),
            self.0.matrix3.z_axis.length(),
        )
    }

    /// Get the rotation as a [`Quat`].
    ///
    /// The transform is expected to be non-degenerate and without shearing,
    /// or the output will be invalid.
    ///
    /// Some of the computations overlap with `to_scale_rotation_translation`,
    /// which means you should use it instead if you also need scale.
    #[inline]
    pub fn rotation(&self) -> Quat {
        // Formula based on glam's implementation
        // https://github.com/bitshifter/glam-rs/blob/2e4443e70c709710dfb25958d866d29b11ed3e2b/src/f32/affine3a.rs#L290
        let det = self.0.matrix3.determinant();
        let scale = Vec3::new(
            self.0.matrix3.x_axis.length() * ops::copysign(1., det),
            self.0.matrix3.y_axis.length(),
            self.0.matrix3.z_axis.length(),
        );

        let inv_scale = scale.recip();

        Quat::from_mat3(&Mat3::from_cols(
            (self.0.matrix3.x_axis * inv_scale.x).into(),
            (self.0.matrix3.y_axis * inv_scale.y).into(),
            (self.0.matrix3.z_axis * inv_scale.z).into(),
        ))
    }

    /// Extracts `scale`, `rotation` and `translation` from `self`.
    ///
    /// The transform is expected to be non-degenerate and without shearing,
    /// or the output will be invalid.
    #[inline]
    pub fn to_scale_rotation_translation(&self) -> (Vec3, Quat, Vec3) {
        self.0.to_scale_rotation_translation()
    }

    /// Returns the 3d affine transformation matrix as a [`Mat4`].
    #[inline]
    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from(self.0)
    }

    /// Computes a Scale-Rotation-Translation decomposition of the transformation and
    /// returns the isometric part as an [Isometry3d].
    ///
    /// Any scaling done by the transformation will be ignored.
    ///
    /// Note: this is a somewhat costly and lossy conversion.
    ///
    /// The transform is expected to be non-degenerate and without shearing, or the output
    /// will be invalid.
    #[inline]
    pub fn to_isometry(&self) -> Isometry3d {
        let (_, rotation, translation) = self.0.to_scale_rotation_translation();
        Isometry3d::new(translation, rotation)
    }
    /// Transforms the given point from local space to global space,
    /// applying shear, scale, rotation and translation.
    ///
    /// It can be used like this:
    ///
    /// ```
    /// # use zlim_transform::{GlobalTransform};
    /// # use zlim_math::Vec3;
    /// let global_transform = GlobalTransform::from_xyz(1., 2., 3.);
    /// let local_point = Vec3::new(1., 2., 3.);
    /// let global_point = global_transform.transform_point(local_point);
    /// assert_eq!(global_point, Vec3::new(2., 4., 6.));
    /// ```
    ///
    /// To apply shear, scale, and rotation *without* applying translation,
    /// different functions are available:
    ///
    /// ```
    /// # use zlim_transform::{GlobalTransform};
    /// # use zlim_math::Vec3;
    /// let global_transform = GlobalTransform::from_xyz(1., 2., 3.);
    /// let local_direction = Vec3::new(1., 2., 3.);
    /// let global_direction = global_transform.transform_vector(local_direction);
    /// assert_eq!(global_direction, Vec3::new(1., 2., 3.));
    /// let roundtripped_local_direction = global_transform.affine().inverse().transform_vector3(global_direction);
    /// assert_eq!(roundtripped_local_direction, local_direction);
    /// ```
    #[inline]
    pub fn transform_point(&self, point: Vec3) -> Vec3 {
        self.0.transform_point3(point)
    }

    /// Transforms the given vector from local space to global space,
    /// applying shear, scale, rotation but not translation.
    ///
    /// It can be used like this:
    ///
    /// ```
    /// # use zlim_transform::{GlobalTransform};
    /// # use zlim_math::Vec3;
    /// let global_transform = GlobalTransform::from_xyz(1., 2., 3.);
    /// let local_vector = Vec3::new(1., 2., 3.);
    /// let global_vector = global_transform.transform_vector(local_vector);
    /// assert_eq!(global_vector, Vec3::new(1., 2., 3.));
    /// ```
    #[inline]
    pub fn transform_vector(&self, vector: Vec3) -> Vec3 {
        self.0.transform_vector3(vector)
    }

    /// Multiplies `self` with `transform` component by component, returning the
    /// resulting [`GlobalTransform`]
    #[inline]
    pub fn mul_transform(&self, transform: Transform) -> Self {
        Self(self.0 * transform.compute_affine())
    }

    /// Returns the transformation as a [`Transform`].
    ///
    /// The transform is expected to be non-degenerate and without shearing, or the output
    /// will be invalid.
    #[inline]
    pub fn compute_transform(&self) -> Transform {
        let (scale, rotation, translation) = self.0.to_scale_rotation_translation();
        Transform {
            translation,
            rotation,
            scale,
        }
    }

    /// Returns the [`Transform`] `self` would have if it was a child of an entity
    /// with the `parent` [`GlobalTransform`].
    ///
    /// This is useful if you want to "reparent" an [`EntityId`].
    ///
    /// Say you have an entity `e1` that you want to turn into a child of `e2`,
    /// but you want `e1` to keep the same global transform, even after re-parenting.
    ///
    /// The transform is expected to be non-degenerate and without shearing, or the output
    /// will be invalid.
    ///
    /// [`EntityId`]: zlim_core::entity::EntityId
    #[inline]
    pub fn reparented_to(&self, parent: &GlobalTransform) -> Transform {
        let relative_affine = parent.affine().inverse() * self.affine();
        let (scale, rotation, translation) = relative_affine.to_scale_rotation_translation();
        Transform {
            translation,
            rotation,
            scale,
        }
    }
}

impl From<Mat4> for GlobalTransform {
    fn from(world_from_local: Mat4) -> Self {
        Self(Affine3A::from_mat4(world_from_local))
    }
}

impl Mul<GlobalTransform> for GlobalTransform {
    type Output = GlobalTransform;

    #[inline]
    fn mul(self, global_transform: GlobalTransform) -> Self::Output {
        GlobalTransform(self.0 * global_transform.0)
    }
}

impl Mul<Transform> for GlobalTransform {
    type Output = GlobalTransform;

    #[inline]
    fn mul(self, transform: Transform) -> Self::Output {
        self.mul_transform(transform)
    }
}

impl Mul<Vec3> for GlobalTransform {
    type Output = Vec3;

    #[inline]
    fn mul(self, value: Vec3) -> Self::Output {
        self.transform_point(value)
    }
}

// -----------------------------------------------------------------------------
// Transform

impl Transform {
    /// Creates a new [`Transform`] at the position `(x, y, z)`.
    ///
    /// In 2d, the `z` component is used for z-ordering elements:
    /// higher `z`-value will be in front of lower `z`-value.
    #[inline]
    pub const fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self::from_translation(Vec3::new(x, y, z))
    }

    /// Extracts the translation, rotation, and scale from `matrix`.
    ///
    /// It must be a 3d affine transformation matrix.
    #[inline]
    pub fn from_matrix(world_from_local: Mat4) -> Self {
        let (scale, rotation, translation) = world_from_local.to_scale_rotation_translation();

        Transform {
            translation,
            rotation,
            scale,
        }
    }

    /// Creates a new [`Transform`], with `translation`.
    ///
    /// Rotation will be 0 and scale 1 on all axes.
    #[inline]
    pub const fn from_translation(translation: Vec3) -> Self {
        Transform {
            translation,
            ..Self::IDENTITY
        }
    }

    /// Creates a new [`Transform`], with `rotation`.
    ///
    /// Translation will be 0 and scale 1 on all axes.
    #[inline]
    pub const fn from_rotation(rotation: Quat) -> Self {
        Transform {
            rotation,
            ..Self::IDENTITY
        }
    }

    /// Creates a new [`Transform`], with `scale`.
    ///
    /// Translation will be 0 and rotation 0 on all axes.
    #[inline]
    pub const fn from_scale(scale: Vec3) -> Self {
        Transform {
            scale,
            ..Self::IDENTITY
        }
    }

    /// Creates a new [`Transform`] that is equivalent to the given [`Isometry3d`].
    #[inline]
    pub fn from_isometry(iso: Isometry3d) -> Self {
        Transform {
            translation: iso.translation.into(),
            rotation: iso.rotation,
            ..Self::IDENTITY
        }
    }

    /// Returns this [`Transform`] with a new translation.
    #[inline]
    #[must_use]
    pub const fn with_translation(mut self, translation: Vec3) -> Self {
        self.translation = translation;
        self
    }

    /// Returns this [`Transform`] with a new rotation.
    #[inline]
    #[must_use]
    pub const fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }

    /// Returns this [`Transform`] with a new scale.
    #[inline]
    #[must_use]
    pub const fn with_scale(mut self, scale: Vec3) -> Self {
        self.scale = scale;
        self
    }
}

/// Checks that a vector with the given squared length is normalized.
///
/// Warns for small error with a length threshold of approximately `1e-4`,
/// and panics for large error with a length threshold of approximately `1e-2`.
#[cfg(any(debug_assertions, feature = "debug"))]
fn assert_is_normalized(message: &str, length_squared: f32) {
    let length_error_squared = ops::abs(length_squared - 1.0);

    // Panic for large error and warn for slight error.
    if length_error_squared > 2e-2 || length_error_squared.is_nan() {
        ::core::hint::cold_path();
        // Length error is approximately 1e-2 or more.
        panic!("Error: {message}",);
    } else if length_error_squared > 2e-4 {
        ::core::hint::cold_path();
        // Length error is approximately 1e-4 or more.
        zlim_log::warn!("Warning: {message}",);
    }
}

impl Transform {
    /// Computes the 3d affine transformation matrix from this transform's translation,
    /// rotation, and scale.
    #[inline]
    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /// Get the [isometry] defined by this transform's rotation and translation, ignoring scale.
    ///
    /// [isometry]: Isometry3d
    #[inline]
    pub fn to_isometry(&self) -> Isometry3d {
        Isometry3d::new(self.translation, self.rotation)
    }

    /// Returns the 3d affine transformation matrix from this transforms translation,
    /// rotation, and scale.
    #[inline]
    pub fn compute_affine(&self) -> Affine3A {
        Affine3A::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /// Get the unit vector in the local `X` direction.
    #[inline]
    pub fn local_x(&self) -> Dir3 {
        // Quat * unit vector is length 1
        Dir3::new_unchecked(self.rotation * Vec3::X)
    }

    /// Get the unit vector in the local `Y` direction.
    #[inline]
    pub fn local_y(&self) -> Dir3 {
        // Quat * unit vector is length 1
        Dir3::new_unchecked(self.rotation * Vec3::Y)
    }

    /// Get the unit vector in the local `Z` direction.
    #[inline]
    pub fn local_z(&self) -> Dir3 {
        // Quat * unit vector is length 1
        Dir3::new_unchecked(self.rotation * Vec3::Z)
    }

    /// Equivalent to [`-local_x()`][Transform::local_x()]
    #[inline]
    pub fn left(&self) -> Dir3 {
        -self.local_x()
    }

    /// Equivalent to [`local_x()`][Transform::local_x()]
    #[inline]
    pub fn right(&self) -> Dir3 {
        self.local_x()
    }

    /// Equivalent to [`local_y()`][Transform::local_y]
    #[inline]
    pub fn up(&self) -> Dir3 {
        self.local_y()
    }

    /// Equivalent to [`-local_y()`][Transform::local_y]
    #[inline]
    pub fn down(&self) -> Dir3 {
        -self.local_y()
    }

    /// Equivalent to [`-local_z()`][Transform::local_z]
    #[inline]
    pub fn forward(&self) -> Dir3 {
        -self.local_z()
    }

    /// Equivalent to [`local_z()`][Transform::local_z]
    #[inline]
    pub fn back(&self) -> Dir3 {
        self.local_z()
    }

    /// Rotates this [`Transform`] by the given rotation.
    ///
    /// If this [`Transform`] has a parent, the `rotation` is relative to the rotation of the parent.
    #[inline]
    pub fn rotate(&mut self, rotation: Quat) {
        self.rotation = rotation * self.rotation;
    }

    /// Rotates this [`Transform`] around the given `axis` by `angle` (in radians).
    ///
    /// If this [`Transform`] has a parent, the `axis` is relative to the rotation of the parent.
    ///
    /// # Warning
    ///
    /// If you pass in an `axis` based on the current rotation (e.g. obtained via [`Transform::local_x`]),
    /// floating point errors can accumulate exponentially when applying rotations repeatedly this way.
    ///
    /// This will result in a denormalized rotation. In this case, it is recommended to normalize the
    /// [`Transform::rotation`] after each call to this method.
    ///
    /// # Panics
    ///
    /// In debug mode, passing a non-normalized `axis` will cause a panic due to the debug assertion.
    #[inline]
    pub fn rotate_axis(&mut self, axis: Dir3, angle: f32) {
        #[cfg(any(debug_assertions, feature = "debug"))]
        assert_is_normalized(
            "The axis given to `Transform::rotate_axis` is not normalized. This may be a result of obtaining \
            the axis from the transform. See the documentation of `Transform::rotate_axis` for more details.",
            axis.length_squared(),
        );
        self.rotate(Quat::from_axis_angle(axis.into(), angle));
    }

    /// Rotates this [`Transform`] around the `X` axis by `angle` (in radians).
    ///
    /// If this [`Transform`] has a parent, the axis is relative to the rotation of the parent.
    #[inline]
    pub fn rotate_x(&mut self, angle: f32) {
        self.rotate(Quat::from_rotation_x(angle));
    }

    /// Rotates this [`Transform`] around the `Y` axis by `angle` (in radians).
    ///
    /// If this [`Transform`] has a parent, the axis is relative to the rotation of the parent.
    #[inline]
    pub fn rotate_y(&mut self, angle: f32) {
        self.rotate(Quat::from_rotation_y(angle));
    }

    /// Rotates this [`Transform`] around the `Z` axis by `angle` (in radians).
    ///
    /// If this [`Transform`] has a parent, the axis is relative to the rotation of the parent.
    #[inline]
    pub fn rotate_z(&mut self, angle: f32) {
        self.rotate(Quat::from_rotation_z(angle));
    }

    /// Rotates this [`Transform`] by the given `rotation`.
    ///
    /// The `rotation` is relative to this [`Transform`]'s current rotation.
    #[inline]
    pub fn rotate_local(&mut self, rotation: Quat) {
        self.rotation *= rotation;
    }

    /// Rotates this [`Transform`] around its local `axis` by `angle` (in radians).
    ///
    /// # Warning
    ///
    /// If you pass in an `axis` based on the current rotation (e.g. obtained via [`Transform::local_x`]),
    /// floating point errors can accumulate exponentially when applying rotations repeatedly this way.
    /// This will result in a denormalized rotation. In this case, it is recommended to normalize the
    /// [`Transform::rotation`] after each call to this method.
    ///
    /// # Panics
    ///
    /// In debug mode, passing a non-normalized `axis` will cause a panic due to the debug assertion.
    #[inline]
    pub fn rotate_local_axis(&mut self, axis: Dir3, angle: f32) {
        #[cfg(any(debug_assertions, feature = "debug"))]
        assert_is_normalized(
            "The axis given to `Transform::rotate_axis_local` is not normalized. This may be a result of obtaining \
            the axis from the transform. See the documentation of `Transform::rotate_axis_local` for more details.",
            axis.length_squared(),
        );
        self.rotate_local(Quat::from_axis_angle(axis.into(), angle));
    }

    /// Rotates this [`Transform`] around its local `X` axis by `angle` (in radians).
    #[inline]
    pub fn rotate_local_x(&mut self, angle: f32) {
        self.rotate_local(Quat::from_rotation_x(angle));
    }

    /// Rotates this [`Transform`] around its local `Y` axis by `angle` (in radians).
    #[inline]
    pub fn rotate_local_y(&mut self, angle: f32) {
        self.rotate_local(Quat::from_rotation_y(angle));
    }

    /// Rotates this [`Transform`] around its local `Z` axis by `angle` (in radians).
    #[inline]
    pub fn rotate_local_z(&mut self, angle: f32) {
        self.rotate_local(Quat::from_rotation_z(angle));
    }

    /// Rotates this [`Transform`] around a `point` in space.
    ///
    /// If this [`Transform`] has a parent, the `point` is relative to the [`Transform`] of the parent.
    #[inline]
    pub fn rotate_around(&mut self, point: Vec3, rotation: Quat) {
        self.translate_around(point, rotation);
        self.rotate(rotation);
    }

    /// Translates this [`Transform`] around a `point` in space.
    ///
    /// If this [`Transform`] has a parent, the `point` is relative to the [`Transform`] of the parent.
    #[inline]
    pub fn translate_around(&mut self, point: Vec3, rotation: Quat) {
        self.translation = point + rotation * (self.translation - point);
    }

    /// Returns this [`Transform`] with a new rotation so that [`Transform::forward`]
    /// points towards the `target` position and [`Transform::up`] points towards `up`.
    ///
    /// In some cases it's not possible to construct a rotation. Another axis will be picked in those cases:
    /// - if `target` is the same as the transform translation, `Vec3::Z` is used instead
    /// - if `up` fails converting to `Dir3` (e.g if it is `Vec3::ZERO`), `Dir3::Y` is used instead
    /// - if the resulting forward direction is parallel with `up`, an orthogonal vector is used as the "right" direction
    #[inline]
    #[must_use]
    pub fn looking_at(mut self, target: Vec3, up: impl TryInto<Dir3>) -> Self {
        self.look_at(target, up);
        self
    }

    /// Returns this [`Transform`] with a new rotation so that [`Transform::forward`]
    /// points in the given `direction` and [`Transform::up`] points towards `up`.
    ///
    /// In some cases it's not possible to construct a rotation. Another axis will be picked in those cases:
    /// - if `direction` fails converting to `Dir3` (e.g if it is `Vec3::ZERO`), `Dir3::Z` is used instead
    /// - if `up` fails converting to `Dir3`, `Dir3::Y` is used instead
    /// - if `direction` is parallel with `up`, an orthogonal vector is used as the "right" direction
    #[inline]
    #[must_use]
    pub fn looking_to(mut self, direction: impl TryInto<Dir3>, up: impl TryInto<Dir3>) -> Self {
        self.look_to(direction, up);
        self
    }

    /// Rotates this [`Transform`] so that [`Transform::forward`] points towards the `target` position,
    /// and [`Transform::up`] points towards `up`.
    ///
    /// In some cases it's not possible to construct a rotation. Another axis will be picked in those cases:
    /// * if `target` is the same as the transform translation, `Vec3::Z` is used instead
    /// * if `up` fails converting to `Dir3` (e.g if it is `Vec3::ZERO`), `Dir3::Y` is used instead
    /// * if the resulting forward direction is parallel with `up`, an orthogonal vector is used as the "right" direction
    #[inline]
    pub fn look_at(&mut self, target: Vec3, up: impl TryInto<Dir3>) {
        self.look_to(target - self.translation, up);
    }

    /// Rotates this [`Transform`] so that [`Transform::forward`] points in the given `direction`
    /// and [`Transform::up`] points towards `up`.
    ///
    /// In some cases it's not possible to construct a rotation. Another axis will be picked in those cases:
    /// * if `direction` fails converting to `Dir3` (e.g if it is `Vec3::ZERO`), `Dir3::NEG_Z` is used instead
    /// * if `up` fails converting to `Dir3`, `Dir3::Y` is used instead
    /// * if `direction` is parallel with `up`, an orthogonal vector is used as the "right" direction
    #[inline]
    pub fn look_to(&mut self, direction: impl TryInto<Dir3>, up: impl TryInto<Dir3>) {
        let back = -direction.try_into().unwrap_or(Dir3::NEG_Z);
        let up = up.try_into().unwrap_or(Dir3::Y);
        let right = up
            .cross(back.into())
            .try_normalize()
            .unwrap_or_else(|| up.any_orthonormal_vector());
        let up = back.cross(right);
        self.rotation = Quat::from_mat3(&Mat3::from_cols(right, up, back.into()));
    }

    /// Rotates this [`Transform`] so that the `main_axis` vector, reinterpreted in local coordinates, points
    /// in the given `main_direction`, while `secondary_axis` points towards `secondary_direction`.
    /// For example, if a spaceship model has its nose pointing in the X-direction in its own local coordinates
    /// and its dorsal fin pointing in the Y-direction, then `align(Dir3::X, v, Dir3::Y, w)` will make the spaceship's
    /// nose point in the direction of `v`, while the dorsal fin does its best to point in the direction `w`.
    ///
    /// In some cases a rotation cannot be constructed. Another axis will be picked in those cases:
    /// - if `main_axis` or `main_direction` fail converting to `Dir3` (e.g are zero), `Dir3::X` takes their place
    /// - if `secondary_axis` or `secondary_direction` fail converting, `Dir3::Y` takes their place
    /// - if `main_axis` is parallel with `secondary_axis` or `main_direction` is parallel with `secondary_direction`,
    ///   a rotation is constructed which takes `main_axis` to `main_direction` along a great circle, ignoring the secondary
    ///   counterparts
    ///
    /// See [`Transform::align`] for additional details.
    #[inline]
    #[must_use]
    pub fn aligned_by(
        mut self,
        main_axis: impl TryInto<Dir3>,
        main_direction: impl TryInto<Dir3>,
        secondary_axis: impl TryInto<Dir3>,
        secondary_direction: impl TryInto<Dir3>,
    ) -> Self {
        self.align(
            main_axis,
            main_direction,
            secondary_axis,
            secondary_direction,
        );
        self
    }

    /// Rotates this [`Transform`] so that the `main_axis` vector, reinterpreted in local coordinates, points
    /// in the given `main_direction`, while `secondary_axis` points towards `secondary_direction`.
    ///
    /// For example, if a spaceship model has its nose pointing in the X-direction in its own local coordinates
    /// and its dorsal fin pointing in the Y-direction, then `align(Dir3::X, v, Dir3::Y, w)` will make the spaceship's
    /// nose point in the direction of `v`, while the dorsal fin does its best to point in the direction `w`.
    ///
    /// More precisely, the [`Transform::rotation`] produced will be such that:
    /// - applying it to `main_axis` results in `main_direction`
    /// - applying it to `secondary_axis` produces a vector that lies in the half-plane generated by `main_direction` and
    ///   `secondary_direction` (with positive contribution by `secondary_direction`)
    ///
    /// [`Transform::look_to`] is recovered, for instance, when `main_axis` is `Dir3::NEG_Z` (the [`Transform::forward`]
    /// direction in the default orientation) and `secondary_axis` is `Dir3::Y` (the [`Transform::up`] direction in the default
    /// orientation). (Failure cases may differ somewhat.)
    ///
    /// In some cases a rotation cannot be constructed. Another axis will be picked in those cases:
    /// - if `main_axis` or `main_direction` fail converting to `Dir3` (e.g are zero), `Dir3::X` takes their place
    /// - if `secondary_axis` or `secondary_direction` fail converting, `Dir3::Y` takes their place
    /// - if `main_axis` is parallel with `secondary_axis` or `main_direction` is parallel with `secondary_direction`,
    ///   a rotation is constructed which takes `main_axis` to `main_direction` along a great circle, ignoring the secondary
    ///   counterparts
    ///
    /// Example
    /// ```
    /// # use zlim_math::{Dir3, Vec3, Quat};
    /// # use zlim_transform::Transform;
    /// # let mut t1 = Transform::IDENTITY;
    /// # let mut t2 = Transform::IDENTITY;
    /// #
    /// t1.align(Dir3::X, Dir3::Y, Vec3::new(1., 1., 0.), Dir3::Z);
    /// let main_axis_image = t1.rotation * Dir3::X;
    /// let secondary_axis_image = t1.rotation * Vec3::new(1., 1., 0.);
    /// assert!(main_axis_image.abs_diff_eq(Vec3::Y, 1e-5));
    /// assert!(secondary_axis_image.abs_diff_eq(Vec3::new(0., 1., 1.), 1e-5));
    ///
    /// t1.align(Vec3::ZERO, Dir3::Z, Vec3::ZERO, Dir3::X);
    /// t2.align(Dir3::X, Dir3::Z, Dir3::Y, Dir3::X);
    /// assert_eq!(t1.rotation, t2.rotation);
    ///
    /// t1.align(Dir3::X, Dir3::Z, Dir3::X, Dir3::Y);
    /// assert_eq!(t1.rotation, Quat::from_rotation_arc(Vec3::X, Vec3::Z));
    /// ```
    #[inline]
    pub fn align(
        &mut self,
        main_axis: impl TryInto<Dir3>,
        main_direction: impl TryInto<Dir3>,
        secondary_axis: impl TryInto<Dir3>,
        secondary_direction: impl TryInto<Dir3>,
    ) {
        let main_axis = main_axis.try_into().unwrap_or(Dir3::X);
        let main_direction = main_direction.try_into().unwrap_or(Dir3::X);
        let secondary_axis = secondary_axis.try_into().unwrap_or(Dir3::Y);
        let secondary_direction = secondary_direction.try_into().unwrap_or(Dir3::Y);

        // The solution quaternion will be constructed in two steps.
        // First, we start with a rotation that takes `main_axis` to `main_direction`.
        let first_rotation = Quat::from_rotation_arc(main_axis.into(), main_direction.into());

        // Let's follow by rotating about the `main_direction` axis so that the image of `secondary_axis`
        // is taken to something that lies in the plane of `main_direction` and `secondary_direction`. Since
        // `main_direction` is fixed by this rotation, the first criterion is still satisfied.
        let secondary_image = first_rotation * secondary_axis;
        let secondary_image_ortho = secondary_image
            .reject_from_normalized(main_direction.into())
            .try_normalize();
        let secondary_direction_ortho = secondary_direction
            .reject_from_normalized(main_direction.into())
            .try_normalize();

        // If one of the two weak vectors was parallel to `main_direction`, then we just do the first part
        self.rotation = match (secondary_image_ortho, secondary_direction_ortho) {
            (Some(secondary_img_ortho), Some(secondary_dir_ortho)) => {
                let second_rotation =
                    Quat::from_rotation_arc(secondary_img_ortho, secondary_dir_ortho);
                second_rotation * first_rotation
            }
            _ => first_rotation,
        };
    }

    /// Multiplies `self` with `transform` component by component, returning the
    /// resulting [`Transform`]
    #[inline]
    #[must_use]
    pub fn mul_transform(&self, transform: Transform) -> Self {
        let translation = self.transform_point(transform.translation);
        let rotation = self.rotation * transform.rotation;
        let scale = self.scale * transform.scale;
        Transform {
            translation,
            rotation,
            scale,
        }
    }

    /// Transforms the given `point`, applying scale, rotation and translation.
    ///
    /// If this [`Transform`] has an ancestor entity with a [`Transform`] component,
    /// [`Transform::transform_point`] will transform a point in local space into its
    /// parent transform's space.
    ///
    /// If this [`Transform`] does not have a parent, [`Transform::transform_point`] will
    /// transform a point in local space into worldspace coordinates.
    ///
    /// If you always want to transform a point in local space to worldspace, or if you need
    /// the inverse transformations, see [`GlobalTransform::transform_point()`].
    #[inline]
    pub fn transform_point(&self, mut point: Vec3) -> Vec3 {
        point = self.scale * point;
        point = self.rotation * point;
        point += self.translation;
        point
    }

    /// Transforms the given `vector`, applying scale and rotation only, not translation.
    ///
    /// If this [`Transform`] has an ancestor entity with a [`Transform`] component,
    /// [`Transform::transform_vector`] will transform a vector in local space into its
    /// parent transform's space.
    ///
    /// If this [`Transform`] does not have a parent, [`Transform::transform_vector`] will
    /// transform a vector in local space into worldspace coordinates.
    ///
    /// If you always want to transform a vector in local space to worldspace,
    /// see [`GlobalTransform::transform_vector()`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use zlim_transform::Transform;
    /// # use zlim_math::{Vec3, Quat};
    /// # use std::f64::consts::PI;
    /// #
    /// let transform = Transform::from_xyz(1., 2., 3.)
    ///    .with_scale(Vec3::splat(2.))
    ///    .with_rotation(Quat::from_axis_angle(Vec3::Y, PI as f32));
    /// let local_vector = Vec3::new(1., 2., 3.);
    /// let global_vector = transform.transform_vector(local_vector);
    /// assert!(global_vector.abs_diff_eq(Vec3::new(-2., 4., -6.), 1e-5));
    /// ```
    #[inline]
    pub fn transform_vector(&self, mut vector: Vec3) -> Vec3 {
        vector = self.scale * vector;
        vector = self.rotation * vector;
        vector
    }

    /// Returns `true` if, and only if, translation, rotation and scale all are
    /// finite. If any of them contains a `NaN`, positive or negative infinity,
    /// this will return `false`.
    #[inline]
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.translation.is_finite() && self.rotation.is_finite() && self.scale.is_finite()
    }
}

impl From<GlobalTransform> for Transform {
    fn from(transform: GlobalTransform) -> Self {
        transform.compute_transform()
    }
}

impl From<Transform> for GlobalTransform {
    fn from(transform: Transform) -> Self {
        Self(transform.compute_affine())
    }
}

impl Mul<Transform> for Transform {
    type Output = Transform;

    fn mul(self, transform: Transform) -> Self::Output {
        self.mul_transform(transform)
    }
}

impl Mul<GlobalTransform> for Transform {
    type Output = GlobalTransform;

    #[inline]
    fn mul(self, global_transform: GlobalTransform) -> Self::Output {
        GlobalTransform::from(self) * global_transform
    }
}

impl Mul<Vec3> for Transform {
    type Output = Vec3;

    fn mul(self, value: Vec3) -> Self::Output {
        self.transform_point(value)
    }
}

// -----------------------------------------------------------------------------
// assert_is_normalized

#[cfg(test)]
mod test {
    use super::*;
    use zlim_math::EulerRot::XYZ;

    fn transform_equal(left: GlobalTransform, right: Transform) -> bool {
        left.0.abs_diff_eq(right.compute_affine(), 0.01)
    }

    #[test]
    fn reparented_to_transform_identity() {
        fn reparent_to_same(t1: GlobalTransform, t2: GlobalTransform) -> Transform {
            t2.mul_transform(t1.into()).reparented_to(&t2)
        }
        let t1 = GlobalTransform::from(Transform {
            translation: Vec3::new(1034.0, 34.0, -1324.34),
            rotation: Quat::from_euler(XYZ, 1.0, 0.9, 2.1),
            scale: Vec3::new(1.0, 1.0, 1.0),
        });
        let t2 = GlobalTransform::from(Transform {
            translation: Vec3::new(0.0, -54.493, 324.34),
            rotation: Quat::from_euler(XYZ, 1.9, 0.3, 3.0),
            scale: Vec3::new(1.345, 1.345, 1.345),
        });
        let retransformed = reparent_to_same(t1, t2);
        assert!(
            transform_equal(t1, retransformed),
            "t1:{:#?} retransformed:{:#?}",
            t1.compute_transform(),
            retransformed,
        );
    }
    #[test]
    fn reparented_usecase() {
        let t1 = GlobalTransform::from(Transform {
            translation: Vec3::new(1034.0, 34.0, -1324.34),
            rotation: Quat::from_euler(XYZ, 0.8, 1.9, 2.1),
            scale: Vec3::new(10.9, 10.9, 10.9),
        });
        let t2 = GlobalTransform::from(Transform {
            translation: Vec3::new(28.0, -54.493, 324.34),
            rotation: Quat::from_euler(XYZ, 0.0, 3.1, 0.1),
            scale: Vec3::new(0.9, 0.9, 0.9),
        });
        // goal: find `X` such as `t2 * X = t1`
        let reparented = t1.reparented_to(&t2);
        let t1_prime = t2 * reparented;
        assert!(
            transform_equal(t1, t1_prime.into()),
            "t1:{:#?} t1_prime:{:#?}",
            t1.compute_transform(),
            t1_prime.compute_transform(),
        );
    }

    #[test]
    fn scale() {
        // Note: a scale of zero produces a singular matrix, which glam's
        // `to_scale_rotation_translation` refuses to decompose (det == 0).
        let test_values = [-42.42, 42.42];
        for x in test_values {
            for y in test_values {
                for z in test_values {
                    let scale = Vec3::new(x, y, z);
                    let gt = GlobalTransform::from_scale(scale);
                    assert_eq!(gt.scale(), gt.to_scale_rotation_translation().0);
                }
            }
        }
    }
}
