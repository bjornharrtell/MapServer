//! Compiles the verbatim C reference implementations in `c_ref/` into a
//! static library so they can be called from the parity tests via FFI. See
//! `rust/README.md` ("Parity testing against the C implementation") for how
//! this fits into the overall parity-testing approach.

fn main() {
    cc::Build::new()
        .file("c_ref/color.c")
        .file("c_ref/string.c")
        .file("c_ref/bits.c")
        .compile("mapserver_parity_c_ref");
}
