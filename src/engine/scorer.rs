/// Compute asymmetric mean squared error between canvas and target grayscale buffers.
/// `alpha` controls the penalty for overshoot (canvas darker than target, i.e. ink on whitespace).
/// With alpha=1.0 this is standard MSE. With alpha>1.0, drawing on bright areas is penalized more.
/// Returns a value in [0.0, ..] where 0.0 = identical.
pub fn asymmetric_mse(canvas: &[u8], target: &[u8], alpha: f64) -> f64 {
    debug_assert_eq!(canvas.len(), target.len(), "buffers must have same length");

    let sum: f64 = canvas
        .iter()
        .zip(target.iter())
        .map(|(&cv, &tv)| {
            let diff = cv as f64 - tv as f64;
            let sq = diff * diff;
            // diff < 0 means canvas is darker than target (overshoot)
            if diff < 0.0 { sq * alpha } else { sq }
        })
        .sum();

    sum / (canvas.len() as f64 * 255.0 * 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_buffers_zero() {
        let a = vec![128u8; 100];
        assert!((asymmetric_mse(&a, &a, 2.0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn alpha_one_is_standard_mse() {
        let a = vec![0u8; 100];
        let b = vec![255u8; 100];
        assert!((asymmetric_mse(&a, &b, 1.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn overshoot_penalized_more() {
        // canvas=0 (dark), target=255 (bright) → overshoot, diff<0
        let canvas_dark = vec![0u8; 100];
        let target_bright = vec![255u8; 100];
        let score_overshoot = asymmetric_mse(&canvas_dark, &target_bright, 2.0);

        // canvas=255 (bright), target=0 (dark) → undershoot, diff>0
        let canvas_bright = vec![255u8; 100];
        let target_dark = vec![0u8; 100];
        let score_undershoot = asymmetric_mse(&canvas_bright, &target_dark, 2.0);

        // Overshoot should be penalized 2x
        assert!((score_overshoot / score_undershoot - 2.0).abs() < 0.01);
    }
}
