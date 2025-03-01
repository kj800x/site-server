use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("{0}")]
    Context(String),
}

pub type Result<T> = std::result::Result<T, Error>;

// Helper trait to provide context for errors
pub trait ResultExt<T> {
    fn context(self, context: &str) -> Result<T>;
}

impl<T, E> ResultExt<T> for std::result::Result<T, E>
where
    E: std::error::Error + 'static,
{
    fn context(self, context: &str) -> Result<T> {
        self.map_err(|_| Error::Context(context.to_string()))
    }
}
