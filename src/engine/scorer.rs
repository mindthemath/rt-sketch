/// Compute mean squared error between two grayscale buffers.
/// Both buffers must have the same length.
/// Returns a value in [0.0, 1.0] where 0.0 = identical, 1.0 = maximally different.
pub fn mse(a: &[u8], b: &[u8]) -> f64 {
    debug_assert_eq!(a.len(), b.len(), "buffers must have same length");

    let sum: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(&av, &bv)| {
            let diff = av as i32 - bv as i32;
            (diff * diff) as u64
        })
        .sum();

    sum as f64 / (a.len() as f64 * 255.0 * 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_buffers_zero_mse() {
        let a = vec![128u8; 100];
        assert!((mse(&a, &a) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn opposite_buffers_max_mse() {
        let a = vec![0u8; 100];
        let b = vec![255u8; 100];
        assert!((mse(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn half_difference() {
        let a = vec![0u8; 100];
        let b = vec![128u8; 100];
        let score = mse(&a, &b);
        // 128^2 / 255^2 ≈ 0.252
        assert!(score > 0.25 && score < 0.26);
    }
}
