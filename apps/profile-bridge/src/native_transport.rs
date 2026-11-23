#![forbid(unsafe_code)]
use bridge_domain::BridgePortError;
use std::{
    error::Error,
    io,
    sync::OnceLock,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum HttpFailure {
    #[error("DNS")]
    Dns,
    #[error("CONNECT")]
    Connect,
    #[error("TLS")]
    Tls,
    #[error("TIMEOUT")]
    Timeout,
    #[error("REDIRECT")]
    Redirect,
    #[error("RESPONSE_SIZE")]
    Size,
    #[error("LOCAL_IO")]
    Disk,
    #[error("CONNECTION_RESET")]
    ConnectionReset,
    #[error("CONNECTION_ABORTED")]
    ConnectionAborted,
    #[error("BROKEN_PIPE")]
    BrokenPipe,
    #[error("UNEXPECTED_EOF")]
    UnexpectedEof,
    #[error("UNCLASSIFIED_NETWORK")]
    Network,
}
impl From<HttpFailure> for BridgePortError {
    fn from(value: HttpFailure) -> Self {
        match value {
            HttpFailure::Size | HttpFailure::Redirect => Self::InvalidResponse,
            _ => Self::Unavailable,
        }
    }
}
fn classify_io(error: &io::Error) -> Option<HttpFailure> {
    // WinSock codes are reliable only when the OS actually exposes them.
    // Never infer a peer reset from an unspecified reqwest/hyper send failure.
    match error.raw_os_error() {
        Some(11001..=11004) => return Some(HttpFailure::Dns),
        Some(-2146762481 | -2146762495) => return Some(HttpFailure::Tls),
        Some(10054) => return Some(HttpFailure::ConnectionReset),
        Some(10053) => return Some(HttpFailure::ConnectionAborted),
        _ => {}
    }
    match error.kind() {
        io::ErrorKind::ConnectionReset => Some(HttpFailure::ConnectionReset),
        io::ErrorKind::ConnectionAborted => Some(HttpFailure::ConnectionAborted),
        io::ErrorKind::BrokenPipe => Some(HttpFailure::BrokenPipe),
        io::ErrorKind::UnexpectedEof => Some(HttpFailure::UnexpectedEof),
        _ => None,
    }
}

fn classify_source_chain(root: &(dyn Error + 'static)) -> Option<HttpFailure> {
    let mut source: Option<&(dyn Error + 'static)> = Some(root);
    while let Some(value) = source {
        if value.downcast_ref::<native_tls::Error>().is_some() {
            return Some(HttpFailure::Tls);
        }
        if let Some(io) = value.downcast_ref::<io::Error>() {
            if let Some(kind) = classify_io(io) {
                return Some(kind);
            }
        }
        source = value.source();
    }
    None
}

fn classify(error: &reqwest::Error) -> HttpFailure {
    if error.is_timeout() {
        return HttpFailure::Timeout;
    }
    if error.is_redirect() {
        return HttpFailure::Redirect;
    }
    if let Some(kind) = classify_source_chain(error) {
        return kind;
    }
    if error.is_connect() {
        HttpFailure::Connect
    } else {
        HttpFailure::Network
    }
}

pub(crate) use crate::user_diagnostics::report::TransportOperation as HttpOperation;
pub(crate) struct NativeTransport {
    pub(crate) runtime: tokio::runtime::Runtime,
    pub(crate) client: reqwest::Client,
    delivery: OnceLock<Result<reqwest::Client, HttpFailure>>,
}
static TRANSPORT: OnceLock<Result<NativeTransport, HttpFailure>> = OnceLock::new();
fn base_client() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .tls_backend_native()
        .https_only(true)
        .connect_timeout(Duration::from_secs(10))
}
pub(crate) fn shared() -> Result<&'static NativeTransport, HttpFailure> {
    TRANSPORT
        .get_or_init(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .map_err(|_| HttpFailure::Network)?;
            let client = runtime
                .block_on(async {
                    base_client()
                        .no_proxy()
                        .redirect(reqwest::redirect::Policy::none())
                        .build()
                })
                .map_err(|e| classify(&e))?;
            Ok(NativeTransport {
                runtime,
                client,
                delivery: OnceLock::new(),
            })
        })
        .as_ref()
        .map_err(|e| *e)
}
impl NativeTransport {
    pub(crate) fn delivery_client(&self) -> Result<&reqwest::Client, HttpFailure> {
        self.delivery
            .get_or_init(|| {
                self.runtime.block_on(async {
                    base_client()
                        .redirect(reqwest::redirect::Policy::custom(|attempt| {
                            if attempt.url().scheme() != "https" || attempt.previous().len() > 5 {
                                attempt.error("delivery redirect refused")
                            } else {
                                attempt.follow()
                            }
                        }))
                        .build()
                        .map_err(|e| classify(&e))
                })
            })
            .as_ref()
            .map_err(|e| *e)
    }
}
pub(crate) fn note_failure(
    operation: HttpOperation,
    correlation: Option<&str>,
    category: HttpFailure,
    status: Option<u16>,
    bytes: Option<u64>,
    started: Instant,
) -> HttpFailure {
    use crate::user_diagnostics::report::{self, Cause};
    let cause = match category {
        HttpFailure::Dns => Cause::NetworkDns,
        HttpFailure::Connect => Cause::NetworkConnect,
        HttpFailure::Tls => Cause::NetworkTls,
        HttpFailure::Timeout => Cause::NetworkTimeout,
        HttpFailure::Redirect => Cause::NetworkRedirect,
        HttpFailure::Size => Cause::NetworkResponseSize,
        HttpFailure::Disk => Cause::NetworkLocalIo,
        HttpFailure::ConnectionReset => Cause::NetworkConnectionReset,
        HttpFailure::ConnectionAborted => Cause::NetworkConnectionAborted,
        HttpFailure::BrokenPipe => Cause::NetworkBrokenPipe,
        HttpFailure::UnexpectedEof => Cause::NetworkUnexpectedEof,
        HttpFailure::Network => Cause::NetworkUnclassified,
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;
    tracing::warn!(target:"profile_bridge_transport", ?operation, ?category, correlation_id=correlation.unwrap_or(""), ?status, ?bytes, elapsed_ms);
    report::record_transport_failure(operation, correlation, status, bytes, cause, elapsed_ms);
    category
}
pub(crate) fn failure(
    operation: HttpOperation,
    correlation: Option<&str>,
    error: &reqwest::Error,
    started: Instant,
) -> HttpFailure {
    note_failure(operation, correlation, classify(error), None, None, started)
}
pub(crate) fn completed(
    operation: HttpOperation,
    correlation: Option<&str>,
    status: u16,
    bytes: u64,
    started: Instant,
) {
    let elapsed_ms = started.elapsed().as_millis() as u64;
    tracing::info!(target:"profile_bridge_transport", ?operation, correlation_id=correlation.unwrap_or(""), status, bytes, elapsed_ms);
    if !(200..300).contains(&status) {
        use crate::user_diagnostics::report::{self, Cause};
        report::record_transport_failure(
            operation,
            correlation,
            Some(status),
            Some(bytes),
            Cause::NetworkHttp,
            elapsed_ms,
        );
    }
}
pub(crate) fn buffered(
    request: reqwest::RequestBuilder,
    max: usize,
    operation: HttpOperation,
    correlation: Option<&str>,
) -> Result<(u16, Vec<u8>), HttpFailure> {
    let transport = shared()?;
    let started = Instant::now();
    transport.runtime.block_on(async {
        let mut response = request
            .send()
            .await
            .map_err(|e| failure(operation, correlation, &e, started))?;
        let status = response.status().as_u16();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| failure(operation, correlation, &e, started))?
        {
            if bytes.len().saturating_add(chunk.len()) > max {
                return Err(note_failure(
                    operation,
                    correlation,
                    HttpFailure::Size,
                    Some(status),
                    Some(bytes.len() as u64),
                    started,
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        completed(operation, correlation, status, bytes.len() as u64, started);
        Ok((status, bytes))
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precise_native_disconnect_requires_a_real_io_cause_in_the_source_chain() {
        let samples = [
            (io::ErrorKind::ConnectionReset, HttpFailure::ConnectionReset),
            (
                io::ErrorKind::ConnectionAborted,
                HttpFailure::ConnectionAborted,
            ),
            (io::ErrorKind::BrokenPipe, HttpFailure::BrokenPipe),
            (io::ErrorKind::UnexpectedEof, HttpFailure::UnexpectedEof),
        ];
        for (kind, expected) in samples {
            let wrapped = io::Error::other(io::Error::from(kind));
            assert_eq!(classify_source_chain(&wrapped), Some(expected));
            assert_eq!(
                BridgePortError::from(expected),
                BridgePortError::Unavailable
            );
        }
        assert_eq!(
            classify_io(&io::Error::from_raw_os_error(10054)),
            Some(HttpFailure::ConnectionReset)
        );
        assert_eq!(
            classify_io(&io::Error::from_raw_os_error(10053)),
            Some(HttpFailure::ConnectionAborted)
        );
        assert_eq!(classify_source_chain(&io::Error::other("opaque")), None);
        assert_eq!(classify_io(&io::Error::from(io::ErrorKind::Other)), None);
    }

    #[test]
    fn transport_is_shared_and_delivery_policy_is_separate() -> Result<(), HttpFailure> {
        let a = shared()?;
        assert!(std::ptr::eq(a, shared()?));
        assert!(std::ptr::eq(a.delivery_client()?, a.delivery_client()?));
        Ok(())
    }
    #[test]
    #[ignore = "requires isolated process HTTPS_PROXY and live staging HTTPS"]
    fn delivery_proxy_does_not_change_direct_transport() -> Result<(), Box<dyn Error>> {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        let port = std::env::var("PB_TEST_PROXY_PORT")?.parse::<u16>()?;
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true)?;
        let peer = std::thread::spawn(move || -> io::Result<String> {
            let deadline = Instant::now() + Duration::from_secs(25);
            loop {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket.set_read_timeout(Some(Duration::from_secs(3)))?;
                        socket.set_write_timeout(Some(Duration::from_secs(3)))?;
                        let mut reader = BufReader::new(&socket);
                        let mut first = String::new();
                        reader.read_line(&mut first)?;
                        drop(reader);
                        socket.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
                        return Ok(first);
                    }
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(e) => return Err(e),
                }
            }
        });
        let transport = shared()?;
        let direct = transport
            .runtime
            .block_on(async {
                transport
                    .client
                    .get("https://staging.alegria.by/")
                    .timeout(Duration::from_secs(12))
                    .send()
                    .await
            })
            .map_err(|e| classify(&e))?;
        assert!(direct.status().is_redirection() || direct.status().is_success());
        drop(direct);
        let client = transport.delivery_client()?;
        let delivery = transport.runtime.block_on(async {
            client
                .get("https://staging.alegria.by/")
                .timeout(Duration::from_secs(12))
                .send()
                .await
        });
        let connect = peer.join().map_err(|_| "proxy test fixture failed")??;
        assert_eq!(
            connect.trim_end(),
            "CONNECT staging.alegria.by:443 HTTP/1.1"
        );
        assert!(delivery.is_err() || delivery.is_ok_and(|v| v.status().as_u16() == 407));
        eprintln!("DELIVERY_PROXY_CONNECT_OBSERVED_DIRECT_BYPASS_PASS");
        Ok(())
    }
}
