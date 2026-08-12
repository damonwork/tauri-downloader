use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("Descarga no encontrada")]
    DownloadNotFound,
    #[error("Perfil de proxy no encontrado")]
    ProxyNotFound,
    #[error("No se pudo acceder al almacenamiento local: {0}")]
    Storage(#[source] std::io::Error),
    #[error("El estado guardado no tiene un formato válido")]
    InvalidStore(#[source] serde_json::Error),
    #[error("No se pudo resolver el directorio de la aplicación")]
    AppDirectory,
    #[error("No se pudo abrir la ubicación del archivo: {0}")]
    ExternalOpen(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::Storage(error)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        Self::InvalidStore(error)
    }
}
