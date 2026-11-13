//! TraceDraw: a window's minimum and maximum size kept in logical units,
//! so that they follow the monitor's scaling on Windows (see
//! `vendor/README.md`, #2487). The file has no dependency on the rest of
//! the crate, so that TraceDraw's tests can include and run it.

/// A bound given in physical pixels (`width` by `height`) at the scaling
/// `scale`, in logical units: the same bound on a monitor of any
/// scaling. `None` when a number is not finite or the scaling is not
/// above zero: the bound is then kept as it was given.
pub(crate) fn logical_bound(width: f64, height: f64, scale: f64) -> Option<(f64, f64)> {
    if !(width.is_finite() && height.is_finite() && scale.is_finite()) || scale <= 0.0 {
        return None;
    }
    Some((width / scale, height / scale))
}

#[cfg(test)]
mod tests {
    use super::logical_bound;

    #[test]
    fn a_bound_in_pixels_becomes_one_in_logical_units() {
        // 800 x 500 points asked on a monitor at 150 %: 1200 x 750 pixels.
        assert_eq!(logical_bound(1200.0, 750.0, 1.5), Some((800.0, 500.0)));
        // On a monitor at 100 % the bound is then 800 x 500 pixels, not
        // the 1200 x 750 that held the window larger than it was asked
        // to be, and at 200 % 1600 x 1000.
        let (w, h) = logical_bound(1200.0, 750.0, 1.5).expect("a bound");
        assert_eq!((w * 1.0, h * 1.0), (800.0, 500.0));
        assert_eq!((w * 2.0, h * 2.0), (1600.0, 1000.0));
        assert_eq!(logical_bound(800.0, 500.0, 1.0), Some((800.0, 500.0)));
    }

    #[test]
    fn a_scaling_that_makes_no_sense_leaves_the_bound_as_given() {
        assert_eq!(logical_bound(800.0, 500.0, 0.0), None);
        assert_eq!(logical_bound(800.0, 500.0, -1.0), None);
        assert_eq!(logical_bound(800.0, 500.0, f64::NAN), None);
        assert_eq!(logical_bound(f64::INFINITY, 500.0, 1.0), None);
    }
}
