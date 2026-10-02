//! Live capture: starts the `kalbion-sniffer` helper (the only privileged part), reads the
//! PCAP stream it writes to stdout and runs it through the same decoder as the file import.
//! Loot of players on the roster goes into the session as `observed`, one event at a time.
//!
//! Optionally writes a diagnostic trace (JSON lines) next to the app log: every loot pick-up
//! with what became of it, the item-related events with redacted parameters, and a count
//! of all other event codes every 10 s. It exists to answer "I picked X up, why is it not
//! in the list?" against what the game actually sent.
use crate::AppState;
use kalbion_capture::{convert, pcap, trace::Tracer, Limits, Observation, Pipeline, Seen};
use kalbion_core::{domain::Item, store::CaptureImport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const RECENT: usize = 30;
const TRACE_FLUSH_EVERY: Duration = Duration::from_secs(1);
const MAX_TRACE_BYTES: u64 = 50 * 1024 * 1024;
/// Helper exit codes (see crates/kalbion-sniffer).
const EXIT_PERMISSION: i32 = 3;
const EXIT_NO_INTERFACE: i32 = 4;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Interface {
    pub name: String,
    pub description: Option<String>,
    pub addresses: Vec<String>,
    pub loopback: bool,
    pub up: bool,
    pub running: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Idle,
    Running,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentLoot {
    pub at: String,
    /// Only for players on the roster; others are not named.
    pub player: Option<String>,
    pub item_index: u32,
    pub item: Option<Item>,
    pub quantity: u32,
    /// `inserted`, `duplicate`, `outside_roster`, `unknown_item`, `invalid_player`, `error`.
    pub outcome: &'static str,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LiveStatus {
    pub state: State,
    pub session_id: Option<String>,
    pub interface: Option<String>,
    pub started_at: Option<String>,
    pub stopped_at: Option<String>,
    pub error: Option<String>,
    pub inserted: u64,
    pub duplicates: u64,
    pub outside_roster: u64,
    pub invalid_players: u64,
    /// Game item indexes the imported catalog does not have, with counts.
    pub unknown_items: BTreeMap<u32, u64>,
    /// Packets the kernel or driver dropped before the helper read them.
    pub dropped_by_capture: u64,
    pub recent: VecDeque<RecentLoot>,
    pub trace_path: Option<String>,
    pub trace_full: bool,
    pub diagnostics: Option<kalbion_capture::Diagnostics>,
}

struct Running {
    child: Arc<Mutex<Child>>,
    /// Kept open while capturing; the helper exits when it closes.
    stdin: Option<ChildStdin>,
    reader: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct LiveCapture {
    running: Mutex<Option<Running>>,
    status: Arc<Mutex<LiveStatus>>,
}

impl LiveCapture {
    pub fn status(&self) -> LiveStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default()
    }
}

fn update(status: &Mutex<LiveStatus>, change: impl FnOnce(&mut LiveStatus)) {
    if let Ok(mut status) = status.lock() {
        change(&mut status);
    }
}

/// Debug builds may point at another helper (`KALBION_SNIFFER`); otherwise it must sit next
/// to the app executable, which is where Cargo builds it and where the installer puts it.
pub fn sniffer_path() -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("KALBION_SNIFFER") {
        tracing::warn!(operation = "sniffer_overridden");
        return Ok(PathBuf::from(path));
    }
    let name = if cfg!(windows) {
        "kalbion-sniffer.exe"
    } else {
        "kalbion-sniffer"
    };
    let directory = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .ok_or("Não foi possível localizar o executável do Kalbion")?;
    let path = directory.join(name);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "Ajudante de captura não encontrado em {}. Compile com: cargo build -p kalbion-sniffer",
            path.display()
        ))
    }
}

#[cfg(not(windows))]
fn command(path: &PathBuf) -> Command {
    Command::new(path)
}
#[cfg(windows)]
fn command(path: &PathBuf) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(path);
    // CREATE_NO_WINDOW: no console flashing up next to the app.
    command.creation_flags(0x0800_0000);
    command
}

