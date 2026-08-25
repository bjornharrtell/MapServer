/*
 * Verbatim copy of `msGetBitArraySize()` from `src/mapbits.c` (with the
 * `MS_ARRAY_BIT` constant from `src/mapserver.h` inlined), extracted so it
 * can be compiled standalone for parity testing against
 * `mapserver_core::bits::get_bit_array_size()`.
 *
 * DO NOT "fix" or modernize this file to make tests pass: if the ported
 * Rust behavior and this reference disagree, the Rust port has a bug (or
 * this copy has drifted from `src/mapbits.c`/`src/mapserver.h` and needs to
 * be re-synced), not the other way around.
 */

#include <stddef.h>

#define MS_ARRAY_BIT 32

size_t ms_ref_get_bit_array_size(int numbits) {
  return ((numbits + MS_ARRAY_BIT - 1) / MS_ARRAY_BIT);
}
