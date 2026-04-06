use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

type CallbackServerHandle = (Arc<AtomicBool>, thread::JoinHandle<()>);

static CALLBACK_SERVERS: Lazy<Arc<Mutex<HashMap<u16, CallbackServerHandle>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

fn callback_path_for(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "/anthropic/callback",
        "codex" => "/codex/callback",
        "google" => "/google/callback",
        "antigravity" => "/antigravity/callback",
        _ => "/callback",
    }
}

fn build_redirect_url(
    mode: &str,
    provider: &str,
    base_url: Option<String>,
    local_port: Option<u16>,
    query: &str,
) -> String {
    let callback_path = callback_path_for(provider);
    let base = if mode == "local" {
        let port = local_port.unwrap_or(8317);
        format!("http://127.0.0.1:{}{}", port, callback_path)
    } else {
        let base_url = base_url.unwrap_or_else(|| "http://127.0.0.1:8317".to_string());
        if base_url.ends_with('/') {
            format!("{}{}", base_url, callback_path.trim_start_matches('/'))
        } else {
            format!("{}/{}", base_url, callback_path.trim_start_matches('/'))
        }
    };

    if query.is_empty() {
        base
    } else {
        format!("{}?{}", base, query)
    }
}

fn run_callback_server(
    stop: Arc<AtomicBool>,
    listen_port: u16,
    mode: String,
    provider: String,
    base_url: Option<String>,
    local_port: Option<u16>,
) {
    let addr = format!("127.0.0.1:{}", listen_port);
    let listener = match std::net::TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("[CALLBACK] failed to bind {}: {}", addr, error);
            return;
        }
    };
    if let Err(error) = listener.set_nonblocking(false) {
        eprintln!("[CALLBACK] set_nonblocking failed: {}", error);
    }
    println!("[CALLBACK] listening on {} for provider {}", addr, provider);

    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut reader = match stream.try_clone() {
                    Ok(clone) => BufReader::new(clone),
                    Err(error) => {
                        eprintln!("[CALLBACK] stream clone failed: {}", error);
                        continue;
                    }
                };
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_ok() {
                    let path_and_query = request_line.split_whitespace().nth(1).unwrap_or("/");
                    let query = path_and_query.split_once('?').map(|(_, query)| query).unwrap_or("");
                    let location =
                        build_redirect_url(&mode, &provider, base_url.clone(), local_port, query);
                    let response = format!(
                        "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        location
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
            Err(error) => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                eprintln!("[CALLBACK] accept error: {}", error);
                thread::sleep(Duration::from_millis(50));
            }
        }
    }

    println!("[CALLBACK] server on {} stopped", addr);
}

#[tauri::command]
pub(crate) fn start_callback_server(
    provider: String,
    listen_port: u16,
    mode: String,
    base_url: Option<String>,
    local_port: Option<u16>,
) -> Result<Value, String> {
    let mut map = CALLBACK_SERVERS.lock();
    if let Some((flag, handle)) = map.remove(&listen_port) {
        flag.store(true, Ordering::SeqCst);
        let _ = std::net::TcpStream::connect(("127.0.0.1", listen_port));
        let _ = handle.join();
    }

    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        run_callback_server(
            stop_clone,
            listen_port,
            mode,
            provider,
            base_url,
            local_port,
        )
    });

    map.insert(listen_port, (stop, handle));
    Ok(json!({"success": true}))
}

#[tauri::command]
pub(crate) fn stop_callback_server(listen_port: u16) -> Result<Value, String> {
    let server = CALLBACK_SERVERS.lock().remove(&listen_port);
    if let Some((flag, handle)) = server {
        flag.store(true, Ordering::SeqCst);
        let _ = std::net::TcpStream::connect(("127.0.0.1", listen_port));
        thread::spawn(move || {
            let _ = handle.join();
        });
        Ok(json!({"success": true}))
    } else {
        Ok(json!({"success": false, "error": "not running"}))
    }
}

#[cfg(test)]
mod tests {
    use super::{build_redirect_url, callback_path_for};

    #[test]
    fn callback_path_matches_known_provider_routes() {
        assert_eq!(callback_path_for("codex"), "/codex/callback");
        assert_eq!(callback_path_for("google"), "/google/callback");
        assert_eq!(callback_path_for("unknown"), "/callback");
    }

    #[test]
    fn local_redirect_url_uses_loopback_port() {
        let redirect =
            build_redirect_url("local", "codex", None, Some(8765), "code=demo&state=abc");

        assert_eq!(
            redirect,
            "http://127.0.0.1:8765/codex/callback?code=demo&state=abc"
        );
    }

    #[test]
    fn remote_redirect_url_keeps_single_slash() {
        let redirect = build_redirect_url(
            "remote",
            "anthropic",
            Some("https://example.com/base/".to_string()),
            None,
            "code=demo",
        );

        assert_eq!(
            redirect,
            "https://example.com/base/anthropic/callback?code=demo"
        );
    }
}
