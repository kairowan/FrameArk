//! One-slot acknowledged OS commands with process-unique revision filtering.

use super::*;
use frameark_dlna::{
    MediaPlaybackBackend, MediaUriNetworkPolicy, PlaybackBackendError as Error, PlaybackCommand,
    PlaybackFeedback, RemoteMediaSource,
};
use jni::sys::jlongArray;
use std::sync::Condvar;
use std::sync::atomic::AtomicU64;

static NEXT_COMMAND: AtomicU64 = AtomicU64::new(1);
static OUTPUT: OnceLock<Mutex<Option<OutputBridge>>> = OnceLock::new();

// Internal JNI command 8 prepares a still image; argument 1=JPEG, 2=PNG.
// The OS decoder also checks dimensions/sampling before allocating pixels.
const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;

fn preparation(mime: &str, length: u64) -> Result<(i64, i64), Error> {
    match mime {
        "video/mp4" | "audio/mpeg" | "audio/wav" | "audio/x-wav" => Ok((1, 0)),
        "image/jpeg" | "image/png" if (1..=MAX_IMAGE_BYTES).contains(&length) => {
            Ok((8, if mime == "image/jpeg" { 1 } else { 2 }))
        }
        _ => Err(Error::Unsupported),
    }
}

fn output() -> &'static Mutex<Option<OutputBridge>> {
    OUTPUT.get_or_init(|| Mutex::new(None))
}
pub(super) fn install(bridge: Option<OutputBridge>) {
    if let Ok(mut current) = output().lock() {
        *current = bridge;
    }
}
fn current() -> Option<OutputBridge> {
    output().lock().ok()?.clone()
}

fn unique_id() -> Result<u64, Error> {
    NEXT_COMMAND
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
            (value < i64::MAX as u64).then_some(value + 1)
        })
        .map_err(|_| Error::Failed)
}

#[derive(Clone, Copy)]
struct Command {
    id: u64,
    kind: i64,
    source: u64,
    argument: i64,
}

#[derive(Default)]
struct State {
    loading: Option<Arc<RemoteMediaSource>>,
    pending: Option<Command>,
    in_flight: Option<Command>,
    reply: Option<(u64, bool)>,
    accepted: u64,
    source: u64,
    feedback: Option<PlaybackFeedback>,
    closed: bool,
    release_pending: bool,
}

#[derive(Clone)]
pub(super) struct OutputBridge(Arc<(Mutex<State>, Condvar)>, u64);

impl OutputBridge {
    pub(super) fn subtitle_report(&self) -> Option<dlna_source::SubtitleReport> {
        dlna_source::subtitle_report(self.0.0.lock().ok()?.source)
    }
    pub(super) fn source_report(&self) -> Option<frameark_dlna::RemoteSourceReport> {
        let source = self.0.0.lock().ok()?.source;
        dlna_source::report(source)
    }
    pub(super) fn new() -> Result<Self, Error> {
        Ok(Self(
            Arc::new((Mutex::new(State::default()), Condvar::new())),
            unique_id()?,
        ))
    }

    fn command(&self, kind: i64, source: u64, argument: i64) -> Result<(), Error> {
        self.command_with_timeout(kind, source, argument, Duration::from_secs(5))
    }

    fn command_with_timeout(
        &self,
        kind: i64,
        source: u64,
        argument: i64,
        timeout: Duration,
    ) -> Result<(), Error> {
        let id = unique_id()?;
        let (lock, wake) = &*self.0;
        let mut state = lock.lock().map_err(|_| Error::Failed)?;
        if state.closed {
            return Err(Error::Unavailable);
        }
        if state.pending.is_some() || state.in_flight.is_some() || state.reply.is_some() {
            return Err(Error::Rejected);
        }
        if state.feedback == Some(PlaybackFeedback::Failed) {
            return Err(Error::Failed);
        }
        state.feedback = None;
        state.reply = None;
        state.pending = Some(Command {
            id,
            kind,
            source,
            argument,
        });
        let deadline = Instant::now() + timeout;
        loop {
            if state.closed {
                return Err(Error::Failed);
            }
            if let Some((reply, success)) = state.reply.take()
                && reply == id
            {
                state.in_flight = None;
                if success {
                    return Ok(());
                }
                return Err(Error::Failed);
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                drop(state);
                self.close();
                return Err(Error::Failed);
            };
            let result = wake
                .wait_timeout(state, remaining)
                .map_err(|_| Error::Failed)?;
            state = result.0;
        }
    }

