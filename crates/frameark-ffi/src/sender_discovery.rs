//! One foreground scan at a time; stale JNI generations cannot read/cancel it.
use super::*;
use frameark_native::discovery::CandidateCache;
use jni::sys::jobjectArray;
use std::sync::atomic::{AtomicI32, AtomicI64};

const SCAN_TIME: Duration = Duration::from_secs(15);
const MAX_EVENTS_PER_SCAN: usize = 4096;
static NEXT_SCAN: AtomicI64 = AtomicI64::new(1);
static SCAN: Mutex<Option<Scan>> = Mutex::new(None);

struct Scan {
    id: i64,
    cancel: Arc<AtomicBool>,
    status: Arc<AtomicI32>,
    cache: Arc<Mutex<CandidateCache>>,
    join: JoinHandle<()>,
}
impl Scan {
    fn spawn() -> Option<Self> {
        let id = NEXT_SCAN
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .ok()?;
        let cancel = Arc::new(AtomicBool::new(false));
        let status = Arc::new(AtomicI32::new(1));
        let cache = Arc::new(Mutex::new(CandidateCache::default()));
        let (worker_cancel, worker_status, worker_cache) =
            (cancel.clone(), status.clone(), cache.clone());
        let join = thread::Builder::new()
            .name("frameark-sender-discovery".into())
            .spawn(move || {
                let result = run_scan(&worker_cancel, &worker_cache);
                worker_status.store(if result.is_ok() { 2 } else { -1 }, Ordering::Release);
            })
            .ok()?;
        Some(Self {
            id,
            cancel,
            status,
            cache,
            join,
        })
    }
    fn matches(&self, id: i64) -> bool {
        self.id == id && !self.cancel.load(Ordering::Acquire)
    }
}

fn run_scan(cancel: &AtomicBool, cache: &Mutex<CandidateCache>) -> Result<(), ()> {
    let discovery = MdnsDiscovery::new(DiscoveryConfig::all_interfaces()).map_err(|_| ())?;
    let result = (|| {
        let browser = discovery.browse().map_err(|_| ())?;
        let deadline = Instant::now() + SCAN_TIME;
        let mut events = 0;
        while !cancel.load(Ordering::Acquire) && Instant::now() < deadline {
            if let Some(event) = browser
                .recv_timeout(Duration::from_millis(100))
                .map_err(|_| ())?
            {
                events += 1;
                if events > MAX_EVENTS_PER_SCAN {
                    return Err(());
                }
                let mut cache = cache.lock().map_err(|_| ())?;
                if !cancel.load(Ordering::Acquire) {
                    cache.update(event, Instant::now());
                }
            }
        }
        Ok(())
    })();
    let shutdown = discovery.shutdown().map_err(|_| ());
    result.and(shutdown)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_discoveryStart(
    _env: JNIEnv<'_>,
    _object: JObject<'_>,
) -> jlong {
    let Ok(mut slot) = SCAN.lock() else {
        return -1;
    };
    if slot.as_ref().is_some_and(|scan| !scan.join.is_finished()) {
        return -2;
    }
    if let Some(previous) = slot.take() {
        let _ = previous.join.join();
    }
    let Some(scan) = Scan::spawn() else {
        return -1;
    };
    let id = scan.id;
    *slot = Some(scan);
    id
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_discoveryStatus(
    _env: JNIEnv<'_>,
    _object: JObject<'_>,
    id: jlong,
) -> jint {
    SCAN.lock()
        .ok()
        .and_then(|slot| {
            slot.as_ref()
                .filter(|scan| scan.matches(id))
                .map(|scan| scan.status.load(Ordering::Acquire))
        })
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_discoveryTickets(
    mut env: JNIEnv<'_>,
    _object: JObject<'_>,
    id: jlong,
) -> jobjectArray {
    let tickets = (|| {
        let slot = SCAN.lock().ok()?;
        let scan = slot.as_ref().filter(|scan| scan.matches(id))?;
        if scan.status.load(Ordering::Acquire) < 0 {
            return None;
        }
        Some(scan.cache.lock().ok()?.snapshot(Instant::now()))
    })()
    .unwrap_or_default();
    let result = (|| {
        let array = env
            .new_object_array(tickets.len() as i32, "java/lang/String", JObject::null())
            .ok()?;
        for (index, ticket) in tickets.iter().enumerate() {
            let text = env.new_string(ticket.encode()).ok()?;
            env.set_object_array_element(&array, index as i32, &text)
                .ok()?;
            env.delete_local_ref(text).ok()?;
        }
        Some(array.into_raw())
    })();
    result.unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_frameark_sender_SenderNative_discoveryStop(
    _env: JNIEnv<'_>,
    _object: JObject<'_>,
    id: jlong,
) {
    if let Ok(slot) = SCAN.lock()
        && let Some(scan) = slot.as_ref().filter(|scan| scan.matches(id))
    {
        scan.cancel.store(true, Ordering::Release);
        if let Ok(mut cache) = scan.cache.lock() {
            *cache = CandidateCache::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancel_joins_and_stale_generation_cannot_match() {
        let scan = Scan::spawn().unwrap();
        assert!(scan.matches(scan.id));
        assert!(!scan.matches(scan.id + 1));
        scan.cancel.store(true, Ordering::Release);
        assert!(!scan.matches(scan.id));
        scan.join.join().unwrap();
        assert_eq!(scan.status.load(Ordering::Acquire), 2);
        assert!(
            scan.cache
                .lock()
                .unwrap()
                .snapshot(Instant::now())
                .is_empty()
        );
        let next = Scan::spawn().unwrap();
        assert_ne!(scan.id, next.id);
        next.cancel.store(true, Ordering::Release);
        next.join.join().unwrap();
    }
}
