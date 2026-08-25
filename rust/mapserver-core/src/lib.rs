//! Core utilities being ported from MapServer's C/C++ codebase to Rust.
//!
//! This crate is the starting point of the incremental Rust port of
//! MapServer (see the project roadmap in `rust/README.md`). Modules here
//! mirror the semantics of their C counterparts so that behavior stays
//! verifiable against the original implementation while new functionality
//! is gradually re-implemented in Rust.

pub mod color;

#[cfg(test)]
mod ci_probe {
    #[test]
    fn rust_only_change_probe() {
        assert!(true);
    }
}