fn permission_hint(path: &std::path::Path) -> String {
    if cfg!(windows) {
        "Sem permissão de captura. Instale o Npcap (npcap.com) sem marcar a opção que restringe a captura a administradores.".into()
    } else {
        format!(
            "Sem permissão de captura. Conceda uma vez, só ao ajudante (repita após recompilá-lo): sudo setcap cap_net_raw=eip {}",
            path.display()
        )
    }
}

pub fn interfaces() -> Result<Vec<Interface>, String> {
    let path = sniffer_path()?;
    let output = command(&path)
        .arg("list")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            tracing::error!(operation = "sniffer_list_failed", cause = %error);
            format!("Não foi possível executar o ajudante de captura: {error}")
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(operation = "sniffer_list_failed", stderr = %stderr.trim());
        if output.status.code() == Some(EXIT_PERMISSION) {
            return Err(permission_hint(&path));
        }
        return Err("O ajudante de captura não conseguiu listar as interfaces de rede".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        tracing::error!(operation = "sniffer_list_failed", cause = %error);
        "Resposta inválida do ajudante de captura".to_string()
    })
}

struct Run {
    run_id: String,
    session_id: String,
    interface: String,
    roster: BTreeSet<String>,
}

struct TraceFile {
    path: PathBuf,
    writer: BufWriter<std::fs::File>,
    written: u64,
    full: bool,
}
impl TraceFile {
    fn open(directory: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&directory)?;
        let name = format!("live-{}.jsonl", chrono::Utc::now().format("%Y%m%dT%H%M%SZ"));
        let path = directory.join(name);
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        Ok(Self {
            path,
            writer: BufWriter::new(file),
            written: 0,
            full: false,
        })
    }
    fn write(&mut self, record: &Value) {
        if self.full {
            return;
        }
        let line = format!("{record}\n");
        if self.written + line.len() as u64 > MAX_TRACE_BYTES {
            self.full = true;
            let _ = self
                .writer
                .write_all(b"{\"kind\":\"trace_full\",\"note\":\"limite de 50 MB atingido\"}\n");
            return;
        }
        if let Err(error) = self.writer.write_all(line.as_bytes()) {
            tracing::error!(operation = "trace_write_failed", cause = %error);
            self.full = true;
            return;
        }
        self.written += line.len() as u64;
    }
}

pub fn start(
    app: &AppHandle,
    live: &LiveCapture,
    session_id: String,
    roster: Vec<String>,
    interface: String,
    trace: bool,
) -> Result<LiveStatus, String> {
    let roster = convert::roster(&roster)?;
    reap(live);
    let mut running = live
        .running
        .lock()
        .map_err(|_| "Estado da captura indisponível")?;
    if running.is_some() {
        return Err("Já existe uma captura em andamento; pare-a antes de iniciar outra".into());
    }
    {
        let store = app.state::<AppState>();
        let store = store.0.lock().map_err(|_| "Banco indisponível")?;
        let session = store
            .session(&session_id)
            .map_err(|error| error.to_string())?;
        if session.closed_at.is_some() {
            return Err("Sessão encerrada; reabra para capturar loot".into());
        }
        let catalog = store.catalog_info().map_err(|error| error.to_string())?;
        if catalog.kind != "ao_bin_dumps" {
            return Err("Importe o catálogo items.json (Configurações) da mesma versão do jogo antes de capturar".into());
        }
    }
    if !interfaces()?.iter().any(|known| known.name == interface) {
        return Err("Interface de rede desconhecida; atualize a lista".into());
    }
    let path = sniffer_path()?;
    let mut child = command(&path)
        .arg("capture")
        .arg(&interface)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            tracing::error!(operation = "sniffer_start_failed", cause = %error);
            format!("Não foi possível iniciar o ajudante de captura: {error}")
        })?;
    let stdin = child.stdin.take();
    let stdout = child
        .stdout
        .take()
        .ok_or("Saída do ajudante indisponível")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Saída do ajudante indisponível")?;

    let trace_file = if trace {
        let directory = app
            .path()
            .app_log_dir()
            .map_err(|error| error.to_string())?
            .join("captures");
        match TraceFile::open(directory) {
            Ok(file) => Some(file),
            Err(error) => {
                tracing::error!(operation = "trace_open_failed", cause = %error);
                None
            }
        }
    } else {
        None
    };
    let run = Run {
        run_id: format!("live-{}", uuid::Uuid::new_v4()),
        session_id: session_id.clone(),
        interface: interface.clone(),
        roster,
    };
    let started_at = kalbion_core::domain::now();
    update(&live.status, |status| {
        *status = LiveStatus {
            state: State::Running,
            session_id: Some(session_id.clone()),
            interface: Some(interface.clone()),
            started_at: Some(started_at),
            trace_path: trace_file
                .as_ref()
                .map(|file| file.path.display().to_string()),
            ..LiveStatus::default()
        };
    });
    tracing::info!(operation = "live_capture_started", %session_id, %interface, run_id = %run.run_id, trace);

    let child = Arc::new(Mutex::new(child));
    spawn_stderr_reader(stderr, live.status.clone());
    let reader = {
        let app = app.clone();
        let status = live.status.clone();
        let child = child.clone();
        std::thread::Builder::new()
            .name("live-capture".into())
            .spawn(move || {
                let error = read(&app, &run, stdout, &status, trace_file);
                finish(&app, &run, &status, &child, &path, error);
            })
            .map_err(|error| format!("Não foi possível iniciar a leitura da captura: {error}"))?
    };
    *running = Some(Running {
        child,
        stdin,
        reader: Some(reader),
    });
    Ok(live.status())
}

