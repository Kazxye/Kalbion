#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kalbion_core::{
    catalog,
    domain::*,
    licensing::{KeyAuth, LicenseProvider},
    Store,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::Manager;

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
static LOG_FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Sends every log line to stderr and, once the app knows its log directory, to a file.
/// A release build on Windows has no console, so the file is the only record there.
struct LogWriter;
impl Write for LogWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buffer);
        if let Some(Ok(mut file)) = LOG_FILE.get().map(Mutex::lock) {
            let _ = file.write_all(buffer);
        }
        Ok(buffer.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn open_log_file(directory: PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("kalbion.log");
    if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() > MAX_LOG_BYTES) {
        std::fs::rename(&path, directory.join("kalbion.log.1"))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let _ = LOG_FILE.set(Mutex::new(file));
    let _ = LOG_PATH.set(path);
    Ok(())
}

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
            "log_path": LOG_PATH.get(),
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
/// The path comes only from the native dialog, never from the webview.
#[tauri::command]
async fn import_catalog(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<Option<catalog::CatalogInfo>, String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Importar catálogo (formatted/items.json)")
        .add_filter("Catálogo ao-bin-dumps", &["json"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let path = file.path().to_owned();
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
        .with_writer(|| LogWriter)
        .init();
    let result = tauri::Builder::default()
        .setup(|app| {
            if let Err(error) = open_log_file(app.path().app_log_dir()?) {
                tracing::error!(operation = "log_file_unavailable", cause = %error);
            }
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let store = Store::open(directory.join("kalbion.db"))?;
            app.manage(AppState(Mutex::new(store)));
            tracing::info!(
                operation = "application_started",
                version = env!("CARGO_PKG_VERSION")
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dispatch,
            export_session,
            import_catalog
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        tracing::error!(operation = "application_failed", cause = %error);
        let location = LOG_PATH.get().map_or_else(
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
