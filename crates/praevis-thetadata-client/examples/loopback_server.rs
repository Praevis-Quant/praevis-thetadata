//! A synthetic loopback service for the Python consumer examples and tests:
//! a fake login endpoint and a fake gRPC endpoint on 127.0.0.1. No
//! credentials and no ThetaData traffic.
//!
//! ```text
//! cargo run -p praevis-thetadata-client --example loopback_server -- partial
//! ```
//!
//! Scenarios:
//! - `partial`: every request streams two batches of two rows, then fails
//!   with `INTERNAL`;
//! - `slow`: every request streams 10 batches of three rows, 100 ms apart
//!   (about 1 s a request);
//! - `busy`: every request is refused with `RESOURCE_EXHAUSTED` at open;
//! - `delayed`: every request's first answer waits 1.5 s, then two rows.
//!
//! It prints one JSON line, `{"auth_url": …, "endpoint": …}`, then serves
//! until its standard input closes. A line `requests` on standard input
//! prints `{"requests": N}`: the stock EOD requests received so far.
#![allow(dead_code)] // The shared support module also serves the tests and benchmark.

use std::io::{BufRead, Write};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[path = "../tests/support/mod.rs"]
mod support;

fn script(scenario: &str) -> Option<support::Script> {
    match scenario {
        "partial" => {
            let (batch, _) = support::table(support::Shape::Prices, 2);
            Some(support::Script {
                messages: vec![
                    Ok(support::response(&batch, false)),
                    Ok(support::response(&batch, true)),
                    Err(tonic::Status::internal("synthetic mid-stream failure")),
                ],
                ..Default::default()
            })
        }
        "slow" => {
            let (batch, _) = support::table(support::Shape::Prices, 3);
            Some(support::Script {
                messages: vec![Ok(support::response(&batch, false)); 10],
                message_delay: Duration::from_millis(100),
                ..Default::default()
            })
        }
        "busy" => Some(support::Script {
            initial_error: Some(tonic::Status::resource_exhausted("synthetic busy")),
            ..Default::default()
        }),
        "delayed" => {
            let (batch, _) = support::table(support::Shape::Prices, 2);
            Some(support::Script {
                messages: vec![Ok(support::response(&batch, false))],
                header_delay: Duration::from_millis(1_500),
                ..Default::default()
            })
        }
        _ => None,
    }
}

/// Answers every login with a synthetic session for a FREE account.
async fn auth_server(listener: tokio::net::TcpListener) {
    while let Ok((mut socket, _)) = listener.accept().await {
        tokio::spawn(async move {
            let mut request = Vec::new();
            let mut buffer = [0; 2048];
            // Read the headers and the (small) body.
            loop {
                let Ok(count) = socket.read(&mut buffer).await else {
                    return;
                };
                if count == 0 {
                    return;
                }
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
                if request.len() > 16_384 {
                    return;
                }
            }
            let body = format!(
                r#"{{"sessionId":"{}","user":{{"email":"{}"}}}}"#,
                support::TOKEN,
                support::EMAIL
            );
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(reply.as_bytes()).await;
        });
    }
}

fn main() -> std::process::ExitCode {
    let scenario = std::env::args().nth(1).unwrap_or_default();
    let Some(script) = script(&scenario) else {
        eprintln!("usage: loopback_server partial|slow|busy|delayed");
        return std::process::ExitCode::FAILURE;
    };
    let fixture = support::Fixture::start(script);
    let (address_tx, address_rx) = std::sync::mpsc::channel();
    // The login endpoint, on its own thread and runtime; it ends with the process.
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            address_tx
                .send(listener.local_addr().expect("address"))
                .expect("main thread");
            auth_server(listener).await;
        });
    });
    let address = address_rx.recv().expect("login endpoint");
    println!(
        "{}",
        serde_json::json!({"auth_url": format!("http://{address}/auth"), "endpoint": fixture.endpoint})
    );
    std::io::stdout().flush().expect("stdout");
    // Serve until standard input closes (the parent process ends or closes it).
    for line in std::io::stdin().lock().lines() {
        if line.as_deref().is_ok_and(|l| l.trim() == "requests") {
            let count = fixture.requests.lock().map_or(0, |r| r.len());
            println!("{}", serde_json::json!({ "requests": count }));
            std::io::stdout().flush().expect("stdout");
        }
    }
    drop(fixture);
    std::process::ExitCode::SUCCESS
}
