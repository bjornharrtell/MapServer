//! Parity tests comparing `mapserver-core` Rust ports against verbatim
//! copies of the original C reference functions (see `c_ref/` and
//! `build.rs`). A failure here means the Rust port has diverged in
//! observable behavior from the C implementation it was ported from.

use mapserver_core::bits::get_bit_array_size;
use mapserver_core::color::hex_to_int;
use mapserver_core::string::encode_char;
use mapserver_parity_tests::{ref_encode_char, ref_get_bit_array_size, ref_hex_to_int};

#[test]
fn hex_to_int_matches_c_reference() {
    // All valid hex digit pairs, both cases, per msHexToInt()'s expected
    // input domain (0-9, A-F/a-f).
    let digits: Vec<u8> = (b'0'..=b'9')
        .chain(b'A'..=b'F')
        .chain(b'a'..=b'f')
        .collect();

    for &a in &digits {
        for &b in &digits {
            let pair = [a, b];
            assert_eq!(
                hex_to_int(&pair),
                ref_hex_to_int(&pair),
                "mismatch for hex pair {:?}",
                pair.map(|c| c as char)
            );
        }
    }
}

#[test]
fn encode_char_matches_c_reference_over_all_bytes() {
    for c in 0u8..=255 {
        assert_eq!(
            encode_char(c),
            ref_encode_char(c),
            "mismatch for byte {c:#04x}"
        );
    }
}

#[test]
fn get_bit_array_size_matches_c_reference() {
    for numbits in 0..=1024usize {
        assert_eq!(
            get_bit_array_size(numbits),
            ref_get_bit_array_size(numbits as i32),
            "mismatch for numbits={numbits}"
        );
    }
}