    fn poll(&self) -> Option<Command> {
        let mut state = self.0.0.lock().ok()?;
        if state.release_pending {
            state.release_pending = false;
            return Some(Command {
                id: 0,
                kind: 7,
                source: 0,
                argument: 0,
            });
        }
        if state.closed {
            return None;
        }
        let command = state.pending.take()?;
        state.in_flight = Some(command);
        Some(command)
    }

    fn poll_for(&self, token: u64) -> Option<Command> {
        if token == 0 || token != self.1 {
            return None;
        }
        self.poll()
    }

    fn acknowledge(&self, id: u64, success: bool) -> bool {
        let Ok(mut state) = self.0.0.lock() else {
            return false;
        };
        if state.closed
            || state.in_flight.is_none_or(|command| command.id != id)
            || state.reply.is_some()
        {
            return false;
        }
        state.reply = Some((id, success));
        if success {
            state.accepted = id;
            state.source = state.in_flight.expect("validated command").source;
        }
        self.0.1.notify_all();
        true
    }

    fn feedback(&self, id: u64, value: PlaybackFeedback) -> bool {
        let Ok(mut state) = self.0.0.lock() else {
            return false;
        };
        if state.closed
            || state.accepted != id
            || id == 0
            || state.pending.is_some()
            || (state.in_flight.is_some() && state.reply != Some((id, true)))
        {
            return false;
        }
        // A progress tick must not overwrite an unconsumed terminal observation.
        if value == PlaybackFeedback::Failed
            || !matches!(
                state.feedback,
                Some(PlaybackFeedback::Ended | PlaybackFeedback::Failed)
            )
        {
            state.feedback = Some(value);
        }
        true
    }

    pub(super) fn close(&self) {
        let Ok(mut state) = self.0.0.lock() else {
            return;
        };
        if state.closed {
            return;
        }
        state.closed = true;
        state.release_pending = true;
        let candidate = state
            .pending
            .or(state.in_flight)
            .map(|command| command.source);
        let selected = state.source;
        let loading = state.loading.take();
        state.pending = None;
        state.in_flight = None;
        state.reply = None;
        state.feedback = None;
        self.0.1.notify_all();
        drop(state);
        if let Some(source) = loading {
            source.close();
        }
        if let Some(id) = candidate {
            dlna_source::revoke(id);
        }
        dlna_source::revoke(selected);
    }
}

pub(super) struct AndroidDlnaBackend {
    bridge: OutputBridge,
    origin: Arc<Mutex<IpAddr>>,
    device_id: DeviceId,
    source: Option<(u64, Arc<RemoteMediaSource>)>,
    session: Option<Session>,
    loopback_probe: bool,
}

impl AndroidDlnaBackend {
    pub(super) fn new(
        bridge: OutputBridge,
        origin: Arc<Mutex<IpAddr>>,
        device_id: DeviceId,
        loopback_probe: bool,
    ) -> Self {
        Self {
            bridge,
            origin,
            device_id,
            source: None,
            session: None,
            loopback_probe,
        }
    }

    fn prepare_session(&mut self, id: u64) -> Result<(), Error> {
        if let Some(session) = &mut self.session
            && session.state() != SessionState::Closed
        {
            session
                .transition(SessionState::Closed)
                .map_err(|_| Error::Failed)?;
        }
        let mut session = Session::new(
            SessionId::try_from(format!("dlna-{id}-{}", unique_id()?))
                .map_err(|_| Error::Failed)?,
            self.device_id.clone(),
        );
        for state in [
            SessionState::Connecting,
            SessionState::Authenticating,
            SessionState::Negotiating,
            SessionState::Preparing,
        ] {
            session.transition(state).map_err(|_| Error::Failed)?;
        }
        self.session = Some(session);
        Ok(())
    }

