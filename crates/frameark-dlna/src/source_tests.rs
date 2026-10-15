use super::*;
use std::sync::{Mutex, mpsc};

fn response(start: usize, end: usize, total: usize, etag: &str) -> Vec<u8> {
    let mut response = HttpResponse::empty(206, "Partial Content");
    response
        .headers
        .insert("content-type".into(), "video/mp4".into());
    response.headers.insert(
        "content-range".into(),
        format!("bytes {start}-{end}/{total}"),
    );
    response.headers.insert("etag".into(), etag.into());
    response.body = (start..=end).map(|index| (index % 251) as u8).collect();
    response.encode().unwrap()
}

type Observed = Arc<Mutex<Vec<HttpRequest>>>;

fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match listener.accept() {
            Ok((socket, _)) => {
                socket.set_nonblocking(false).unwrap();
                return socket;
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(1))
            }
            other => panic!("bounded fixture accept: {other:?}"),
        }
    }
}

fn origin(
    count: usize,
    reply: impl Fn(usize, &HttpRequest) -> Vec<u8> + Send + 'static,
) -> (SocketAddr, Observed, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let seen = observed.clone();
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        for index in 0..count {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    Err(error) => panic!("fixture accept: {error:?}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let request = read_http_request(&mut stream).unwrap();
            let bytes = reply(index, &request);
            seen.lock().unwrap().push(request);
            stream.write_all(&bytes).unwrap();
        }
    });
    (address, observed, worker)
}

fn open(address: SocketAddr) -> RemoteMediaSource {
    RemoteMediaSource::open(
        &format!(
            "http://fixture.invalid:{}/video?token=private",
            address.port()
        ),
        MediaUriNetworkPolicy::AllowAny,
        vec![address.ip()],
        None,
        Duration::from_secs(1),
    )
    .unwrap()
}

#[test]
fn random_reads_pin_addresses_retain_host_and_bound_cache() {
    let (address, observed, worker) = origin(3, |_, request| {
        let range = ByteRange::parse(request.header("range").unwrap(), 200_000).unwrap();
        response(range.start, range.end, 200_000, "\"original\"")
    });
    let source = open(address);
    assert_eq!(source.len(), 200_000);
    assert_eq!(source.mime_type(), "video/mp4");
    assert_eq!(source.read_at(0, 8).unwrap(), vec![0]); // cached probe may short-read
    let first = source.read_at(100, usize::MAX).unwrap();
    assert_eq!(first.len(), MAX_SOURCE_READ_BYTES);
    assert!(
        first
            .iter()
            .enumerate()
            .all(|(index, byte)| *byte == ((index + 100) % 251) as u8)
    );
    assert_eq!(source.read_at(120, 17).unwrap(), first[20..37]);
    assert_eq!(source.read_at(199_995, 100).unwrap().len(), 5);
    assert!(source.read_at(u64::MAX, 1).unwrap().is_empty());
    assert!(source.read_at(50, 0).unwrap().is_empty());
    worker.join().unwrap();
    let requests = observed.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .iter()
            .all(|request| request.target == "/video?token=private"
                && request.header("host")
                    == Some(format!("fixture.invalid:{}", address.port()).as_str()))
    );
    assert_eq!(requests[1].header("if-match"), Some("\"original\""));
    assert_eq!(source.report().cache_hits, 2);
    assert_eq!(
        source.report().received_bytes,
        1 + MAX_SOURCE_READ_BYTES as u64 + 5
    );
    assert!(!format!("{:?}", source.report()).contains("private"));
    source.close();
    source.close();
    assert!(source.read_at(0, 1).is_err());
}

#[test]
fn invalid_policy_and_options_fail_before_connecting() {
    for (uri, addresses, policy) in [
        (
            "http://127.0.0.1:9/video",
            vec!["127.0.0.1".parse().unwrap()],
            MediaUriNetworkPolicy::LocalNetworkOnly,
        ),
        (
            "http://192.168.1.2:9/video",
            vec!["192.168.1.3".parse().unwrap()],
            MediaUriNetworkPolicy::LocalNetworkOnly,
        ),
        (
            "http://fixture.invalid/video",
            vec!["192.168.1.2".parse().unwrap(), "8.8.8.8".parse().unwrap()],
            MediaUriNetworkPolicy::LocalNetworkOnly,
        ),
        (
            "http://user:password@fixture.invalid/video",
            vec!["127.0.0.1".parse().unwrap()],
            MediaUriNetworkPolicy::AllowAny,
        ),
        (
            "https://fixture.invalid/video",
            vec!["127.0.0.1".parse().unwrap()],
            MediaUriNetworkPolicy::AllowAny,
        ),
    ] {
        assert!(
            RemoteMediaSource::open(uri, policy, addresses, None, Duration::from_millis(50))
                .is_err()
        );
    }
    for timeout in [Duration::ZERO, Duration::from_secs(6)] {
        assert!(
            RemoteMediaSource::open(
                "http://127.0.0.1:9/",
                MediaUriNetworkPolicy::AllowAny,
                vec!["127.0.0.1".parse().unwrap()],
                None,
                timeout
            )
            .is_err()
        );
    }
}

