//! tailr-core: the domain core layer.
//!
//! This crate holds tailr's business rules — config, daemon process management,
//! runtime sampling, and the upgrade engine — independent of any presentation
//! layer (CLI / Web / future MCP). See `AGENTS.md` "Architecture Rules" for the
//! boundary contract: no `axum`, no HTTP types, no global runtime, no terminal
//! I/O, no hardcoded business policy.
//!
//! Presentation layers depend on this crate and adapt its output to their format.

pub mod config;
pub mod daemon;
pub mod limits;
pub mod runtime;
pub mod upgrade_engine;
