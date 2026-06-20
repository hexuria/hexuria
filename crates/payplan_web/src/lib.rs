//! Application composition root.
//!
//! Wires every dependency (Postgres pool, repositories, module registry, auth,
//! password hashing) and exposes the `AppContext` that handlers receive.
//!
//! The web crate is intentionally framework-agnostic at this layer; routes
//! are defined in `routes.rs` and built with axum. The same handler functions
//! can later be wrapped behind Spin/Leptos server functions without changes
//! to the underlying business logic.

pub mod context;
pub mod handlers;
pub mod routes;
pub mod session;

pub use context::AppContext;
