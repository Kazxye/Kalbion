#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kalbion_core::{
    adapters,
    domain::*,
    licensing::{KeyAuth, LicenseProvider},
    Store,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::Manager;

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
        item: Item,
        quantity: u32,
    },
    Import {
        session_id: String,
        json: String,
    },
    Price {
        session_id: String,
        item_id: String,
        quality: u8,
        amount: Option<i64>,
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
    Split {
        session_id: String,
        amount: i64,
        players: Vec<String>,
        confirm: bool,
    },
}
fn execute(store: &mut Store, request: Request) -> Result<Value> {
    match request {
        Request::Bootstrap => Ok(
            json!({"sessions":store.sessions()?,"settings":store.settings()?,"catalog":adapters::catalog(),"license":KeyAuth.status()}),
        ),
        Request::CreateSession { name } => Ok(serde_json::to_value(store.create_session(&name)?)?),
        Request::SetClosed { session_id, closed } => {
            store.set_closed(&session_id, closed)?;
            Ok(Value::Null)
        }
        Request::View { session_id, filter } => {
            Ok(serde_json::to_value(store.view(&session_id, &filter)?)?)
        }
        Request::Simulate { session_id } => Ok(serde_json::to_value(store.simulate(&session_id)?)?),
        Request::Manual {
            session_id,
            player,
            item,
            quantity,
        } => Ok(serde_json::to_value(store.manual(
            &session_id,
            &player,
            item,
            quantity,
        )?)?),
        Request::Import { session_id, json } => {
            Ok(serde_json::to_value(store.import(&session_id, &json)?)?)
        }
        Request::Price {
            session_id,
            item_id,
            quality,
            amount,
        } => {
            store.price(&session_id, &item_id, quality, amount)?;
            Ok(Value::Null)
        }
        Request::Settings { settings } => {
            store.save_settings(settings)?;
            Ok(Value::Null)
        }
        Request::Ledger {
            session_id,
            kind,
            player,
            description,
            amount,
        } => {
            store.add_ledger(&session_id, kind, &player, &description, amount)?;
            Ok(Value::Null)
        }
        Request::Split {
            session_id,
            amount,
            players,
            confirm,
        } => Ok(serde_json::to_value(if confirm {
            store.record_split(&session_id, amount, &players)?
        } else {
            split(amount, &players)?
        })?),
    }
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
    execute(&mut store, request).map_err(|error| {
        tracing::error!(operation="ipc_failed", cause=%error);
        error.to_string()
    })
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
        .map_err(|error| error.to_string())?;
    let extension = format.clone();
    let file = rfd::AsyncFileDialog::new()
        .set_title("Exportar sessão completa")
        .set_file_name(format!("kalbion-session.{format}"))
        .add_filter("Exportação", &[&extension])
        .save_file()
        .await;
    let Some(file) = file else {
        return Ok(false);
    };
    let path = file.path().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let mut output = std::fs::File::create(path)?;
        std::io::Write::write_all(&mut output, contents.as_bytes())?;
        output.sync_all()
    })
    .await
    .map_err(|_| "Falha ao executar exportação")?
    .map_err(|error| {
        tracing::error!(operation="export_failed", cause=%error);
        "Não foi possível salvar o arquivo".to_string()
    })?;
    tracing::info!(operation="session_exported", %session_id, %format);
    Ok(true)
}
fn main() {
    tracing_subscriber::fmt().json().with_target(false).init();
    let result = tauri::Builder::default()
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let store = Store::open(directory.join("kalbion.db"))?;
            app.manage(AppState(Mutex::new(store)));
            tracing::info!(operation = "application_started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![dispatch, export_session])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        tracing::error!(operation="application_failed", cause=%error);
        rfd::MessageDialog::new().set_title("Kalbion — falha ao iniciar").set_description("Não foi possível abrir o aplicativo ou o banco local. Consulte os logs do terminal; preserve o banco antes de tentar repará-lo.").set_level(rfd::MessageLevel::Error).show();
        std::process::exit(1);
    }
}
