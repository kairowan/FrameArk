//! Protocol-independent contracts for FrameArk receivers and senders.
//!
//! This crate owns the shared state and data model. Network protocol adapters and
//! platform renderers depend on it, while this crate remains independent of any
//! operating system, codec library, or asynchronous runtime.

mod config;
mod error;
mod event;
mod ids;
mod model;
mod session;

/// Version of the native ABI exposed to platform bindings.
pub const CORE_ABI_VERSION: u32 = 1;

pub use config::CoreConfig;
pub use error::{ErrorKind, FrameArkError, Result};
pub use event::{CoreEvent, DiagnosticEvent, EventLevel};
pub use ids::{DeviceId, IdentifierError, SessionId, TrackId};
pub use model::{
    AudioConfig, Capability, Device, MediaCodec, Protocol, TimeBase, Track, TrackKind, VideoConfig,
};
pub use session::{Session, SessionState, SessionTransition};
