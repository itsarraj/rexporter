use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

/// A deliberately minimal HTTP/1.0-ish responder: reads the request line,
/// ignores every header, serves exactly two routes. Real HTTP compliance
/// (keep-alive, chunked encoding, HEAD, ...) is out of scope on purpose —
/// this only ever needs to answer a Prometheus scraper's `GET /metrics`,
/// and staying this small is the actual "lightweight" pitch: no async
/// runtime, no web framework, just `std::net`.
pub fn serve<F>(addr: &str, metrics_fn: F) -> anyhow::Result<()>
where
    F: Fn() -> anyhow::Result<String> + Send + Sync + 'static,
{
    let listener = TcpListener::bind(addr)?;
    let metrics_fn = Arc::new(metrics_fn);
    for stream in listener.incoming() {
        let stream = stream?;
        let metrics_fn = Arc::clone(&metrics_fn);
        thread::spawn(move || {
            let _ = handle_connection(stream, metrics_fn.as_ref());
        });
    }
    Ok(())
}

/// Runs the accept loop for exactly one connection (used by the listener
/// above) or can be called directly against a `TcpListener` bound to a
/// test-chosen port — factored out so tests don't need the infinite
/// `serve` loop just to exercise one request.
pub fn handle_connection<F>(mut stream: TcpStream, metrics_fn: &F) -> std::io::Result<()>
where
    F: Fn() -> anyhow::Result<String>,
{
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf)?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string();

    let (status, content_type, body) = match path.as_str() {
        "/metrics" => match metrics_fn() {
            Ok(text) => ("200 OK", "text/plain; version=0.0.4", text),
            Err(e) => (
                "500 Internal Server Error",
                "text/plain",
                format!("error: {e}"),
            ),
        },
        "/" => (
            "200 OK",
            "text/html",
            "<html><body><a href=\"/metrics\">/metrics</a></body></html>".to_string(),
        ),
        _ => ("404 Not Found", "text/plain", "not found".to_string()),
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}