fn spawn_stderr_reader(stderr: std::process::ChildStderr, status: Arc<Mutex<LiveStatus>>) {
    let _ = std::thread::Builder::new()
        .name("live-capture-log".into())
        .spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                let record: Value = serde_json::from_str(&line).unwrap_or(json!({ "text": line }));
                let operation = record["operation"].as_str().unwrap_or("message");
                if record["level"] == "error" {
                    tracing::warn!(operation = "sniffer", sniffer = %line);
                    let cause = record["cause"].as_str().unwrap_or(operation).to_string();
                    update(&status, |status| {
                        status.error.get_or_insert(cause);
                    });
                } else {
                    tracing::info!(operation = "sniffer", sniffer = %line);
                }
                if operation == "capture_stats" {
                    let dropped = record["dropped"].as_u64().unwrap_or(0)
                        + record["if_dropped"].as_u64().unwrap_or(0);
                    update(&status, |status| status.dropped_by_capture = dropped);
                }
            }
        });
}

/// Reads the helper's PCAP stream until it ends. Returns an error message when the run
/// must stop for a reason other than the helper exiting.
fn read(
    app: &AppHandle,
    run: &Run,
    stdout: std::process::ChildStdout,
    status: &Mutex<LiveStatus>,
    mut trace_file: Option<TraceFile>,
) -> Option<String> {
    let limits = Limits::default();
    let mut reader =
        match pcap::Reader::new(BufReader::new(stdout), limits.max_packet_bytes, u64::MAX) {
            Ok(reader) => reader,
            // The helper failed before writing its header; `finish` reports why.
            Err(_) => return None,
        };
    let mut pipeline = Pipeline::new(None);
    let mut tracer = Tracer::new();
    let mut observations: Vec<Observation> = Vec::new();
    let mut records: Vec<Value> = Vec::new();
    let mut last_flush = Instant::now();
    if let Some(file) = trace_file.as_mut() {
        file.write(&json!({
            "kind": "start",
            "at": kalbion_core::domain::now(),
            "decoder_version": kalbion_capture::DECODER_VERSION,
            "codebook_from": kalbion_capture::albion::CODEBOOK_FROM,
            "loot_event": kalbion_capture::albion::LOOT_EVENT,
            "watched": kalbion_capture::trace::WATCHED.iter().map(|(code, name)| json!({"code": code, "name": name})).collect::<Vec<_>>(),
            "interface": run.interface,
            "roster_size": run.roster.len(),
        }));
    }
    // The stream is open: show zeroed counters, so "no traffic yet" is visible as such.
    let initial = pipeline.diagnostics.clone();
    update(status, |status| status.diagnostics = Some(initial));
    let tracing_enabled = trace_file.is_some();
    let mut failure = None;
    loop {
        let frame = match reader.next_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(error) => {
                failure = Some(format!("Fluxo de captura inválido: {error}"));
                break;
            }
        };
        observations.clear();
        let decoded = pipeline.frame(&frame, &mut observations, &mut |seen: Seen| {
            if tracing_enabled {
                tracer.seen(&seen, &mut records);
            }
        });
        if let Err(error) = decoded {
            tracing::warn!(operation = "live_frame_skipped", cause = %error);
        }
        for observation in &observations {
            if let Err(error) = store_loot(app, run, observation, status, &mut records) {
                failure = Some(error);
                break;
            }
        }
        if let Some(file) = trace_file.as_mut() {
            for record in records.drain(..) {
                file.write(&record);
            }
            if last_flush.elapsed() >= TRACE_FLUSH_EVERY {
                last_flush = Instant::now();
                let _ = file.writer.flush();
            }
        } else {
            records.clear();
        }
        // Every frame: the counters are small, and a burst followed by silence must not
        // leave the panel showing stale numbers.
        let diagnostics = pipeline.diagnostics.clone();
        let full = trace_file.as_ref().is_some_and(|file| file.full);
        update(status, |status| {
            status.diagnostics = Some(diagnostics);
            status.trace_full = full;
        });
        if failure.is_some() {
            break;
        }
    }
    pipeline.finish();
    if let Some(file) = trace_file.as_mut() {
        tracer.flush(&mut records);
        for record in records.drain(..) {
            file.write(&record);
        }
        file.write(&json!({
            "kind": "end",
            "at": kalbion_core::domain::now(),
            "diagnostics": pipeline.diagnostics,
        }));
        let _ = file.writer.flush();
    }
    let diagnostics = pipeline.diagnostics.clone();
    update(status, |status| status.diagnostics = Some(diagnostics));
    failure
}

