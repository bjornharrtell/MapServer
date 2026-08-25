/*
 * Verbatim copy of `msHexToInt()` from `src/mapstring.cpp`, extracted so it
 * can be compiled standalone (without the rest of the MapServer/GDAL build)
 * for parity testing against `mapserver_core::color::hex_to_int()`.
 *
 * DO NOT "fix" or modernize this file to make tests pass: if the ported
 * Rust behavior and this reference disagree, the Rust port has a bug (or
 * this copy has drifted from `src/mapstring.cpp` and needs to be
 * re-synced), not the other way around.
 */

int ms_ref_hex_to_int(char *hex) {
  int number;

  number = (hex[0] >= 'A' ? ((hex[0] & 0xdf) - 'A') + 10 : (hex[0] - '0'));
  number *= 16;
  number += (hex[1] >= 'A' ? ((hex[1] & 0xdf) - 'A') + 10 : (hex[1] - '0'));

  return (number);
}
