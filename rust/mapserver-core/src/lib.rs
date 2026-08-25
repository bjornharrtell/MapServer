//! Core utilities being ported from MapServer's C/C++ codebase to Rust.
//!
//! This crate is the starting point of the incremental Rust port of
//! MapServer (see the project roadmap in `rust/README.md`). Modules here
//! mirror the semantics of their C counterparts so that behavior stays
//! verifiable against the original implementation while new functionality
//! is gradually re-implemented in Rust.

pub mod bits;
pub mod color;
pub mod config;
pub mod datasource;
pub mod debug;
pub mod error;
pub mod mapfile;
pub mod primitive;
pub mod projection;
pub mod string;