fn store_loot(
    app: &AppHandle,
    run: &Run,
    observation: &Observation,
    status: &Mutex<LiveStatus>,
    records: &mut Vec<Value>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut store = state
        .0
        .lock()
        .map_err(|_| "Banco indisponível".to_string())?;
    let mut resolved: Option<Item> = None;
    let converted = convert::observation(
        observation,
        &run.run_id,
        convert::LIVE_SOURCE,
        &run.session_id,
        &run.roster,
        &mut |index| {
            let item = store.catalog_item_by_game_index(index)?;
            resolved.clone_from(&item);
            Ok::<_, kalbion_core::Error>(item)
        },
    );
    let (outcome, player, error) = match converted {
        Ok(convert::Outcome::Event(event)) => {
            let player = Some(event.player.clone());
            match store.ingest(std::slice::from_ref(&event), false) {
                Ok(result) if result.inserted > 0 => ("inserted", player, None),
                Ok(_) => ("duplicate", player, None),
                Err(error) => ("error", player, Some(error.to_string())),
            }
        }
        Ok(convert::Outcome::OutsideRoster) => ("outside_roster", None, None),
        Ok(convert::Outcome::UnknownItem(_)) => ("unknown_item", None, None),
        Ok(convert::Outcome::InvalidPlayer) => ("invalid_player", None, None),
        Err(error) => ("error", None, Some(error.to_string())),
    };
    drop(store);
    // Roster players are already stored by name; the unknown-item case keeps the name only
    // if that player is on the roster, so the trace can confirm whose pick-up it was.
    let player = player.or_else(|| {
        (outcome == "unknown_item")
            .then(|| kalbion_core::domain::normalize_player(&observation.loot.looted_by).ok())
            .flatten()
            .filter(|name| run.roster.contains(&name.to_lowercase()))
    });
    records.push(json!({
        "kind": "loot",
        "at": observation.occurred_at,
        "packet": observation.position.packet,
        "command": observation.position.command,
        "item_index": observation.loot.item_index,
        "quantity": observation.loot.quantity,
        "item": resolved,
        "player": player,
        "outcome": outcome,
        "error": error,
    }));
    tracing::info!(
        operation = "live_loot",
        outcome,
        item_index = observation.loot.item_index,
        item = resolved.as_ref().map(|item| item.id.as_str()).unwrap_or(""),
        quantity = observation.loot.quantity
    );
    update(status, |status| {
        match outcome {
            "inserted" => status.inserted += 1,
            "duplicate" => status.duplicates += 1,
            "outside_roster" => status.outside_roster += 1,
            "unknown_item" => {
                *status
                    .unknown_items
                    .entry(observation.loot.item_index)
                    .or_default() += 1
            }
            "invalid_player" => status.invalid_players += 1,
            _ => {}
        }
        status.recent.push_front(RecentLoot {
            at: observation.occurred_at.clone(),
            player: player.clone(),
            item_index: observation.loot.item_index,
            item: resolved.clone(),
            quantity: observation.loot.quantity,
            outcome,
        });
        status.recent.truncate(RECENT);
    });
    match error {
        Some(error) => Err(format!("Falha ao gravar loot; captura parada: {error}")),
        None => Ok(()),
    }
}

