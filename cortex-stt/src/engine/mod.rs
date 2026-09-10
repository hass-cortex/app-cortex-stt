pub mod manager;
pub mod pool;
pub mod register;
pub mod testing;
pub mod traits;

#[cfg(feature = "engine")]
pub mod transcribe_bridge;

/// The speech runtime this build is bound to, recorded on evaluation
/// runs so a later run can say which library produced which transcript.
#[cfg(feature = "engine")]
pub const ENGINE_VERSION: &str = transcribe_bridge::TRANSCRIBE_CPP_VERSION;

/// Mock builds have no runtime; runs still record something truthful.
#[cfg(not(feature = "engine"))]
pub const ENGINE_VERSION: &str = "mock";
