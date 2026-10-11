pub(crate) mod input;
pub mod model;
mod store;

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("invalid review input: {0}")]
    Invalid(String),
    #[error(transparent)]
    Update(#[from] model::UpdateRejection),
    #[error("review I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("review JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("review database failed: {0}")]
    Database(#[from] rusqlite::Error),
}

#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod tests;
