use core::time::Duration;

use crate::ops::Opaque;

zlim_reflect_derive::impl_reflect! {
    #[type_path = "core::time::Duration"]
    #[reflect(Opaque, Serialize, Deserialize, Default, Debug, Hash, Eq)]
    pub struct Duration;
}

impl Opaque for Duration {
    fn apply_str(&mut self, v: &str) -> Result<(), String> {
        let v = v.trim();
        if v.ends_with("ns") {
            let tail = v.len() - "ns".len();
            if let Ok(x) = v[..tail].parse::<f64>() {
                *self = Duration::from_nanos_u128(x as u128);
                return Ok(());
            }
        } else if v.ends_with("µs") {
            let tail = v.len() - "µs".len();
            if let Ok(x) = v[..tail].parse::<f64>() {
                *self = Duration::from_nanos_u128((x * 1000.0) as u128);
                return Ok(());
            }
        } else if v.ends_with("us") {
            let tail = v.len() - "us".len();
            if let Ok(x) = v[..tail].parse::<f64>() {
                *self = Duration::from_nanos_u128((x * 1000.0) as u128);
                return Ok(());
            }
        } else if v.ends_with("ms") {
            let tail = v.len() - "ms".len();
            if let Ok(x) = v[..tail].parse::<f64>() {
                *self = Duration::from_micros((x * 1000.0) as u64);
                return Ok(());
            }
        } else if v.ends_with("s") {
            let tail = v.len() - "s".len();
            if let Ok(x) = v[..tail].parse::<f64>() {
                *self = Duration::from_secs_f64(x);
                return Ok(());
            }
        } else if v.ends_with("m") {
            let tail = v.len() - "m".len();
            if let Ok(x) = v[..tail].parse::<f64>() {
                *self = Duration::from_secs_f64(x * 60.0);
                return Ok(());
            }
        } else if v.ends_with("h") {
            let tail = v.len() - "h".len();
            if let Ok(x) = v[..tail].parse::<f64>() {
                *self = Duration::from_secs_f64(x * 3600.0);
                return Ok(());
            }
        } else if v.ends_with("d") {
            let tail = v.len() - "d".len();
            if let Ok(x) = v[..tail].parse::<f64>() {
                *self = Duration::from_secs_f64(x * 86400.0);
                return Ok(());
            }
        }

        Err(format!("unsupport duration format: `{v}`"))
    }

    fn stringify(&self) -> String {
        format!("{self:?}")
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::db::TypeDB;
    use crate::ops::Opaque;
    use core::any::TypeId;
    use core::time::Duration;

    #[test]
    fn is_registered() {
        TypeDB::collect();
        assert!(TypeDB::get_by_type(TypeId::of::<Duration>()).is_some());
    }

    #[test]
    fn test_duration_roundtrip() {
        let test_cases = vec![
            Duration::from_secs(0),
            Duration::from_nanos(1),
            Duration::from_nanos(999),
            Duration::from_nanos(1_000),
            Duration::from_micros(1),
            Duration::from_micros(999),
            Duration::from_micros(1_000),
            Duration::from_millis(1),
            Duration::from_millis(999),
            Duration::from_millis(1_000),
            Duration::from_secs(1),
            Duration::from_secs(59),
            Duration::from_secs(60),
            Duration::from_secs(120),
            Duration::from_secs(3600),
            Duration::from_secs(86400),
            Duration::from_secs_f64(0.001),
            Duration::from_secs_f64(0.999),
            Duration::from_secs_f64(1.5),
            Duration::from_secs_f64(42.5),
            Duration::from_secs_f64(60.5),
            Duration::from_secs_f64(90.0),
            Duration::from_secs_f64(119.9),
            Duration::from_secs_f64(120.0),
            Duration::from_secs_f64(3600.5),
            Duration::from_secs_f64(3660.0),
            Duration::from_secs_f64(86400.5),
            Duration::from_secs(604800),
            Duration::from_secs(2_592_000),
            Duration::from_secs(31_536_000),
            Duration::new(1, 200_050_000), // 1.5s
            Duration::new(60, 0),
            Duration::new(3600, 0),
            Duration::new(86400, 0),
            Duration::new(0, 1_500_000),
            Duration::new(0, 42_000_000),
            Duration::new(1, 42_000_000),
            Duration::new(60, 42_000_000),
            Duration::new(3600, 42_000_000),
            Duration::new(86400, 42_000_000),
        ];

        for (i, original) in test_cases.iter().enumerate() {
            let s = Opaque::stringify(original);

            let mut parsed = Duration::default();
            let result = Opaque::apply_str(&mut parsed, &s);

            assert!(result.is_ok(), "Case {}: failed to parse '{}'", i, s);

            let o: u128 = original.as_nanos();
            let f: u128 = parsed.as_nanos();
            let d: u128 = o.max(f) - o.min(f);
            // Accuracy difference less than `1/512`.
            assert!(
                d <= (o >> 9),
                "Case {i}: mismatch: {o}ns vs {f}ns (diff {d}ns)"
            );
        }
    }
}
