use super::*;

enum Server {
    Raop(RaopRtspTcpServer),
    Mirror(MirrorRtspTcpServer),
}

impl Server {
    fn new(mirror: bool) -> Self {
        let address = "127.0.0.1:0".parse().unwrap();
        let timeout = Duration::from_secs(1);
        if mirror {
            Self::Mirror(
                MirrorRtspTcpServer::bind(
                    address,
                    MirrorSession::new("fixture", 7010).unwrap(),
                    timeout,
                )
                .unwrap(),
            )
        } else {
            Self::Raop(
                RaopRtspTcpServer::bind(address, RaopSession::new(6000).unwrap(), timeout).unwrap(),
            )
        }
    }

    fn listener(&self) -> &std::net::TcpListener {
        match self {
            Self::Raop(s) => &s.listener,
            Self::Mirror(s) => &s.listener,
        }
    }

    fn serve(
        &mut self,
        stream: TcpStream,
        peer: SocketAddr,
        timeout: Duration,
    ) -> Result<SocketAddr, AirplayError> {
        match self {
            Self::Raop(s) => s.serve_connection(stream, peer, Instant::now() + timeout),
            Self::Mirror(s) => s.serve_connection(stream, peer, Instant::now() + timeout),
        }
    }

    fn assert_idle(&self) {
        match self {
            Self::Raop(s) => assert_eq!(s.state(), RaopSessionState::Idle),
            Self::Mirror(s) => assert_eq!(s.state(), MirrorSessionState::Idle),
        }
    }

    fn run(&mut self, duration: Duration, max_requests: usize) -> Result<usize, AirplayError> {
        match self {
            Self::Raop(s) => s.serve_for(duration, max_requests),
            Self::Mirror(s) => s.serve_for(duration, max_requests),
        }
    }
}

fn delayed_split(mirror: bool) {
    let mut server = Server::new(mirror);
    let mut client = TcpStream::connect(server.listener().local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let (stream, peer) = server.listener().accept().unwrap();
    // Model Windows accept inheritance explicitly, so this reproducer also runs on Unix.
    stream.set_nonblocking(true).unwrap();
    let sender = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        for part in [
            &b"OPTIONS rtsp://receiver/stream RTSP/1.0\r\nCSe"[..],
            &b"q: 1\r\nContent-Length: 4\r\n\r\n"[..],
            &b"test"[..],
        ] {
            std::thread::sleep(Duration::from_millis(40));
            client.write_all(part)?;
        }
        let mut response = Vec::new();
        client.read_to_end(&mut response)?;
        Ok(response)
    });
    let result = server.serve(stream, peer, Duration::from_secs(1));
    let response = sender.join().unwrap();
    assert_eq!(result, Ok(peer));
    assert!(response.unwrap().starts_with(b"RTSP/1.0 200 OK\r\n"));
    server.assert_idle();
}

#[test]
fn raop_nonblocking_accept_waits_for_delayed_split_request() {
    delayed_split(false);
}

#[test]
fn mirror_nonblocking_accept_waits_for_delayed_split_request() {
    delayed_split(true);
}

#[test]
fn slow_body_cannot_renew_the_connection_deadline() {
    for mirror in [false, true] {
        let mut server = Server::new(mirror);
        let mut client = TcpStream::connect(server.listener().local_addr().unwrap()).unwrap();
        client
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let (stream, peer) = server.listener().accept().unwrap();
        stream.set_nonblocking(false).unwrap();
        client
            .write_all(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nContent-Length: 8\r\n\r\n")
            .unwrap();
        let sender = std::thread::spawn(move || {
            for _ in 0..8 {
                std::thread::sleep(Duration::from_millis(35));
                if client.write_all(b"x").is_err() {
                    break;
                }
            }
        });
        let start = Instant::now();
        let result = server.serve(stream, peer, Duration::from_millis(80));
        let elapsed = start.elapsed();
        sender.join().unwrap();
        assert_eq!(result, Err(AirplayError::Io(ErrorKind::TimedOut)));
        assert!(
            elapsed < Duration::from_secs(1),
            "deadline elapsed: {elapsed:?}"
        );
        server.assert_idle();
    }
}

#[test]
fn runtime_expires_partial_connection_then_serves_a_healthy_client() {
    for mirror in [false, true] {
        let mut server = Server::new(mirror);
        let address = server.listener().local_addr().unwrap();
        let mut stalled = TcpStream::connect(address).unwrap();
        stalled
            .write_all(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nContent-Length: 8\r\n\r\nx")
            .unwrap();
        let start = Instant::now();
        assert_eq!(server.run(Duration::from_millis(50), 2), Ok(1));
        assert!(start.elapsed() < Duration::from_secs(1));
        server.assert_idle();
        drop(stalled);
        let client = std::thread::spawn(move || {
            let mut socket = TcpStream::connect(address).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket
                .write_all(b"OPTIONS * RTSP/1.0\r\nCSeq: 2\r\n\r\n")
                .unwrap();
            let mut response = Vec::new();
            socket.read_to_end(&mut response).unwrap();
            response
        });
        assert_eq!(server.run(Duration::from_secs(1), 1), Ok(1));
        assert!(client.join().unwrap().starts_with(b"RTSP/1.0 200 OK\r\n"));
        // A restored blocking listener must wait for this later connection.
        let client = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            TcpStream::connect(address).unwrap()
        });
        let rejected = match &mut server {
            Server::Raop(s) => s.serve_once_with_peer_policy(|_| false),
            Server::Mirror(s) => s.serve_once_with_peer_policy(|_| false),
        };
        client.join().unwrap();
        assert!(matches!(rejected, Err(AirplayError::Invalid(_))));
        server.assert_idle();
    }
}

