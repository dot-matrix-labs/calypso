//! Thin re-export layer — all library code lives in the `nightshift` crate.
//!
//! This module re-exports everything from `nightshift` so that existing
//! `use calypso_cli::*` paths continue to work unchanged.

pub use nightshift::*;
