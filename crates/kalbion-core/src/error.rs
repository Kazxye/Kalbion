#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Validation(String),
    /// An external service could not answer; distinct from invalid input.
    #[error("{0}")]
    Unavailable(String),
    #[error("Falha no banco de dados local")]
    Database(#[from] rusqlite::Error),
    #[error("JSON inválido: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Falha ao gerar CSV")]
    Csv(#[from] csv::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
