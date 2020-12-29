use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KeymapError {
    #[error("unknown modifier: {0}")]
    UnknownModifier(String),

    #[error("key definition missing")]
    MissingDefinition,

    #[error("keymap not found: {0}")]
    MissingKeymap(String),

    #[error("unrecognised key: {0}")]
    UnknownKey(String),

    #[error("parse: {0}")]
    Parse(#[from] pest::error::Error<crate::keymap_file::Rule>),

    #[error("binding: {0}")]
    Binding(#[from] crate::bindings::BindingError),

    #[error("with file {file}: {error}")]
    WithFile {
        #[source] error: Box<KeymapError>,
        file: PathBuf,
    }
}

impl KeymapError {
    pub(crate) fn with_file(self, file: impl AsRef<Path>) -> Self {
        Self::WithFile {
            error: Box::new(self),
            file: file.as_ref().to_owned(),
        }
    }
}

pub(crate) type Result<T> = std::result::Result<T, KeymapError>;