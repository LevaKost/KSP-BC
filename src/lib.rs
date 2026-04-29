//! KSP BlueprintShare core library.
//!
//! This crate exposes the building blocks used by the `ksp-share` CLI:
//! discovering a local KSP installation, parsing `.craft` blueprint
//! metadata, and transferring blueprints between two peers over a small
//! framed TCP protocol.

pub mod cli;
pub mod config;
pub mod craft;
pub mod engine;
pub mod error;
pub mod ksp;
pub mod transport;

pub use error::{Error, Result};
