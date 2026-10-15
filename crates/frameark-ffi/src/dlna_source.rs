//! Opaque revocable source handles. Platform code never receives a media URL.

use super::*;
use frameark_dlna::{MAX_SOURCE_READ_BYTES, RemoteMediaSource};

#[derive(Default)]
struct Sources {
    next: u64,
    entries: BTreeMap<u64, Arc<RemoteMediaSource>>,
}
static SOURCES: OnceLock<Mutex<Sources>> = OnceLock::new();
fn sources() -> &'static Mutex<Sources> {
    SOURCES.get_or_init(|| Mutex::new(Sources::default()))
}

pub(super) fn register(source: Arc<RemoteMediaSource>) -> Result<u64, ()> {
    let mut sources = sources().lock().map_err(|_| ())?;
    if sources.entries.len() >= 2 || sources.next >= i64::MAX as u64 {
        return Err(());
    }
    sources.next += 1;
    let id = sources.next;
    sources.entries.insert(id, source);
    Ok(id)
}

fn lookup(id: jlong) -> Option<Arc<RemoteMediaSource>> {
    if id <= 0 {
        return None;
    }
    sources().lock().ok()?.entries.get(&(id as u64)).cloned()
}

pub(super) fn report(id: u64) -> Option<frameark_dlna::RemoteSourceReport> {
    lookup(i64::try_from(id).ok()?).map(|source| source.report())
}

pub(super) fn revoke(id: u64) {
    let source = sources()
        .lock()
        .ok()
        .and_then(|mut sources| sources.entries.remove(&id));
    if let Some(source) = source {
        source.close();
    }
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
