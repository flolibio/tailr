//! Web-layer re-exports of the core [`RuntimeSampler`].
//!
//! The sampling algorithm (sysinfo refresh + TTL cache + /proc CPU diff) lives
//! in `tailr_core::runtime` as a **synchronous** `sample_blocking` — per the
//! core-layer rule "computation stays sync". The Web layer's async adaptation
//! (wrapping the call in `spawn_blocking`) happens at the call site in
//! `api.rs::runtime`, not here — keeping this module a pure re-export avoids
//! orphan-rule issues (inherent impls must live in the defining crate).
//!
//! Concurrency note: concurrent callers that find the cache stale serialize
//! inside `sample_blocking` (on a `std::sync::Mutex`); the first refreshes,
//! followers re-check TTL (now fresh) and return the just-cached snapshot
//! without triggering their own refresh. Because each caller runs on the
//! blocking pool, queued callers occupy a blocking thread (capped at 4) —
//! acceptable since the 5s TTL means refreshes are rare (≈1 per 5s).

pub use tailr_core::runtime::{RuntimeSampler, RuntimeSnapshot};