/// Runs on the reader thread once the stream ends: collects the helper's exit, records the
/// run in the capture audit table and sets the final state.
fn finish(
    app: &AppHandle,
    run: &Run,
    status: &Mutex<LiveStatus>,
    child: &Mutex<Child>,
    path: &std::path::Path,
    failure: Option<String>,
) {
    let exit = child.lock().ok().and_then(|mut child| {
        if failure.is_some() {
            let _ = child.kill();
        }
        child.wait().ok()
    });
    let stopped_by_user = status
        .lock()
        .map(|status| status.state == State::Stopped)
        .unwrap_or(false);
    let helper_error = match exit.and_then(|exit| exit.code()) {
        _ if stopped_by_user => None,
        Some(0) => None,
        Some(EXIT_PERMISSION) => Some(permission_hint(path)),
        Some(EXIT_NO_INTERFACE) => Some("Interface de rede não encontrada pelo ajudante".into()),
        Some(code) => Some(format!(
            "O ajudante de captura terminou com erro (código {code})"
        )),
        None if exit.is_some() => Some("O ajudante de captura foi encerrado".into()),
        None => None,
    };
    let message = failure.or(helper_error);
    let snapshot = {
        let mut snapshot = LiveStatus::default();
        update(status, |status| {
            if let Some(message) = &message {
                status.state = State::Failed;
                status.error = Some(message.clone());
            } else {
                status.state = State::Stopped;
            }
            status.stopped_at = Some(kalbion_core::domain::now());
            snapshot = status.clone();
        });
        snapshot
    };
    let state = app.state::<AppState>();
    if let Ok(store) = state.0.lock() {
        let record = CaptureImport {
            session_id: run.session_id.clone(),
            file_label: format!("Captura ao vivo ({})", run.interface)
                .chars()
                .filter(|c| !c.is_control())
                .take(200)
                .collect(),
            file_sha256: String::new(),
            fingerprint: run.run_id.clone(),
            decoder_version: kalbion_capture::DECODER_VERSION.into(),
            inserted: snapshot.inserted as usize,
            duplicates: snapshot.duplicates as usize,
            diagnostics: serde_json::to_string(&snapshot.diagnostics).unwrap_or_default(),
        };
        if let Err(error) = store.record_capture_import(&record) {
            tracing::error!(operation = "live_capture_audit_failed", cause = %error);
        }
    }
    tracing::info!(
        operation = "live_capture_stopped",
        run_id = %run.run_id,
        inserted = snapshot.inserted,
        duplicates = snapshot.duplicates,
        error = message.as_deref().unwrap_or("")
    );
}

pub fn stop(live: &LiveCapture) -> Result<LiveStatus, String> {
    let running = live
        .running
        .lock()
        .map_err(|_| "Estado da captura indisponível")?
        .take();
    let Some(mut running) = running else {
        return Ok(live.status());
    };
    update(&live.status, |status| {
        if status.state == State::Running {
            status.state = State::Stopped;
        }
    });
    drop(running.stdin.take());
    if let Ok(mut child) = running.child.lock() {
        let _ = child.kill();
    }
    if let Some(reader) = running.reader.take() {
        let _ = reader.join();
    }
    Ok(live.status())
}

/// Clears a finished run so a new one can start; a running capture is left alone.
pub fn reap(live: &LiveCapture) {
    let finished = live.running.lock().ok().and_then(|mut running| {
        let done = running
            .as_ref()
            .and_then(|running| running.reader.as_ref())
            .is_some_and(JoinHandle::is_finished);
        if done {
            running.take()
        } else {
            None
        }
    });
    if let Some(mut running) = finished {
        if let Some(reader) = running.reader.take() {
            let _ = reader.join();
        }
    }
}
