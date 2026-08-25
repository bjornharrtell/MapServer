//! FFI bindings to the verbatim C reference implementations in `c_ref/`,
//! used by the parity tests in `tests/`.
//!
//! See `rust/README.md` ("Parity testing against the C implementation") for
//! the rationale and instructions for adding new parity checks.

use std::os::raw::{c_char, c_int};

extern "C" {
    fn ms_ref_hex_to_int(hex: *mut c_char) -> c_int;
    fn ms_ref_encode_char(c: c_char) -> c_int;
    fn ms_ref_get_bit_array_size(numbits: c_int) -> usize;
}

/// Safe wrapper around the C reference `ms_ref_hex_to_int()`
/// (`msHexToInt()` in `src/mapstring.cpp`).
pub fn ref_hex_to_int(hex: &[u8; 2]) -> i32 {
    // The C signature takes a non-const `char *`; it never writes through
    // the pointer, so a local mutable copy keeps this call sound.
    let mut buf = [hex[0] as c_char, hex[1] as c_char];
    unsafe { ms_ref_hex_to_int(buf.as_mut_ptr()) }
}

/// Safe wrapper around the C reference `ms_ref_encode_char()`
/// (`msEncodeChar()` in `src/mapstring.cpp`).
pub fn ref_encode_char(c: u8) -> bool {
    unsafe { ms_ref_encode_char(c as c_char) != 0 }
}

/// Safe wrapper around the C reference `ms_ref_get_bit_array_size()`
/// (`msGetBitArraySize()` in `src/mapbits.c`).
pub fn ref_get_bit_array_size(numbits: i32) -> usize {
    unsafe { ms_ref_get_bit_array_size(numbits) }
}