#[test]
fn connection_configuration_rejects_zero_and_over_limit_budgets() {
    for timeout in [
        Duration::ZERO,
        MAX_RTSP_CONNECTION_DURATION + Duration::from_nanos(1),
        Duration::MAX,
    ] {
        assert!(
            RaopRtspTcpServer::bind(
                "127.0.0.1:0".parse().unwrap(),
                RaopSession::new(6000).unwrap(),
                timeout
            )
            .is_err()
        );
        assert!(
            MirrorRtspTcpServer::bind(
                "127.0.0.1:0".parse().unwrap(),
                MirrorSession::new("fixture", 7010).unwrap(),
                timeout
            )
            .is_err()
        );
    }
    assert!(
        RaopRtspTcpServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            RaopSession::new(6000).unwrap(),
            MAX_RTSP_CONNECTION_DURATION
        )
        .is_ok()
    );
    assert!(
        MirrorRtspTcpServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            MirrorSession::new("fixture", 7010).unwrap(),
            MAX_RTSP_CONNECTION_DURATION
        )
        .is_ok()
    );
}

#[test]
fn malformed_and_truncated_network_frames_do_not_mutate_either_session() {
    let payloads = [
        format!(
            "OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nContent-Length: {}\r\n\r\n",
            usize::MAX
        )
        .into_bytes(),
        b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nContent-Length: 4\r\ncontent-length: 4\r\n\r\n".to_vec(),
        b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nContent-Length: nope\r\n\r\n".to_vec(),
        b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nContent-Length: 4\r\n\r\nx".to_vec(),
        vec![b'x'; MAX_RTSP_BYTES + 1],
    ];
    for mirror in [false, true] {
        for payload in &payloads {
            let mut server = Server::new(mirror);
            let mut client = TcpStream::connect(server.listener().local_addr().unwrap()).unwrap();
            client
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let (stream, peer) = server.listener().accept().unwrap();
            let payload = payload.clone();
            let sender = std::thread::spawn(move || {
                let _ = client.write_all(&payload);
                let _ = client.shutdown(std::net::Shutdown::Write);
            });
            let error = server
                .serve(stream, peer, Duration::from_secs(1))
                .unwrap_err();
            sender.join().unwrap();
            assert!(
                matches!(error, AirplayError::Invalid(_) | AirplayError::TooLarge(_)),
                "{error:?}"
            );
            server.assert_idle();
        }
    }
}

#[test]
fn exact_maximum_message_and_body_are_accepted_over_tcp() {
    let mut head =
        format!("OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nContent-Length: {MAX_RTSP_BODY_BYTES}");
    let target = MAX_RTSP_BYTES - 4 - MAX_RTSP_BODY_BYTES;
    let mut index = 0;
    while head.len() < target {
        let prefix = format!("\r\nX-{index}: ");
        let remaining = target - head.len() - prefix.len();
        assert!(remaining > 0);
        head.push_str(&prefix);
        head.push_str(&"x".repeat(remaining.min(2048)));
        index += 1;
    }
    let mut message = head.into_bytes();
    message.extend_from_slice(b"\r\n\r\n");
    message.resize(MAX_RTSP_BYTES, b'x');
    assert_eq!(
        RtspMessage::parse(&message).unwrap().body.len(),
        MAX_RTSP_BODY_BYTES
    );
    for mirror in [false, true] {
        let mut server = Server::new(mirror);
        let mut client = TcpStream::connect(server.listener().local_addr().unwrap()).unwrap();
        client
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let (stream, peer) = server.listener().accept().unwrap();
        let bytes = message.clone();
        let sender = std::thread::spawn(move || {
            client.write_all(&bytes).unwrap();
            let mut response = Vec::new();
            client.read_to_end(&mut response).unwrap();
            response
        });
        let result = server.serve(stream, peer, Duration::from_secs(1));
        let response = sender.join().unwrap();
        assert_eq!(result, Ok(peer));
        assert!(response.starts_with(b"RTSP/1.0 200 OK\r\n"));
    }
}
