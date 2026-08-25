//! Color parsing utilities.
//!
//! Ported from `msHexToInt()` in `src/mapstring.cpp` and
//! `msSLDSetColorObject()` in `src/mapogcsld.cpp`. The behavior is kept
//! identical to the original C implementation (including its lack of
//! validation of individual hex digits) so results stay verifiable against
//! the C code while callers migrate to the new API.

/// A simple RGB color, mirroring the relevant fields of MapServer's
/// `colorObj` (see `src/mapsymbol.h`).
///
/// Channel values are `i32` (rather than `u8`) to intentionally match the
/// original C struct, which uses `int` fields even though valid colors are
/// documented as being in the `[0-255]` range: MapServer uses out-of-range
/// values (e.g. negative numbers) as sentinels to mean "unset" elsewhere in
/// the codebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub red: i32,
    pub green: i32,
    pub blue: i32,
}

/// Converts a 2 character hexadecimal string to an integer.
///
/// Direct port of `msHexToInt()` from `src/mapstring.cpp`.
pub fn hex_to_int(hex: &[u8; 2]) -> i32 {
    fn digit(b: u8) -> i32 {
        if b >= b'A' {
            (((b & 0xdf) - b'A') as i32) + 10
        } else {
            (b.wrapping_sub(b'0')) as i32
        }
    }

    digit(hex[0]) * 16 + digit(hex[1])
}

/// Sets `color` from a `"#RRGGBB"` hex string, mirroring
/// `msSLDSetColorObject()` from `src/mapogcsld.cpp`.
///
/// Returns `true` if the color was updated. As with the original C
/// function, the input must be exactly 7 characters long and start with
/// `#`; otherwise `color` is left unchanged (the C function always returns
/// `MS_SUCCESS` regardless).
pub fn set_color_from_hex(hex: &str, color: &mut Color) -> bool {
    let bytes = hex.as_bytes();
    if bytes.len() != 7 || bytes[0] != b'#' {
        return false;
    }

    color.red = hex_to_int(&[bytes[1], bytes[2]]);
    color.green = hex_to_int(&[bytes[3], bytes[4]]);
    color.blue = hex_to_int(&[bytes[5], bytes[6]]);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_int_handles_digits_and_uppercase() {
        assert_eq!(hex_to_int(b"00"), 0);
        assert_eq!(hex_to_int(b"09"), 9);
        assert_eq!(hex_to_int(b"0A"), 10);
        assert_eq!(hex_to_int(b"FF"), 255);
        assert_eq!(hex_to_int(b"7F"), 127);
    }

    #[test]
    fn hex_to_int_handles_lowercase() {
        assert_eq!(hex_to_int(b"ff"), 255);
        assert_eq!(hex_to_int(b"0a"), 10);
    }

    #[test]
    fn set_color_from_hex_parses_valid_input() {
        let mut color = Color::default();
        assert!(set_color_from_hex("#FF0000", &mut color));
        assert_eq!(
            color,
            Color {
                red: 255,
                green: 0,
                blue: 0
            }
        );

        assert!(set_color_from_hex("#00ff00", &mut color));
        assert_eq!(
            color,
            Color {
                red: 0,
                green: 255,
                blue: 0
            }
        );

        assert!(set_color_from_hex("#0000FF", &mut color));
        assert_eq!(
            color,
            Color {
                red: 0,
                green: 0,
                blue: 255
            }
        );
    }

    #[test]
    fn set_color_from_hex_rejects_invalid_input() {
        let mut color = Color {
            red: 1,
            green: 2,
            blue: 3,
        };

        // Wrong length.
        assert!(!set_color_from_hex("#FFF", &mut color));
        // Missing leading '#'.
        assert!(!set_color_from_hex("FF0000!", &mut color));
        // Unchanged on rejection.
        assert_eq!(
            color,
            Color {
                red: 1,
                green: 2,
                blue: 3
            }
        );
    }
}
