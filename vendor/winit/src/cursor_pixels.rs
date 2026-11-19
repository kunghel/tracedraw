//! TraceDraw: the pixels and the scale of a custom pointer image on Linux,
//! so that a drawn pointer looks the same on every start (see
//! `vendor/README.md`, #2871). The file has no dependency on the rest of
//! the crate, so that TraceDraw's tests can include and run it.

/// One pixel of a pointer image (straight red, green, blue and alpha, as
/// winit takes it) as Xcursor wants it: ARGB with the colour already
/// multiplied by its alpha. Given straight, a half transparent white
/// edge pixel (the halo round a drawn pointer) is an impossible colour
/// brighter than its alpha, which the X server shows as a glowing fringe
/// or, where it draws the pointer with the two colours of a core cursor,
/// a pointer of other colours.
pub(crate) fn xcursor_pixel(rgba: [u8; 4]) -> u32 {
    let a = u32::from(rgba[3]);
    // Rounded: 255 alpha keeps the colour exactly, 0 alpha makes it 0.
    let mul = |c: u8| (u32::from(c) * a + 127) / 255;
    (a << 24) | (mul(rgba[0]) << 16) | (mul(rgba[1]) << 8) | mul(rgba[2])
}

/// The buffer scale for a custom pointer image on Wayland, whose pixels
/// are drawn for the window's scale `window_scale` (the program draws
/// the pointer at the window's pixels per point). The pointer's own
/// surface reports scale 1 until the compositor tells it which output it
/// is on, which may come after the first pointer is set: the first
/// pointer of a start then showed at twice its size on a 200 % screen,
/// and another start, or the next pointer, at its size. The window's
/// scale is known from its first configure. Whole numbers only (a buffer
/// scale is an integer); at least 1.
pub(crate) fn wayland_cursor_buffer_scale(window_scale: f64) -> i32 {
    if !window_scale.is_finite() || window_scale < 1.0 {
        return 1;
    }
    (window_scale.round() as i32).clamp(1, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xcursor_pixels_are_premultiplied() {
        // Opaque colours stay as they are, in ARGB order.
        assert_eq!(xcursor_pixel([255, 255, 255, 255]), 0xFFFF_FFFF);
        assert_eq!(xcursor_pixel([0, 0, 0, 255]), 0xFF00_0000);
        assert_eq!(xcursor_pixel([0x12, 0x34, 0x56, 255]), 0xFF12_3456);
        // A half transparent white halo pixel: no channel above alpha.
        let p = xcursor_pixel([255, 255, 255, 128]);
        assert_eq!(p, 0x8080_8080);
        // Fully transparent: all zero, whatever colour it had.
        assert_eq!(xcursor_pixel([255, 200, 100, 0]), 0);
        // Never a colour channel above the alpha.
        for a in [1u8, 40, 128, 200, 254] {
            for c in [0u8, 1, 127, 128, 255] {
                let p = xcursor_pixel([c, c, c, a]);
                let alpha = p >> 24;
                for shift in [0, 8, 16] {
                    assert!((p >> shift) & 0xFF <= alpha, "{c} {a} -> {p:08x}");
                }
            }
        }
    }

    #[test]
    fn the_wayland_pointer_takes_the_windows_scale() {
        assert_eq!(wayland_cursor_buffer_scale(1.0), 1);
        assert_eq!(wayland_cursor_buffer_scale(2.0), 2);
        assert_eq!(wayland_cursor_buffer_scale(3.0), 3);
        // Fractional scales (without a viewport) round to a whole one.
        assert_eq!(wayland_cursor_buffer_scale(1.25), 1);
        assert_eq!(wayland_cursor_buffer_scale(1.75), 2);
        // Nonsense falls back to 1.
        assert_eq!(wayland_cursor_buffer_scale(0.0), 1);
        assert_eq!(wayland_cursor_buffer_scale(f64::NAN), 1);
        assert_eq!(wayland_cursor_buffer_scale(-2.0), 1);
    }
}