#[test]
fn changing_resource_is_revoked_without_returning_new_bytes() {
    for changed in ["length", "etag", "mime"] {
        let (address, _, worker) = origin(2, move |index, request| {
            let range = ByteRange::parse(request.header("range").unwrap(), 100_000).unwrap();
            let bytes = response(
                range.start,
                range.end,
                if index == 1 && changed == "length" {
                    100_001
                } else {
                    100_000
                },
                if index == 1 && changed == "etag" {
                    "\"changed\""
                } else {
                    "\"original\""
                },
            );
            if index == 1 && changed == "mime" {
                let mut parsed = HttpResponse::parse(&bytes).unwrap();
                parsed
                    .headers
                    .insert("content-type".into(), "audio/mpeg".into());
                parsed.encode().unwrap()
            } else {
                bytes
            }
        });
        let source = open(address);
        assert!(source.read_at(1, 10).is_err());
        assert!(source.read_at(0, 1).is_err());
        worker.join().unwrap();
    }
}

#[test]
fn rejects_redirect_ignored_range_compression_unknown_and_oversized_lengths() {
    for variant in 0..6 {
        let (address, _, worker) = origin(1, move |_, _| {
            let mut value = HttpResponse::parse(&response(0, 0, 100, "\"original\"")).unwrap();
            match variant {
                0 => value.status = 302,
                1 => value.status = 200,
                2 => {
                    value
                        .headers
                        .insert("content-encoding".into(), "gzip".into());
                }
                3 => {
                    value
                        .headers
                        .insert("content-range".into(), "bytes 0-0/*".into());
                }
                4 => {
                    value.headers.insert(
                        "content-range".into(),
                        "bytes 0-0/18446744073709551615".into(),
                    );
                }
                _ => {
                    value
                        .headers
                        .insert("content-range".into(), "bytes 1-1/100".into());
                }
            }
            value.encode().unwrap()
        });
        assert!(
            RemoteMediaSource::open(
                &format!("http://{address}/"),
                MediaUriNetworkPolicy::AllowAny,
                vec![address.ip()],
                None,
                Duration::from_secs(1)
            )
            .is_err()
        );
        worker.join().unwrap();
    }
}

#[test]
fn close_interrupts_active_read_without_waiting_for_cache_lock() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (started, ready) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        read_http_request(&mut stream).unwrap();
        stream
            .write_all(&response(0, 0, 100_000, "\"original\""))
            .unwrap();
        drop(stream);
        let mut stream = accept(&listener);
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        read_http_request(&mut stream).unwrap();
        started.send(()).unwrap();
        match stream.read(&mut [0]) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ) => {}
            other => panic!("expected peer cancellation, got {other:?}"),
        }
    });
    let source = Arc::new(open(address));
    let reader = source.clone();
    let reading = std::thread::spawn(move || reader.read_at(1, 10));
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    let start = Instant::now();
    source.close();
    assert!(reading.join().unwrap().is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    worker.join().unwrap();
}

#[test]
fn slow_trickle_cannot_reset_the_total_request_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        read_http_request(&mut stream).unwrap();
        for byte in response(0, 0, 100, "\"original\"").into_iter().take(30) {
            if stream.write_all(&[byte]).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
    });
    let start = Instant::now();
    assert!(
        RemoteMediaSource::open(
            &format!("http://{address}/"),
            MediaUriNetworkPolicy::AllowAny,
            vec![address.ip()],
            None,
            Duration::from_millis(100)
        )
        .is_err()
    );
    assert!(start.elapsed() < Duration::from_millis(700));
    worker.join().unwrap();
}

#[test]
fn https_source_uses_explicit_roots_and_original_hostname() {
    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(certificate.key_pair.serialize_der());
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate.cert.der().clone()], key.into())
        .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certificate.cert.der().clone()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let socket = accept(&listener);
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut stream = rustls::StreamOwned::new(
            rustls::ServerConnection::new(Arc::new(config)).unwrap(),
            socket,
        );
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") && bytes.len() < MAX_HTTP_BYTES {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            bytes.push(byte[0]);
        }
        assert_eq!(stream.conn.server_name(), Some("localhost"));
        let request = HttpRequest::parse(&bytes).unwrap();
        assert_eq!(request.header("range"), Some("bytes=0-0"));
        stream.write_all(&response(0, 0, 1, "\"tls\"")).unwrap();
        stream.flush().unwrap();
    });
    let source = RemoteMediaSource::open(
        &format!("https://localhost:{}/", address.port()),
        MediaUriNetworkPolicy::AllowAny,
        vec![address.ip()],
        Some(Arc::new(roots)),
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(source.read_at(0, 1).unwrap(), vec![0]);
    worker.join().unwrap();
}
