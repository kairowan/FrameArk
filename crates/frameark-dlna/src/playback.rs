//! Typed, synchronous output commands. Protocol state stays in Rust.

/// A validated command borrowed only for the duration of backend dispatch.
///
/// URI syntax validation is not network authorization. The host must apply
/// Rust-owned URI/DNS/address/redirect policy before opening a media source.
/// Do not log these values: URLs and metadata can contain private information.
pub enum PlaybackCommand<'a> {
    /// Replace the source and release the previous source/output before success.
    SetUri {
        /// Validated HTTP(S) URI; not authorization to fetch it.
        uri: &'a str,
        /// Bounded, untrusted DIDL-Lite text; not executable markup.
        metadata: &'a str,
    },
    /// Start or resume the selected source at normal speed.
    Play,
    /// Pause the selected source without losing its position.
    Pause,
    /// Stop output, retaining the selected URI for a subsequent Play.
    Stop,
    /// Seek to a validated relative position in whole seconds.
    Seek {
        /// Position, checked against the known duration when available.
        position_seconds: u64,
    },
    /// Set the master output level, always in the inclusive 0..100 range.
    SetVolume {
        /// Percent of the backend's master output level.
        volume: u8,
    },
}

/// Redacted backend outcomes; no platform exception or private URL is exposed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackBackendError {
    /// No backend is bound, or its output is temporarily unavailable.
    Unavailable,
    /// The backend cannot execute this command or media format.
    Unsupported,
    /// The command was rejected without changing existing output.
    Rejected,
    /// Output is no longer trustworthy; detach and shut down this backend.
    Failed,
}

/// Platform output implementation owned by one Rust MediaRenderer service.
///
/// Methods execute synchronously on the caller's thread. Implementations must
/// bound I/O/queue waits and never retain borrowed command data. `Ok(())` means
/// the command was accepted by the output, not that a frame has been displayed.
/// `Unavailable`, `Unsupported` and `Rejected` MUST leave prior output intact;
/// if an operation partially changed output or cleanup is uncertain, return
/// `Failed`. The service then invalidates protocol media state, calls shutdown,
/// and permanently detaches the backend. There is no automatic retry/fallback.
///
/// This interface does not transfer protocol/session/trust policy to platform
/// code. A Rust host owns media fetching and the shared session lifecycle; the
/// platform implements only OS demux/decode/output operations. A synchronous
/// trait cannot preempt a blocked implementation or recover from its panic.
pub trait MediaPlaybackBackend: Send {
    /// Execute exactly one already-validated output command.
    fn apply(&mut self, command: PlaybackCommand<'_>) -> Result<(), PlaybackBackendError>;

    /// Release output, sources, queues and workers. Must be bounded/idempotent.
    /// Called on fatal failure or when the owning service is dropped.
    fn shutdown(&mut self);
}

/// Payload-free, saturating dispatch counters for diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlaybackDispatchReport {
    /// Valid mutations submitted to the output boundary (including unbound).
    pub attempted: u64,
    /// Commands acknowledged by the backend.
    pub accepted: u64,
    /// Nonfatal rejections, including an absent backend.
    pub rejected: u64,
    /// Fatal output failures that detached a backend.
    pub fatal_failures: u64,
    /// Most recent dispatch result; cleared after a successful command.
    pub last_error: Option<PlaybackBackendError>,
}
