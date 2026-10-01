use kalbion_core::icons::IconCache;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake-image";

fn serve(answer: fn(&str) -> (u16, &'static [u8])) -> (String, Arc<Mutex<Vec<String>>>) {
    serve_dropping(answer, 0)
}
/// Minimal HTTP server: answers by path and records every request line. The first
/// `drop_first` connections are closed without a response, like a stale keep-alive socket.
fn serve_dropping(
    answer: fn(&str) -> (u16, &'static [u8]),
    drop_first: usize,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    std::thread::spawn(move || {
        for (index, stream) in listener.incoming().enumerate() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
            }
            let path = request_line.split_whitespace().nth(1).unwrap().to_string();
            if index < drop_first {
                continue;
            }
            let (status, body) = answer(&path);
            log.lock().unwrap().push(path);
            write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        }
    });
    (url, requests)
}
fn unreachable_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}
fn cache(directory: &std::path::Path, url: &str) -> IconCache {
    IconCache::with_source(directory.into(), url.into(), false).unwrap()
}

#[test]
fn downloaded_icons_are_served_from_disk_when_the_service_is_gone() {
    let directory = tempfile::tempdir().unwrap();
    let (url, requests) = serve(|_| (200, PNG));
    let online = cache(directory.path(), &url);
    assert_eq!(
        online.get("T5_BAG@1", Some(3)).unwrap().as_deref(),
        Some(PNG)
    );
    assert_eq!(
        online.get("T5_BAG@1", Some(3)).unwrap().as_deref(),
        Some(PNG)
    );
    assert_eq!(
        *requests.lock().unwrap(),
        ["/T5_BAG%401.png?size=64&quality=3"]
    );
    let offline = cache(directory.path(), &unreachable_url());
    assert_eq!(
        offline.get("T5_BAG@1", Some(3)).unwrap().as_deref(),
        Some(PNG)
    );
    assert_eq!(offline.get("T5_BAG@1", Some(4)).unwrap(), None);
}

#[test]
fn unknown_items_are_remembered_across_restarts() {
    let directory = tempfile::tempdir().unwrap();
    let (url, requests) = serve(|_| (404, b"{}"));
    assert_eq!(
        cache(directory.path(), &url).get("T4_NOPE", None).unwrap(),
        None
    );
    assert_eq!(
        cache(directory.path(), &url).get("T4_NOPE", None).unwrap(),
        None
    );
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[test]
fn responses_that_are_not_png_are_never_cached() {
    let directory = tempfile::tempdir().unwrap();
    let (url, requests) = serve(|_| (200, b"<html>maintenance</html>"));
    let first = cache(directory.path(), &url);
    assert_eq!(first.get("T4_BAG", Some(1)).unwrap(), None);
    assert_eq!(first.get("T4_BAG", Some(1)).unwrap(), None);
    assert_eq!(
        requests.lock().unwrap().len(),
        1,
        "no retry storm right after a failure"
    );
    assert_eq!(
        cache(directory.path(), &url)
            .get("T4_BAG", Some(1))
            .unwrap(),
        None
    );
    assert_eq!(requests.lock().unwrap().len(), 2, "retried after restart");
    let cached_files = std::fs::read_dir(directory.path()).unwrap().count();
    assert_eq!(cached_files, 0);
}

#[test]
fn ids_that_could_escape_the_cache_are_rejected_before_any_request() {
    let directory = tempfile::tempdir().unwrap();
    let (url, requests) = serve(|_| (200, PNG));
    let icons = cache(directory.path(), &url);
    for id in ["../secret", "T4_BAG/../../x", "t4_bag", "T4_BAG.png", ""] {
        assert!(icons.get(id, None).is_err(), "{id}");
    }
    assert!(icons.get("T4_BAG", Some(6)).is_err());
    assert!(requests.lock().unwrap().is_empty());
}

#[test]
fn a_connection_dropped_mid_request_is_retried_once() {
    let directory = tempfile::tempdir().unwrap();
    let (url, requests) = serve_dropping(|_| (200, PNG), 1);
    let icons = cache(directory.path(), &url);
    assert_eq!(icons.get("T4_BAG", Some(1)).unwrap().as_deref(), Some(PNG));
    assert_eq!(requests.lock().unwrap().len(), 1);
}
