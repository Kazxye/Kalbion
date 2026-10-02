#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kalbion_capture::{albion, convert};
use kalbion_core::{
    catalog,
    domain::*,
    icons::IconCache,
    licensing::{KeyAuth, LicenseProvider},
    market::AlbionDataProject,
    store::{CaptureImport, InsertResult, MarketRefresh},
    Store,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;
use std::sync::Mutex;
use tauri::Manager;

mod live;
mod logging;

struct AppState(Mutex<Store>);
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Bootstrap,
    CreateSession {
        name: String,
    },
    SetClosed {
        session_id: String,
        closed: bool,
    },
    View {
        session_id: String,
        filter: Filter,
    },
    Simulate {
        session_id: String,
    },
    Manual {
        session_id: String,
        player: String,
        item_id: String,
        quality: Option<u8>,
        quantity: u32,
    },
    Import {
        session_id: String,
        json: String,
    },
    SetVoided {
        session_id: String,
        source: String,
        event_id: String,
        voided: bool,
    },
    Price {
        session_id: String,
        item_id: String,
        quality: Option<u8>,
        amount: Option<i64>,
    },
    CatalogSearch {
        query: String,
        limit: u32,
    },
    Settings {
        settings: Settings,
    },
    Ledger {
        session_id: String,
        kind: LedgerKind,
        player: String,
        description: String,
        amount: i64,
    },
    ReverseLedger {
        session_id: String,
        entry_id: String,
    },
    Split {
        session_id: String,
        amount: i64,
        players: Vec<String>,
        confirm: bool,
    },
}
fn execute(store: &mut Store, request: Request) -> kalbion_core::Result<Value> {
    Ok(match request {
        Request::Bootstrap => json!({
            "sessions": store.sessions()?,
            "settings": store.settings()?,
            "catalog": store.catalog_info()?,
            "license": KeyAuth.status(),
            "log_path": logging::path(),
            "log_failed": logging::failed(),
        }),
        Request::CreateSession { name } => serde_json::to_value(store.create_session(&name)?)?,
        Request::SetClosed { session_id, closed } => {
            store.set_closed(&session_id, closed)?;
            Value::Null
        }
        Request::View { session_id, filter } => {
            serde_json::to_value(store.view(&session_id, &filter)?)?
        }
        Request::Simulate { session_id } => serde_json::to_value(store.simulate(&session_id)?)?,
        Request::Manual {
            session_id,
            player,
            item_id,
            quality,
            quantity,
        } => serde_json::to_value(store.manual(
            &session_id,
            &player,
            &item_id,
            quality,
            quantity,
        )?)?,
        Request::Import { session_id, json } => {
            serde_json::to_value(store.import(&session_id, &json)?)?
        }
        Request::SetVoided {
            session_id,
            source,
            event_id,
            voided,
        } => {
            store.set_voided(&session_id, &source, &event_id, voided)?;
            Value::Null
        }
        Request::Price {
            session_id,
            item_id,
            quality,
            amount,
        } => {
            store.price(&session_id, &item_id, quality, amount)?;
            Value::Null
        }
        Request::CatalogSearch { query, limit } => {
            serde_json::to_value(store.search_catalog(&query, limit)?)?
        }
        Request::Settings { settings } => {
            store.save_settings(&settings)?;
            Value::Null
        }
        Request::Ledger {
            session_id,
            kind,
            player,
            description,
            amount,
        } => serde_json::to_value(store.add_ledger(
            &session_id,
            kind,
            &player,
            &description,
            amount,
        )?)?,
        Request::ReverseLedger {
            session_id,
            entry_id,
        } => serde_json::to_value(store.reverse_ledger(&session_id, &entry_id)?)?,
        Request::Split {
            session_id,
            amount,
            players,
            confirm,
        } => serde_json::to_value(if confirm {
            store.record_split(&session_id, amount, &players)?
        } else {
            split(amount, &players)?
        })?,
    })
}
fn report(operation: &str, error: kalbion_core::Error) -> String {
    tracing::error!(operation, cause = ?error);
    error.to_string()
}
#[tauri::command]
async fn dispatch(
    state: tauri::State<'_, AppState>,
    request: Request,
) -> std::result::Result<Value, String> {
    let mut store = state
        .0
        .lock()
        .map_err(|_| "Banco indisponível; reinicie o aplicativo")?;
    execute(&mut store, request).map_err(|error| report("ipc_failed", error))
}
#[tauri::command]
async fn export_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
    format: String,
) -> std::result::Result<bool, String> {
    let contents = state
        .0
        .lock()
        .map_err(|_| "Banco indisponível")?
        .export(&session_id, &format)
        .map_err(|error| report("export_failed", error))?;
    let file = rfd::AsyncFileDialog::new()
        .set_title("Exportar sessão completa")
        .set_file_name(format!("kalbion-session.{format}"))
        .add_filter("Exportação", &[&format])
        .save_file()
        .await;
    let Some(file) = file else {
        return Ok(false);
    };
    let path = file.path().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let mut output = std::fs::File::create(path)?;
        output.write_all(contents.as_bytes())?;
        output.sync_all()
    })
    .await
    .map_err(|_| "Falha ao executar exportação")?
    .map_err(|error| {
        tracing::error!(operation = "export_failed", cause = %error);
        "Não foi possível salvar o arquivo".to_string()
    })?;
    tracing::info!(operation = "session_exported", %session_id, %format);
    Ok(true)
}
/// Debug builds only: the desktop smoke test answers "open file" dialogs, which WebDriver
/// cannot drive, with `<KALBION_TEST_DIALOG_DIR>/<name>`. Release builds always ask the user.
async fn pick_file(dialog: rfd::AsyncFileDialog, test_name: &str) -> Option<std::path::PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(directory) = std::env::var_os("KALBION_TEST_DIALOG_DIR") {
        tracing::warn!(operation = "file_dialog_overridden", file = test_name);
        return Some(std::path::PathBuf::from(directory).join(test_name));
    }
    #[cfg(not(debug_assertions))]
    let _ = test_name;
    dialog.pick_file().await.map(|file| file.path().to_owned())
}
/// The path comes only from the native dialog, never from the webview.
#[tauri::command]
async fn import_catalog(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<Option<catalog::CatalogInfo>, String> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title("Importar catálogo (formatted/items.json)")
        .add_filter("Catálogo ao-bin-dumps", &["json"]);
    let Some(path) = pick_file(dialog, "items.json").await else {
        return Ok(None);
    };
    let label: String = path
        .file_name()
        .map(|name| {
            name.to_string_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .take(200)
                .collect()
        })
        .filter(|name: &String| !name.trim().is_empty())
        .unwrap_or_else(|| "items.json".into());
    let parsed = tauri::async_runtime::spawn_blocking(move || {
        let file = std::fs::File::open(&path).map_err(|error| {
            tracing::error!(operation = "catalog_read_failed", cause = %error);
            "Não foi possível ler o arquivo".to_string()
        })?;
        let size = file
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or(u64::MAX);
        if size > catalog::MAX_CATALOG_BYTES {
            return Err("Arquivo de catálogo maior que 64 MB".to_string());
        }
        catalog::parse_ao_bin_dumps(file).map_err(|error| report("catalog_parse_failed", error))
    })
    .await
    .map_err(|_| "Falha ao processar catálogo")??;
    let info = state
        .0
        .lock()
        .map_err(|_| "Banco indisponível")?
        .replace_catalog(&parsed, &label)
        .map_err(|error| report("catalog_import_failed", error))?;
    Ok(Some(info))
}
/// The network request runs without the database lock, so the rest of the app stays usable
/// while the Albion Data Project answers (up to its timeout).
#[tauri::command]
async fn refresh_market_prices(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> std::result::Result<MarketRefresh, String> {
    let plan = state
        .0
        .lock()
        .map_err(|_| "Banco indisponível")?
        .market_plan(&session_id)
        .map_err(|error| report("market_refresh_failed", error))?;
    let request = plan.clone();
    let quotes = tauri::async_runtime::spawn_blocking(move || {
        app.state::<AlbionDataProject>()
            .quotes(&request.server, &request.city, &request.wanted)
    })
    .await
    .map_err(|_| "Falha ao consultar preços")?
    .map_err(|error| report("market_refresh_failed", error))?;
    state
        .0
        .lock()
        .map_err(|_| "Banco indisponível")?
        .apply_market_quotes(&session_id, &plan, &quotes)
        .map_err(|error| report("market_refresh_failed", error))
}
#[derive(serde::Serialize)]
struct CaptureSummary {
    file_label: String,
    inserted: usize,
    duplicates: usize,
    report: convert::Report,
    diagnostics: kalbion_capture::Diagnostics,
    /// The capture is newer than the latest date the codebook was observed in real traffic.
    newer_than_codebook: bool,
}
/// Offline import of a capture file the user recorded. The path comes only from the native
/// dialog; decoding runs without the database lock. Nothing here captures traffic.
#[tauri::command]
async fn import_capture(
    state: tauri::State<'_, AppState>,
    session_id: String,
    roster: Vec<String>,
) -> std::result::Result<Option<CaptureSummary>, String> {
    let roster = convert::roster(&roster)?;
    {
        let store = state.0.lock().map_err(|_| "Banco indisponível")?;
        let session = store
            .session(&session_id)
            .map_err(|error| report("capture_import_failed", error))?;
        if session.closed_at.is_some() {
            return Err("Sessão encerrada; reabra para importar loot".into());
        }
        let catalog = store
            .catalog_info()
            .map_err(|error| report("capture_import_failed", error))?;
        if catalog.kind != "ao_bin_dumps" {
            return Err("Importe o catálogo items.json (Configurações) da mesma versão do jogo antes de importar capturas".into());
        }
    }
    let dialog = rfd::AsyncFileDialog::new()
        .set_title("Importar captura de loot (PCAP/PCAPNG)")
        .add_filter("Captura de pacotes", &["pcap", "pcapng", "cap"]);
    let Some(path) = pick_file(dialog, "capture.pcap").await else {
        return Ok(None);
    };
    let file_label: String = path
        .file_name()
        .map(|name| {
            name.to_string_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .take(200)
                .collect()
        })
        .filter(|name: &String| !name.trim().is_empty())
        .unwrap_or_else(|| "captura.pcap".into());
    let capture = tauri::async_runtime::spawn_blocking(move || {
        let limits = kalbion_capture::Limits::default();
        let file = std::fs::File::open(&path).map_err(|error| {
            tracing::error!(operation = "capture_read_failed", cause = %error);
            "Não foi possível ler o arquivo".to_string()
        })?;
        let size = file
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or(u64::MAX);
        if size > limits.max_file_bytes {
            return Err(format!(
                "Arquivo maior que o limite de {} MB",
                limits.max_file_bytes / (1024 * 1024)
            ));
        }
        kalbion_capture::decode(std::io::BufReader::new(file), limits).map_err(|error| {
            tracing::warn!(operation = "capture_decode_refused", cause = %error);
            error.to_string()
        })
    })
    .await
    .map_err(|_| "Falha ao processar a captura")??;
    let mut store = state.0.lock().map_err(|_| "Banco indisponível")?;
    if let Some(other) = store
        .capture_session_elsewhere(&capture.fingerprint, &session_id)
        .map_err(|error| report("capture_import_failed", error))?
    {
        return Err(format!(
            "Esta captura já foi importada na sessão «{other}»; o mesmo loot não pode contar em duas sessões"
        ));
    }
    let (events, conversion) = convert::to_events(&capture, &session_id, &roster, |index| {
        store.catalog_item_by_game_index(index)
    })
    .map_err(|error| report("capture_import_failed", error))?;
    let mut total = InsertResult {
        inserted: 0,
        duplicates: 0,
    };
    // Each batch is atomic; if a later batch fails the earlier ones stay, and importing the
    // same file again completes the rest without duplicates.
    for batch in events.chunks(kalbion_core::import::MAX_IMPORT_EVENTS) {
        let result = store
            .ingest(batch, true)
            .map_err(|error| report("capture_import_failed", error))?;
        total.inserted += result.inserted;
        total.duplicates += result.duplicates;
    }
    store
        .record_capture_import(&CaptureImport {
            session_id: session_id.clone(),
            file_label: file_label.clone(),
            file_sha256: capture.file_sha256.clone(),
            fingerprint: capture.fingerprint.clone(),
            decoder_version: kalbion_capture::DECODER_VERSION.into(),
            inserted: total.inserted,
            duplicates: total.duplicates,
            diagnostics: serde_json::to_string(&capture.diagnostics).unwrap_or_default(),
        })
        .map_err(|error| report("capture_import_failed", error))?;
    let newer_than_codebook = capture
        .diagnostics
        .last_packet_at
        .as_deref()
        .is_some_and(|last| last > albion::CODEBOOK_OBSERVED_UNTIL);
    Ok(Some(CaptureSummary {
        file_label,
        inserted: total.inserted,
        duplicates: total.duplicates,
        report: conversion,
        diagnostics: capture.diagnostics,
        newer_than_codebook,
    }))
}
#[tauri::command]
async fn live_capture_interfaces() -> std::result::Result<Vec<live::Interface>, String> {
    tauri::async_runtime::spawn_blocking(live::interfaces)
        .await
        .map_err(|_| "Falha ao listar interfaces".to_string())?
}
/// Starts the capture helper on an interface it listed itself; the helper path never comes
/// from the webview.
#[tauri::command]
async fn start_live_capture(
    app: tauri::AppHandle,
    session_id: String,
    roster: Vec<String>,
    interface: String,
    trace: bool,
) -> std::result::Result<live::LiveStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let capture = app.state::<live::LiveCapture>();
        live::start(&app, &capture, session_id, roster, interface, trace)
    })
    .await
    .map_err(|_| "Falha ao iniciar a captura".to_string())?
}
#[tauri::command]
async fn stop_live_capture(app: tauri::AppHandle) -> std::result::Result<live::LiveStatus, String> {
    tauri::async_runtime::spawn_blocking(move || live::stop(&app.state::<live::LiveCapture>()))
        .await
        .map_err(|_| "Falha ao parar a captura".to_string())?
}
#[tauri::command]
fn live_capture_status(capture: tauri::State<'_, live::LiveCapture>) -> live::LiveStatus {
    live::reap(&capture);
    capture.status()
}
/// Debug builds can point at a local mock (desktop smoke test); release builds always use
/// the official hosts.
fn market_source() -> AlbionDataProject {
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("KALBION_ADP_URL") {
        tracing::warn!(operation = "market_source_overridden", %url);
        return AlbionDataProject::with_base_url(url);
    }
    AlbionDataProject::default()
}
/// Serves `icon://localhost/<UniqueName>?quality=N` (Windows: `http://icon.localhost/...`).
/// Anything unexpected becomes a 4xx/5xx, and the UI falls back to its generic icon.
fn icon_response(
    app: &tauri::AppHandle,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let respond = |status: u16, body: Vec<u8>| {
        let mut response = tauri::http::Response::builder().status(status);
        if status == 200 {
            response = response
                .header("Content-Type", "image/png")
                .header("Cache-Control", "max-age=86400");
        }
        response.body(body).unwrap_or_default()
    };
    let Some(icons) = app.try_state::<IconCache>() else {
        return respond(503, Vec::new());
    };
    let Some(item_id) = percent_decode(request.uri().path().trim_start_matches('/')) else {
        return respond(400, Vec::new());
    };
    let quality = request
        .uri()
        .query()
        .and_then(|query| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix("quality="))
        })
        .and_then(|value| value.parse::<u8>().ok());
    match icons.get(&item_id, quality) {
        Ok(Some(bytes)) => respond(200, bytes),
        Ok(None) => respond(404, Vec::new()),
        Err(_) => respond(400, Vec::new()),
    }
}
fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}
fn main() {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // WebKitGTK's DMA-BUF renderer aborts on some Wayland/NVIDIA setups with
        // "Error 71 (Protocol error) dispatching to Wayland display". Set before any thread starts.
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    tracing_subscriber::fmt()
        .json()
        .with_target(false)
        .with_writer(|| logging::Writer)
        .init();
    let result = tauri::Builder::default()
        .setup(|app| {
            if let Err(error) = logging::open(&app.path().app_log_dir()?) {
                tracing::error!(operation = "log_file_unavailable", cause = %error);
            }
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let store = Store::open(directory.join("kalbion.db"))?;
            app.manage(AppState(Mutex::new(store)));
            app.manage(market_source());
            app.manage(live::LiveCapture::default());
            match IconCache::new(app.path().app_cache_dir()?.join("icons")) {
                Ok(icons) => {
                    app.manage(icons);
                }
                Err(error) => tracing::error!(operation = "icon_cache_unavailable", cause = %error),
            }
            tracing::info!(
                operation = "application_started",
                version = env!("CARGO_PKG_VERSION")
            );
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("icon", |context, request, responder| {
            let app = context.app_handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(icon_response(&app, &request));
            });
        })
        .invoke_handler(tauri::generate_handler![
            dispatch,
            export_session,
            import_catalog,
            refresh_market_prices,
            import_capture,
            live_capture_interfaces,
            start_live_capture,
            stop_live_capture,
            live_capture_status
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        tracing::error!(operation = "application_failed", cause = %error);
        let location = logging::path().map_or_else(
            || "nos logs do terminal".to_string(),
            |path| format!("em {}", path.display()),
        );
        rfd::MessageDialog::new()
            .set_title("Kalbion — falha ao iniciar")
            .set_description(format!(
                "Não foi possível abrir o aplicativo ou o banco local. Detalhes {location}. \
                 Preserve o banco antes de tentar repará-lo."
            ))
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn icon_paths_decode_enchantment_and_reject_malformed_escapes() {
        assert_eq!(percent_decode("T5_BAG%401").as_deref(), Some("T5_BAG@1"));
        assert_eq!(percent_decode("T4_BAG").as_deref(), Some("T4_BAG"));
        assert_eq!(percent_decode("%2E%2E%2Fx").as_deref(), Some("../x"));
        assert_eq!(percent_decode("T4%4"), None);
        assert_eq!(percent_decode("T4%zz"), None);
        assert_eq!(percent_decode("%FF"), None);
    }
}
