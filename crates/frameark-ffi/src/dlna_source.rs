//! Opaque revocable source handles. Platform code never receives a media URL.

use super::*;
use frameark_dlna::{MAX_SOURCE_READ_BYTES, RemoteMediaSource};
use frameark_media::subtitles::SubtitleTrack;

pub(super) struct NamedSubtitle {
    pub label: String,
    pub track: SubtitleTrack,
}

struct Source {
    media: Arc<RemoteMediaSource>,
    subtitles: Vec<NamedSubtitle>,
    selected: Option<usize>,
}

#[derive(Default)]
struct Sources {
    next: u64,
    entries: BTreeMap<u64, Source>,
}
static SOURCES: OnceLock<Mutex<Sources>> = OnceLock::new();
fn sources() -> &'static Mutex<Sources> {
    SOURCES.get_or_init(|| Mutex::new(Sources::default()))
}

pub(super) fn register(
    source: Arc<RemoteMediaSource>,
    subtitles: Vec<NamedSubtitle>,
) -> Result<u64, ()> {
    let mut sources = sources().lock().map_err(|_| ())?;
    if sources.entries.len() >= 2 || sources.next >= i64::MAX as u64 {
        return Err(());
    }
    sources.next += 1;
    let id = sources.next;
    let selected = (!subtitles.is_empty()).then_some(0);
    sources.entries.insert(
        id,
        Source {
            media: source,
            subtitles,
            selected,
        },
    );
    Ok(id)
}

fn lookup(id: jlong) -> Option<Arc<RemoteMediaSource>> {
    if id <= 0 {
        return None;
    }
    sources()
        .lock()
        .ok()?
        .entries
        .get(&(id as u64))
        .map(|entry| entry.media.clone())
}

pub(super) fn report(id: u64) -> Option<frameark_dlna::RemoteSourceReport> {
    lookup(i64::try_from(id).ok()?).map(|source| source.report())
}

/// Counts/selection only. Never retain URI, language labels or cue payloads.
#[derive(Debug)]
pub(super) struct SubtitleReport {
    pub tracks: usize,
    pub cues: usize,
    pub selected: Option<usize>,
}

pub(super) fn subtitle_report(id: u64) -> Option<SubtitleReport> {
    let sources = sources().lock().ok()?;
    let entry = sources.entries.get(&id)?;
    Some(SubtitleReport {
        tracks: entry.subtitles.len(),
        cues: entry
            .subtitles
            .iter()
            .map(|subtitle| subtitle.track.cue_count())
            .sum(),
        selected: entry.selected,
    })
}

pub(super) fn revoke(id: u64) {
    let source = sources()
        .lock()
        .ok()
        .and_then(|mut sources| sources.entries.remove(&id));
    if let Some(source) = source {
        source.media.close();
    }
}

fn subtitle_options(id: u64) -> Option<Vec<String>> {
    let sources = sources().lock().ok()?;
    Some(
        sources
            .entries
            .get(&id)?
            .subtitles
            .iter()
            .map(|s| s.label.clone())
            .collect(),
    )
}

fn select_subtitle(id: u64, index: i32) -> bool {
    let Ok(mut sources) = sources().lock() else {
        return false;
    };
    let Some(source) = sources.entries.get_mut(&id) else {
        return false;
    };
    if index < -1 || index >= source.subtitles.len() as i32 {
        return false;
    }
    source.selected = (index >= 0).then_some(index as usize);
    true
}

fn subtitle_text(id: u64, position: u64) -> Option<String> {
    let sources = sources().lock().ok()?;
    let source = sources.entries.get(&id)?;
    Some(source.selected.map_or_else(String::new, |index| {
        source.subtitles[index].track.text_at(position)
    }))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_subtitleOptions(
    mut env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    id: jlong,
) -> jni::sys::jobjectArray {
    let mut result = || {
        let options = subtitle_options(u64::try_from(id).ok()?)?;
        let array = env
            .new_object_array(options.len() as i32, "java/lang/String", JObject::null())
            .ok()?;
        for (index, label) in options.iter().enumerate() {
            let text = env.new_string(label).ok()?;
            env.set_object_array_element(&array, index as i32, text)
                .ok()?;
        }
        Some(array.into_raw())
    };
    result().unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_selectSubtitle(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    id: jlong,
    index: jint,
) -> jboolean {
    u8::from(id > 0 && select_subtitle(id as u64, index))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_subtitleText(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    id: jlong,
    position_ms: jlong,
) -> jni::sys::jstring {
    let result = || {
        if id <= 0 || position_ms < 0 {
            return None;
        }
        let text = subtitle_text(id as u64, position_ms as u64)?;
        env.new_string(text).ok().map(|s| s.into_raw())
    };
    result().unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_sourceSize(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    id: jlong,
) -> jlong {
    lookup(id).map_or(-1, |source| source.len() as jlong)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_sourceRead(
    env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    id: jlong,
    position: jlong,
    size: jint,
) -> jbyteArray {
    let result = || {
        if position < 0 || size < 0 || size as usize > MAX_SOURCE_READ_BYTES {
            return None;
        }
        let source = lookup(id)?;
        let bytes = source.read_at(position as u64, size as usize).ok()?;
        env.byte_array_from_slice(&bytes)
            .ok()
            .map(JByteArray::into_raw)
    };
    result().unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_receiver_DlnaNative_sourceClose(
    _env: JNIEnv<'_>,
    _receiver: JObject<'_>,
    id: jlong,
) {
    if id > 0 {
        revoke(id as u64);
    }
}
