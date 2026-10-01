use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LicenseState {
    Disabled { reason: String },
    Unauthenticated,
    Valid { expires_at: String },
    Expired,
    Unavailable,
}
pub trait LicenseProvider {
    fn status(&self) -> LicenseState;
    fn authenticate(&self, license_key: &str) -> Result<LicenseState, String>;
}
pub struct KeyAuth;
impl LicenseProvider for KeyAuth {
    fn status(&self) -> LicenseState {
        LicenseState::Disabled { reason: "KeyAuth desabilitado nesta versão de desenvolvimento. Nenhuma licença foi validada.".into() }
    }
    fn authenticate(&self, _license_key: &str) -> Result<LicenseState, String> {
        Err("Integração KeyAuth não configurada".into())
    }
}
