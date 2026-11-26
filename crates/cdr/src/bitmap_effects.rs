//! Saved bitmap effects in object style strings. See docs/cdr-format.md.

use serde_json::Value;
use tracedraw_core::{document::RasterFx, BitmapEffect, Rect, Shape, ShapeKind};

use crate::ParseReport;

const STACK: &str = "StackedBitmapEffects";
const GAUSSIAN: &str = "GaussianBlurEffect";

/// Only the version and vector case observed in the original sample are
/// enabled. The app makes the picture from this editable list; headless
/// callers can use the renderer's bitmap-effects preparation.
pub(crate) fn apply(style: &str, version: u16, shape: &mut Shape, report: &mut ParseReport) {
    if !style.contains(STACK) {
        return;
    }
    if style.len() > 1 << 20 {
        report.warn("bitmap effect style exceeds 1 MiB; effects not read");
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(style) else {
        report.warn("bitmap effect style is not valid JSON; effects not read");
        return;
    };
    let Some(stack) = value.get(STACK).and_then(Value::as_object) else {
        report.warn("bitmap effect stack is not an object; effects not read");
        return;
    };
    if version != 27 || matches!(shape.kind, ShapeKind::Bitmap { .. }) {
        report.warn(
            "bitmap effect layout is unverified for this version or bitmap; effects not read",
        );
        return;
    }
    if stack.len() > 64 {
        report.warn("bitmap effect stack exceeds 64 entries; effects not read");
        return;
    }
    let mut effects = Vec::new();
    for (name, settings) in stack {
        if name.ends_with("_Deleted") || name.ends_with("_Visible") || name.ends_with("_Z_Index") {
            continue;
        }
        if name != GAUSSIAN {
            report.warn(format!("unsupported bitmap effect {name}; effect not read"));
            continue;
        }
        let flag = |suffix: &str| {
            stack
                .get(&format!("{name}_{suffix}"))
                .and_then(Value::as_str)
        };
        match flag("Deleted") {
            Some("1") => continue,
            Some("0") => {}
            _ => {
                report.warn("Gaussian blur has invalid deletion state; effect not read");
                continue;
            }
        }
        let parsed = (|| {
            // An effect that was never hidden has no visibility key
            // (#1238: 464 of the 1 000 blurred objects of one drawing,
            // blurred in its reference picture).
            let visible = match flag("Visible") {
                Some("0") => false,
                Some("1") | None => true,
                _ => return None,
            };
            // One filter layout has been observed. Do not guess at the
            // meaning of other resampling modes or missing ordering data.
            let _index = flag("Z_Index")?.parse::<u32>().ok()?;
            let settings = settings.as_object()?;
            if settings.len() != 2 || settings.get("Resampled")?.as_str()? != "0" {
                return None;
            }
            let radius = settings.get("Radius")?.as_str()?.parse::<f64>().ok()? / 100.0;
            if !radius.is_finite() || !(0.0..=250.0).contains(&radius) {
                return None;
            }
            Some(BitmapEffect {
                id: "gaussian_blur".into(),
                params: [("radius".into(), radius), ("kernel_extent".into(), 2.0)].into(),
                visible,
            })
        })();
        match parsed {
            Some(effect) => effects.push(effect),
            None => report.warn("unsupported or malformed Gaussian blur settings; effect not read"),
        }
    }
    if !effects.is_empty() {
        shape.raster_fx = Some(Box::new(RasterFx {
            effects,
            rect: Rect::ZERO,
            width_px: 0,
            height_px: 0,
            png: Default::default(),
            source: 0,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::ShapeId;

    fn read(stack: Value, version: u16) -> (Shape, ParseReport) {
        let mut shape = Shape::new(
            ShapeId(1),
            ShapeKind::rect_with_corners(Rect::new(0., 0., 20., 10.), Default::default()),
        );
        let mut report = ParseReport::default();
        apply(
            &serde_json::json!({STACK: stack}).to_string(),
            version,
            &mut shape,
            &mut report,
        );
        (shape, report)
    }

    fn stack() -> Value {
        serde_json::json!({
            GAUSSIAN: {"Radius":"1500", "Resampled":"0"},
            "GaussianBlurEffect_Visible":"1",
            "GaussianBlurEffect_Deleted":"0",
            "GaussianBlurEffect_Z_Index":"0"
        })
    }

    #[test]
    fn saved_gaussian_blur_keeps_radius_visibility_and_native_round_trip() {
        for visible in ["0", "1"] {
            let mut s = stack();
            s["GaussianBlurEffect_Visible"] = visible.into();
            let (shape, report) = read(s, 27);
            assert!(report.warnings.is_empty(), "{:?}", report.warnings);
            let back: Shape =
                serde_json::from_str(&serde_json::to_string(&shape).unwrap()).unwrap();
            let fx = back.raster_fx.unwrap();
            assert_eq!(fx.effects[0].params["radius"], 15.0);
            assert_eq!(fx.effects[0].visible, visible == "1");
            assert!(fx.png.is_empty());
        }
        for radius in ["0", "25000"] {
            let mut s = stack();
            s[GAUSSIAN]["Radius"] = radius.into();
            let (shape, report) = read(s, 27);
            assert!(shape.raster_fx.is_some());
            assert!(report.warnings.is_empty());
        }
    }

    #[test]
    fn effect_without_a_visibility_key_is_visible() {
        // #1238: an effect that was never hidden has no visibility key.
        let mut s = stack();
        s.as_object_mut().unwrap().remove("GaussianBlurEffect_Visible");
        let (shape, report) = read(s, 27);
        let fx = shape.raster_fx.expect("blur read");
        assert!(fx.effects[0].visible);
        assert!(report.warnings.is_empty());
        let mut s = stack();
        s["GaussianBlurEffect_Visible"] = "0".into();
        assert!(!read(s, 27).0.raster_fx.expect("blur read").effects[0].visible);
    }

    #[test]
    fn deleted_unknown_and_unverified_effects_are_not_invented() {
        let mut s = stack();
        s["GaussianBlurEffect_Deleted"] = "1".into();
        assert!(read(s, 27).0.raster_fx.is_none());
        let (shape, report) = read(stack(), 26);
        assert!(shape.raster_fx.is_none());
        assert!(!report.warnings.is_empty());
        let (shape, report) = read(serde_json::json!({"UnknownEffect":{}}), 27);
        assert!(shape.raster_fx.is_none());
        assert!(!report.warnings.is_empty());
    }

    #[test]
    fn malformed_gaussian_settings_warn_without_panicking() {
        for radius in ["NaN", "inf", "-1", "25001", "", "no"] {
            let mut s = stack();
            s[GAUSSIAN]["Radius"] = radius.into();
            let (shape, report) = read(s, 27);
            assert!(shape.raster_fx.is_none());
            assert!(!report.warnings.is_empty(), "{radius}");
        }
        for field in ["Radius", "Resampled"] {
            let mut s = stack();
            s[GAUSSIAN].as_object_mut().unwrap().remove(field);
            assert!(read(s, 27).0.raster_fx.is_none());
        }
        for (key, value) in [
            ("GaussianBlurEffect_Visible", "2"),
            ("GaussianBlurEffect_Deleted", "-1"),
            ("GaussianBlurEffect_Z_Index", "NaN"),
        ] {
            let mut s = stack();
            s[key] = value.into();
            let (shape, report) = read(s, 27);
            assert!(shape.raster_fx.is_none());
            assert!(!report.warnings.is_empty());
        }
        let (shape, report) = read(Value::Null, 27);
        assert!(shape.raster_fx.is_none());
        assert!(!report.warnings.is_empty());
    }

    #[test]
    fn truncated_oversized_and_bitmap_records_are_safe() {
        let text = serde_json::json!({STACK: stack()}).to_string();
        for cut in 0..text.len() {
            let mut shape = read(Value::Null, 27).0;
            let mut report = ParseReport::default();
            apply(&text[..cut], 27, &mut shape, &mut report);
            assert!(shape.raster_fx.is_none(), "truncated at {cut}");
        }
        let mut shape = read(Value::Null, 27).0;
        let mut report = ParseReport::default();
        apply(
            &format!("{STACK}{}", " ".repeat(1 << 20)),
            27,
            &mut shape,
            &mut report,
        );
        assert!(shape.raster_fx.is_none());
        assert!(!report.warnings.is_empty());
        shape.kind = ShapeKind::Bitmap {
            rect: Rect::new(0., 0., 10., 10.),
            width_px: 1,
            height_px: 1,
            png: Default::default(),
            fx: None,
        };
        apply(&text, 27, &mut shape, &mut report);
        assert!(
            shape.raster_fx.is_none(),
            "do not filter unknown cached pixels twice"
        );
    }
}
