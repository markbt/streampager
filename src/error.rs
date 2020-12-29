//! Error types.
use std::{
    ffi::OsStr,
    result::Result as StdResult,
    sync::mpsc::{RecvError,  SendError, TryRecvError},
};
use thiserror::Error;

/// Convenient return type for functions.
pub type Result<T> = StdResult<T, Error>;

/// Main error type
#[derive(Debug, Error)]
pub enum Error {
    #[error("termwiz: {0}")]
    Termwiz(anyhow::Error),

    #[error("regex: {0}")]
    Regex(#[from] regex::Error),

    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),

    #[error("temporary file persistence: {0}")]
    TempfilePersist(#[from] tempfile::PersistError),

    #[error("keymap: {0}")]
    Keymap(#[from] crate::keymaps::error::KeymapError),

    #[error("binding: {0}")]
    Binding(#[from] crate::bindings::BindingError),

    #[error("formatting: {0}")]
    Fmt(#[from] std::fmt::Error),

    #[error("channel: {0}")]
    ChannelRecv(#[from] RecvError),

    #[error("channel: {0}")]
    ChannelTryRecv(#[from] TryRecvError),

    #[error("channel: {0}")]
    ChannelSendFileEvent(#[from] SendError<crate::file::FileEvent>),

    #[error("channel: {0}")]
    ChannelSendEnvelope(#[from] SendError<crate::event::Envelope>),

    #[error("terminfo database not found (is $TERM correct?)")]
    TerminfoDatabaseMissing,

    #[error("with command `{command}`: {error}")]
    WithCommand {
        #[source] error: Box<Self>,
        command: String,
    },

    #[error("with file {file}: {error}")]
    WithFile {
        #[source] error: Box<Self>,
        file: String,
    },
}

impl Error {
    pub(crate) fn with_file(self, file: impl AsRef<str>) -> Self {
        Self::WithFile {
            error: Box::new(self),
            file: file.as_ref().to_owned(),
        }
    }

    pub(crate) fn with_command(self, command: impl AsRef<OsStr>) -> Self {
        Self::WithCommand {
            error: Box::new(self),
            command: command.as_ref().to_string_lossy().to_string(),
        }
    }
}