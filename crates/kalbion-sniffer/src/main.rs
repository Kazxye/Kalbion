//! Kalbion capture helper. This is the only Kalbion binary that needs capture rights
//! (`cap_net_raw` on Linux, Npcap on Windows), so it does as little as possible:
//!
//! - `kalbion-sniffer list` prints the capture interfaces as a JSON array.
//! - `kalbion-sniffer capture <interface>` opens the interface without promiscuous mode,
//!   applies a fixed BPF filter (UDP from the Albion game port) and writes the packets to
//!   stdout as a classic PCAP stream (microseconds, little endian).
//!
//! It never parses packet contents, takes no filter or path from its caller and writes
//! nothing to disk; decoding happens in the unprivileged app. It exits when its stdin or
//! stdout closes, so it cannot outlive the app that started it, even with no traffic.
//! Status goes to stderr as JSON lines.
use serde_json::json;
use std::io::{BufWriter, Read, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// Fixed by design: the caller cannot widen what is captured.
const FILTER: &str = "udp src port 5056";
const SNAPLEN: i32 = 65_535;
const READ_TIMEOUT_MS: i32 = 500;
const STATS_EVERY: Duration = Duration::from_secs(30);
const MAX_INTERFACE_NAME: usize = 256;

const EXIT_USAGE: u8 = 2;
const EXIT_PERMISSION: u8 = 3;
const EXIT_NO_INTERFACE: u8 = 4;
const EXIT_CAPTURE: u8 = 5;

fn log(level: &str, operation: &str, fields: serde_json::Value) {
    let mut record = json!({ "level": level, "operation": operation });
    if let (Some(record), Some(fields)) = (record.as_object_mut(), fields.as_object()) {
        record.extend(fields.clone());
    }
    let _ = writeln!(std::io::stderr(), "{record}");
}

fn exit_code(error: &pcap::Error) -> u8 {
    let text = error.to_string().to_lowercase();
    if text.contains("permission") || text.contains("not permitted") {
        EXIT_PERMISSION
    } else {
        EXIT_CAPTURE
    }
}

fn list() -> ExitCode {
    let devices = match pcap::Device::list() {
        Ok(devices) => devices,
        Err(error) => {
            log(
                "error",
                "list_failed",
                json!({ "cause": error.to_string() }),
            );
            return ExitCode::from(exit_code(&error));
        }
    };
    let interfaces: Vec<_> = devices
        .iter()
        .map(|device| {
            json!({
                "name": device.name,
                "description": device.desc,
                "addresses": device
                    .addresses
                    .iter()
                    .map(|address| address.addr.to_string())
                    .collect::<Vec<_>>(),
                "loopback": device.flags.is_loopback(),
                "up": device.flags.is_up(),
                "running": device.flags.is_running(),
            })
        })
        .collect();
    println!("{}", serde_json::Value::Array(interfaces));
    ExitCode::SUCCESS
}

fn pcap_header(link_type: u32) -> [u8; 24] {
    let mut header = [0u8; 24];
    header[0..4].copy_from_slice(&0xA1B2_C3D4u32.to_le_bytes());
    header[4..6].copy_from_slice(&2u16.to_le_bytes());
    header[6..8].copy_from_slice(&4u16.to_le_bytes());
    header[16..20].copy_from_slice(&(SNAPLEN as u32).to_le_bytes());
    header[20..24].copy_from_slice(&link_type.to_le_bytes());
    header
}

/// The app keeps our stdin open while it wants packets; end of input means it is gone.
fn exit_when_stdin_closes() {
    std::thread::spawn(|| {
        let mut buffer = [0u8; 64];
        let mut stdin = std::io::stdin();
        loop {
            match stdin.read(&mut buffer) {
                Ok(0) | Err(_) => {
                    log(
                        "info",
                        "capture_stopped",
                        json!({ "reason": "stdin_closed" }),
                    );
                    std::process::exit(0);
                }
                Ok(_) => {}
            }
        }
    });
}

fn capture(interface: &str) -> ExitCode {
    if interface.is_empty()
        || interface.len() > MAX_INTERFACE_NAME
        || interface.chars().any(char::is_control)
    {
        log("error", "invalid_interface", json!({}));
        return ExitCode::from(EXIT_USAGE);
    }
    let known = match pcap::Device::list() {
        Ok(devices) => devices.into_iter().any(|device| device.name == interface),
        Err(error) => {
            log(
                "error",
                "list_failed",
                json!({ "cause": error.to_string() }),
            );
            return ExitCode::from(exit_code(&error));
        }
    };
    if !known {
        log(
            "error",
            "interface_not_found",
            json!({ "interface": interface }),
        );
        return ExitCode::from(EXIT_NO_INTERFACE);
    }
    let opened = pcap::Capture::from_device(interface).and_then(|inactive| {
        inactive
            .promisc(false)
            .snaplen(SNAPLEN)
            .immediate_mode(true)
            .timeout(READ_TIMEOUT_MS)
            .open()
    });
    let mut handle = match opened {
        Ok(handle) => handle,
        Err(error) => {
            log(
                "error",
                "open_failed",
                json!({ "interface": interface, "cause": error.to_string() }),
            );
            return ExitCode::from(exit_code(&error));
        }
    };
    if let Err(error) = handle.filter(FILTER, true) {
        log(
            "error",
            "filter_failed",
            json!({ "cause": error.to_string() }),
        );
        return ExitCode::from(EXIT_CAPTURE);
    }
    exit_when_stdin_closes();
    let link_type = handle.get_datalink().0 as u32;
    let mut output = BufWriter::new(std::io::stdout().lock());
    if output
        .write_all(&pcap_header(link_type))
        .and_then(|()| output.flush())
        .is_err()
    {
        return ExitCode::SUCCESS;
    }
    log(
        "info",
        "capture_started",
        json!({ "interface": interface, "link_type": link_type, "filter": FILTER }),
    );
    let mut packets: u64 = 0;
    let mut last_stats = Instant::now();
    loop {
        match handle.next_packet() {
            Ok(packet) => {
                packets += 1;
                let header = packet.header;
                let mut record = [0u8; 16];
                record[0..4].copy_from_slice(&(header.ts.tv_sec as u32).to_le_bytes());
                record[4..8].copy_from_slice(&(header.ts.tv_usec as u32).to_le_bytes());
                record[8..12].copy_from_slice(&header.caplen.to_le_bytes());
                record[12..16].copy_from_slice(&header.len.to_le_bytes());
                let written = output
                    .write_all(&record)
                    .and_then(|()| output.write_all(packet.data))
                    .and_then(|()| output.flush());
                if written.is_err() {
                    // The app closed the pipe: stop quietly.
                    log("info", "capture_stopped", json!({ "packets": packets }));
                    return ExitCode::SUCCESS;
                }
            }
            Err(pcap::Error::TimeoutExpired) => {}
            Err(error) => {
                log(
                    "error",
                    "capture_failed",
                    json!({ "cause": error.to_string(), "packets": packets }),
                );
                return ExitCode::from(EXIT_CAPTURE);
            }
        }
        if last_stats.elapsed() >= STATS_EVERY {
            last_stats = Instant::now();
            if let Ok(stats) = handle.stats() {
                log(
                    "info",
                    "capture_stats",
                    json!({
                        "packets": packets,
                        "received": stats.received,
                        "dropped": stats.dropped,
                        "if_dropped": stats.if_dropped,
                    }),
                );
            }
        }
    }
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [command] if command == "list" => list(),
        [command, interface] if command == "capture" => capture(interface),
        _ => {
            log(
                "error",
                "usage",
                json!({ "usage": "kalbion-sniffer list | kalbion-sniffer capture <interface>" }),
            );
            ExitCode::from(EXIT_USAGE)
        }
    }
}
