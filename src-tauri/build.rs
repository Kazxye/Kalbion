fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["dispatch", "export_session", "import_catalog"]),
    ))
    .expect("Tauri build failed");
}
