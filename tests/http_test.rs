use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use rexporter::http::handle_connection;

fn one_shot_request(
    path_request: &str,
    responder: impl Fn() -> anyhow::Result<String> + Send + 'static,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        handle_connection(stream, &responder).unwrap();
    });

    let mut client = TcpStream::connect(addr).unwrap();
    client.write_all(path_request.as_bytes()).unwrap();
    client.flush().unwrap();

    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    handle.join().unwrap();
    response
}

#[test]
fn metrics_endpoint_returns_the_rendered_text() {
    let response = one_shot_request("GET /metrics HTTP/1.1\r\nHost: x\r\n\r\n", || {
        Ok("node_load1 0.5\n".to_string())
    });
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.contains("text/plain"));
    assert!(response.contains("node_load1 0.5"));
}

#[test]
fn unknown_path_returns_404() {
    let response = one_shot_request("GET /nonsense HTTP/1.1\r\n\r\n", || Ok(String::new()));
    assert!(response.starts_with("HTTP/1.1 404 Not Found"));
}

#[test]
fn collector_error_surfaces_as_500_not_a_crash() {
    let response = one_shot_request("GET /metrics HTTP/1.1\r\n\r\n", || {
        Err(anyhow::anyhow!("proc file missing"))
    });
    assert!(response.starts_with("HTTP/1.1 500"));
    assert!(response.contains("proc file missing"));
}

#[test]
fn root_path_serves_a_link_to_metrics() {
    let response = one_shot_request("GET / HTTP/1.1\r\n\r\n", || Ok(String::new()));
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.contains("/metrics"));
}
