/*
 * Verbatim copy of `msEncodeChar()` from `src/mapstring.cpp`, extracted so
 * it can be compiled standalone for parity testing against
 * `mapserver_core::string::encode_char()`.
 *
 * DO NOT "fix" or modernize this file to make tests pass: if the ported
 * Rust behavior and this reference disagree, the Rust port has a bug (or
 * this copy has drifted from `src/mapstring.cpp` and needs to be
 * re-synced), not the other way around.
 */

int ms_ref_encode_char(const char c) {
  if ((c >= 0x61 && c <= 0x7A) || /* Letters a-z */
      (c >= 0x41 && c <= 0x5A) || /* Letters A-Z */
      (c >= 0x30 && c <= 0x39) || /* Numbers 0-9 */
      (c >= 0x27 && c <= 0x2A) || /* * ' ( )     */
      (c >= 0x2D && c <= 0x2E) || /* - .         */
      (c == 0x5F) ||              /* _           */
      (c == 0x21) ||              /* !           */
      (c == 0x7E)) {              /* ~           */
    return (0);
  } else {
    return (1);
  }
}