    fn close_session(&mut self) {
        if let Some(session) = &mut self.session
            && session.state() != SessionState::Closed
        {
            if session.state() == SessionState::Streaming {
                let _ = session.transition(SessionState::Draining);
            }
            let _ = session.transition(SessionState::Closed);
        }
    }

    fn is_image(&self) -> bool {
        self.source
            .as_ref()
            .is_some_and(|(_, source)| matches!(source.mime_type(), "image/jpeg" | "image/png"))
    }

    fn load_subtitles(
        &self,
        metadata: &str,
        uri: &str,
        origin: IpAddr,
        policy: MediaUriNetworkPolicy,
    ) -> Result<Vec<dlna_source::NamedSubtitle>, Error> {
        use frameark_media::subtitles::{MAX_SUBTITLE_BYTES, SubtitleFormat, SubtitleTrack};
        let resources =
            frameark_dlna::subtitle_resources(metadata, uri).map_err(|_| Error::Rejected)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut total = 0usize;
        let mut tracks = Vec::new();
        for resource in resources {
            if self.bridge.0.0.lock().map_err(|_| Error::Failed)?.closed {
                return Err(Error::Failed);
            }
            let source = Arc::new(
                RemoteMediaSource::open_until(
                    &resource.uri,
                    policy,
                    vec![origin],
                    None,
                    Duration::from_secs(2),
                    deadline,
                )
                .map_err(|_| Error::Rejected)?,
            );
            let format = match (resource.mime_type.as_str(), source.mime_type()) {
                ("text/vtt", "text/vtt") => SubtitleFormat::WebVtt,
                ("text/srt" | "application/x-subrip", "text/srt" | "application/x-subrip") => {
                    SubtitleFormat::SubRip
                }
                _ => return Err(Error::Unsupported),
            };
            let length = usize::try_from(source.len()).map_err(|_| Error::Rejected)?;
            if length > MAX_SUBTITLE_BYTES - total {
                return Err(Error::Rejected);
            }
            {
                let mut state = self.bridge.0.0.lock().map_err(|_| Error::Failed)?;
                if state.closed {
                    return Err(Error::Failed);
                }
                state.loading = Some(source.clone());
            }
            let result = (|| {
                let mut bytes = Vec::with_capacity(length);
                while bytes.len() < length {
                    let chunk = source
                        .read_at(bytes.len() as u64, length - bytes.len())
                        .map_err(|_| Error::Rejected)?;
                    if chunk.is_empty() {
                        return Err(Error::Rejected);
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let track = SubtitleTrack::parse(&bytes, format).map_err(|_| Error::Unsupported)?;
                if Instant::now() >= deadline {
                    return Err(Error::Rejected);
                }
                Ok(track)
            })();
            source.close();
            self.bridge.0.0.lock().map_err(|_| Error::Failed)?.loading = None;
            total += length;
            let format_name = if format == SubtitleFormat::WebVtt {
                "WebVTT"
            } else {
                "SRT"
            };
            let label = if resource.language.is_empty() {
                format!("{} · {format_name}", tracks.len() + 1)
            } else {
                format!("{} · {format_name}", resource.language)
            };
            tracks.push(dlna_source::NamedSubtitle {
                label,
                track: result?,
            });
        }
        Ok(tracks)
    }
}

impl MediaPlaybackBackend for AndroidDlnaBackend {
    fn apply(&mut self, command: PlaybackCommand<'_>) -> Result<(), Error> {
        if let PlaybackCommand::SetUri { uri, metadata } = command {
            let origin = *self.origin.lock().map_err(|_| Error::Failed)?;
            let policy = if self.loopback_probe {
                MediaUriNetworkPolicy::AllowAny
            } else {
                MediaUriNetworkPolicy::LocalNetworkOnly
            };
            let source = Arc::new(
                RemoteMediaSource::open(uri, policy, vec![origin], None, Duration::from_secs(2))
                    .map_err(|_| Error::Rejected)?,
            );
            let (kind, argument) = preparation(source.mime_type(), source.len())?;
            let subtitles = self.load_subtitles(metadata, uri, origin, policy)?;
            if kind == 8 && !subtitles.is_empty() {
                return Err(Error::Unsupported);
            }
            let id =
                dlna_source::register(source.clone(), subtitles).map_err(|_| Error::Unavailable)?;
            if let Err(error) = self.bridge.command(kind, id, argument) {
                dlna_source::revoke(id);
                return Err(error);
            }
            if let Some((old, _)) = self.source.replace((id, source)) {
                dlna_source::revoke(old);
            }
            self.prepare_session(id)?;
            return Ok(());
        }
        if matches!(command, PlaybackCommand::Seek { position_seconds } if position_seconds > 0)
            && self.is_image()
        {
            return Err(Error::Unsupported);
        }
        let source = self.source.as_ref().map_or(0, |(id, _)| *id);
        let (kind, argument) = match command {
            PlaybackCommand::Play => (2, 0),
            PlaybackCommand::Pause => (3, 0),
            PlaybackCommand::Stop => (4, 0),
            PlaybackCommand::Seek { position_seconds } => (
                5,
                i64::try_from(position_seconds.checked_mul(1000).ok_or(Error::Rejected)?)
                    .map_err(|_| Error::Rejected)?,
            ),
            PlaybackCommand::SetVolume { volume } => (6, i64::from(volume)),
            PlaybackCommand::SetUri { .. } => unreachable!(),
        };
        if source == 0 && kind != 6 {
            return Err(Error::Rejected);
        }
        self.bridge.command(kind, source, argument)?;
        if kind == 2 {
            if self
                .session
                .as_ref()
                .is_none_or(|session| session.state() == SessionState::Closed)
            {
                self.prepare_session(source)?;
            }
            if let Some(session) = &mut self.session
                && session.state() != SessionState::Streaming
            {
                session
                    .transition(SessionState::Streaming)
                    .map_err(|_| Error::Failed)?;
            }
        } else if kind == 4 {
            self.close_session();
        }
        Ok(())
    }

    fn feedback(&mut self) -> Option<PlaybackFeedback> {
        let value = self.bridge.0.0.lock().ok()?.feedback.take();
        if matches!(
            value,
            Some(PlaybackFeedback::Ended | PlaybackFeedback::Failed)
        ) {
            self.close_session();
        }
        value.or_else(|| {
            // Stills have no advancing media clock or automatic EOS. This is
            // protocol semantics, not invented decoder progress or duration.
            self.is_image().then_some(PlaybackFeedback::Progress {
                position_seconds: 0,
                duration_seconds: Some(0),
            })
        })
    }

    fn shutdown(&mut self) {
        self.bridge.close();
        if let Some((id, _)) = self.source.take() {
            dlna_source::revoke(id);
        }
        self.close_session();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_pollCommand(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    token: jlong,
) -> jlongArray {
    let result = || {
        let bridge = current()?;
        if token <= 0 {
            return None;
        }
        let command = bridge.poll_for(token as u64)?;
        let array = env.new_long_array(4).ok()?;
        env.set_long_array_region(
            &array,
            0,
            &[
                command.id as i64,
                command.kind,
                command.source as i64,
                command.argument,
            ],
        )
        .ok()?;
        Some(array.into_raw())
    };
    result().unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_outputToken(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
) -> jlong {
    current().map_or(0, |bridge| bridge.1 as jlong)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_acknowledge(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    id: jlong,
    success: jboolean,
) -> jboolean {
    u8::from(id > 0 && current().is_some_and(|bridge| bridge.acknowledge(id as u64, success != 0)))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_feedback(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    id: jlong,
    kind: jint,
    position_ms: jlong,
    duration_ms: jlong,
) -> jboolean {
    if id <= 0 {
        return 0;
    }
    let value = match kind {
        0 if position_ms >= 0 && duration_ms >= -1 => PlaybackFeedback::Progress {
            position_seconds: position_ms as u64 / 1000,
            duration_seconds: (duration_ms >= 0).then_some(duration_ms as u64 / 1000),
        },
        1 => PlaybackFeedback::Ended,
        2 => PlaybackFeedback::Failed,
        _ => return 0,
    };
    u8::from(current().is_some_and(|bridge| bridge.feedback(id as u64, value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn still_preparation_is_typed_and_bounded_before_output() {
        for (mime, kind) in [("image/jpeg", 1), ("image/png", 2)] {
            assert_eq!(preparation(mime, 1), Ok((8, kind)));
            assert_eq!(preparation(mime, MAX_IMAGE_BYTES), Ok((8, kind)));
            for length in [0, MAX_IMAGE_BYTES + 1, u64::MAX] {
                assert_eq!(preparation(mime, length), Err(Error::Unsupported));
            }
        }
        for mime in [
            "image/gif",
            "image/svg+xml",
            "image/webp",
            "text/html",
            "image/jpg",
        ] {
            assert_eq!(preparation(mime, 100), Err(Error::Unsupported));
        }
        assert_eq!(preparation("video/mp4", MAX_IMAGE_BYTES + 1), Ok((1, 0)));
        assert_eq!(preparation("audio/mpeg", 100), Ok((1, 0)));
    }

    #[test]
    fn image_source_commands_keep_rust_session_and_timeless_seek_semantics() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(1))
                    }
                    other => panic!("bounded image origin accept: {other:?}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") && request.len() < 4096 {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            assert!(
                String::from_utf8(request)
                    .unwrap()
                    .contains("Range: bytes=0-0")
            );
            socket.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Type: image/jpeg\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/100\r\n\r\n\xff").unwrap();
        });
        let bridge = OutputBridge::new().unwrap();
        let output = bridge.clone();
        let worker = thread::spawn(move || {
            let mut backend = AndroidDlnaBackend::new(
                output,
                Arc::new(Mutex::new(address.ip())),
                DeviceId::try_from("image-fixture".to_string()).unwrap(),
                true,
            );
            backend
                .apply(PlaybackCommand::SetUri {
                    uri: &format!("http://{address}/photo"),
                    metadata: "",
                })
                .unwrap();
            assert_eq!(
                backend.session.as_ref().unwrap().state(),
                SessionState::Preparing
            );
            assert_eq!(
                backend.feedback(),
                Some(PlaybackFeedback::Progress {
                    position_seconds: 0,
                    duration_seconds: Some(0)
                })
            );
            assert_eq!(
                backend.apply(PlaybackCommand::Seek {
                    position_seconds: 1
                }),
                Err(Error::Unsupported)
            );
            backend.apply(PlaybackCommand::Play).unwrap();
            assert_eq!(
                backend.session.as_ref().unwrap().state(),
                SessionState::Streaming
            );
            backend.apply(PlaybackCommand::Pause).unwrap();
            backend.apply(PlaybackCommand::Stop).unwrap();
            assert_eq!(
                backend.session.as_ref().unwrap().state(),
                SessionState::Closed
            );
            backend.apply(PlaybackCommand::Play).unwrap();
            assert_eq!(
                backend.session.as_ref().unwrap().state(),
                SessionState::Streaming
            );
            backend.shutdown();
            assert_eq!(
                backend.session.as_ref().unwrap().state(),
                SessionState::Closed
            );
        });
        let mut source = 0;
        for expected in [8, 2, 3, 4, 2] {
            let command = await_command(&bridge);
            assert_eq!(command.kind, expected);
            if expected == 8 {
                source = command.source;
                assert_eq!(command.argument, 1);
            }
            assert_ne!(source, 0);
            assert_eq!(command.source, source);
            assert!(bridge.acknowledge(command.id, true));
        }
        worker.join().unwrap();
        origin.join().unwrap();
        assert!(dlna_source::report(source).is_none());
        assert_eq!(bridge.poll().unwrap().kind, 7);
    }

    fn await_command(bridge: &OutputBridge) -> Command {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(command) = bridge.poll_for(bridge.1) {
                return command;
            }
            assert!(Instant::now() < deadline, "command was not queued");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn acknowledgment_is_single_use_and_feedback_is_generation_scoped() {
        let bridge = OutputBridge::new().unwrap();
        let sender = bridge.clone();
        let command = thread::spawn(move || sender.command(6, 0, 73));
        let received = await_command(&bridge);
        assert_eq!((received.kind, received.argument), (6, 73));
        assert_eq!(bridge.command(6, 0, 20), Err(Error::Rejected));
        assert!(!bridge.acknowledge(received.id + 1, true));
        assert!(!bridge.feedback(received.id, PlaybackFeedback::Ended));
        assert!(bridge.acknowledge(received.id, true));
        // No lost observation between JNI acknowledgement and the Rust waiter.
        assert!(bridge.feedback(received.id, PlaybackFeedback::Ended));
        assert!(!bridge.acknowledge(received.id, true));
        assert_eq!(command.join().unwrap(), Ok(()));
        assert!(bridge.feedback(
            received.id,
            PlaybackFeedback::Progress {
                position_seconds: 1,
                duration_seconds: Some(2)
            }
        ));
        assert_eq!(
            bridge.0.0.lock().unwrap().feedback,
            Some(PlaybackFeedback::Ended)
        );
        assert!(bridge.feedback(received.id, PlaybackFeedback::Failed));
        assert_eq!(bridge.command(6, 0, 30), Err(Error::Failed));
        assert_eq!(
            bridge.0.0.lock().unwrap().feedback,
            Some(PlaybackFeedback::Failed)
        );
        bridge.close();
    }

    #[test]
    fn old_consumer_cannot_steal_new_runtime_commands() {
        let old = OutputBridge::new().unwrap();
        let bridge = OutputBridge::new().unwrap();
        assert_ne!(bridge.1, old.1);
        let sender = bridge.clone();
        let command = thread::spawn(move || sender.command(6, 0, 50));
        assert!(bridge.poll_for(old.1).is_none());
        let received = await_command(&bridge);
        assert!(!old.acknowledge(received.id, true));
        assert!(!old.feedback(received.id, PlaybackFeedback::Failed));
        assert!(bridge.acknowledge(received.id, false));
        assert_eq!(command.join().unwrap(), Err(Error::Failed));
        bridge.close();
    }

    #[test]
    fn close_interrupts_wait_and_emits_only_one_release() {
        let bridge = OutputBridge::new().unwrap();
        let sender = bridge.clone();
        let command = thread::spawn(move || sender.command(2, 0, 0));
        let received = await_command(&bridge);
        let started = Instant::now();
        bridge.close();
        bridge.close();
        assert_eq!(command.join().unwrap(), Err(Error::Failed));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(!bridge.acknowledge(received.id, true));
        assert!(!bridge.feedback(received.id, PlaybackFeedback::Ended));
        assert_eq!(bridge.poll().unwrap().kind, 7);
        assert!(bridge.poll().is_none());
        assert_eq!(bridge.command(2, 0, 0), Err(Error::Unavailable));
    }

    #[test]
    fn unacknowledged_command_times_out_and_revokes_bridge() {
        let bridge = OutputBridge::new().unwrap();
        assert_eq!(
            bridge.command_with_timeout(6, 0, 50, Duration::from_millis(20)),
            Err(Error::Failed)
        );
        assert_eq!(bridge.poll().unwrap().kind, 7);
        assert!(bridge.poll().is_none());
        assert_eq!(bridge.command(6, 0, 50), Err(Error::Unavailable));
    }
}
