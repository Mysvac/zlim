use rand::RngExt;
use rand::distr::{Distribution, StandardUniform};
use zlim_math::{Dir2, Dir3, Dir3A, Quat, Rot2};

pub trait FromRng
where
    Self: Sized,
    StandardUniform: Distribution<Self>,
{
    /// Construct a value of this type uniformly at random
    /// using `rng` as the source of randomness.
    ///
    /// # Example
    ///
    /// ```
    /// use rand::{SeedableRng, rngs::StdRng};
    /// use zlim_math::Dir3;
    /// use zlim_sample::FromRng;
    ///
    /// let mut rng = StdRng::seed_from_u64(0);
    /// let dir: Dir3 = FromRng::from_rng(&mut rng);
    /// ```
    #[inline]
    fn from_rng<R: RngExt + ?Sized>(rng: &mut R) -> Self {
        RngExt::random(rng)
    }
}

impl FromRng for Dir2 {}
impl FromRng for Dir3 {}
impl FromRng for Dir3A {}
impl FromRng for Rot2 {}
impl FromRng for Quat {}
