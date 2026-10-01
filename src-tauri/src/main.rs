#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kalbion_core::{
    catalog,
    domain::*,
    icons::IconCache,
    licensing::{KeyAuth, LicenseProvider},
    market::AlbionDataProject,
    store::MarketRefresh,
    Store,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;
use std::sync::Mutex;
use tauri::Manager;

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
            refresh_market_prices
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
