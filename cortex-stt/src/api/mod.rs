pub mod auth;
pub mod discovery;
pub mod engine;
pub mod error;
pub mod eval;
pub mod health;
pub mod history;
pub mod keys;
pub mod metrics;
pub mod models;
pub mod range;
pub mod router;
pub mod settings;
pub mod stream;
pub mod system;
pub mod transcribe;

pub use router::build_router;
