//! Error types.

use std::ffi::OsStr;
use std::result::Result as StdResult;
use std::sync::mpsc::{RecvError, SendError, TryRecvError};

use thiserror::Error;

/// Convenient return type for functions.
pub type Result<T> = StdResult<T, Error>;

/// Main error type.
#[derive(Debug, Error)]
pub enum Error {
    /// Comes from [Termwiz](https://crates.io/crates/termwiz).
    #[error("termwiz: {0}")]
    Termwiz(#[source] anyhow::Error),

    /// Comes from [Regex](https://github.com/rust-lang/regex).
    #[error("regex: {0}")]
    Regex(#[from] regex::Error),

    /// Generic I/O error.
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),

    /// Returned when persisting a temporary file fails.
    #[error("temporary file persistence: {0}")]
    TempfilePersist(#[from] tempfile::PersistError),

    /// Keymap-related error.
    #[error("keymap: {0}")]
    Keymap(#[from] crate::keymaps::error::KeymapError),

    /// Binding-related error.
    #[error("binding: {0}")]
    Binding(#[from] crate::bindings::BindingError),

    /// Generic formatting error.
    #[error("formatting: {0}")]
    Fmt(#[from] std::fmt::Error),

    /// Receive error on a channel.
    #[error("channel: {0}")]
    ChannelRecv(#[from] RecvError),

    /// (Try)Receive error on a channel.
    #[error("channel: {0}")]
    ChannelTryRecv(#[from] TryRecvError),

    /// Send error on a FileEvent channel.
    #[error("channel: {0}")]
    ChannelSendFileEvent(#[from] SendError<crate::file::FileEvent>),

    /// Send error on an Envelope channel.
    #[error("channel: {0}")]
    ChannelSendEnvelope(#[from] SendError<crate::event::Envelope>),

    /// Error returned if the terminfo database is missing.
    #[error("terminfo database not found (is $TERM correct?)")]
    TerminfoDatabaseMissing,

    /// Wrapped error within the context of a command.
    #[error("with command `{command}`: {error}")]
    WithCommand {
        /// Wrapped error.
        #[source]
        error: Box<Self>,

        /// Command the error is about.
        command: String,
    },

    /// Wrapped error within the context of a file.
    #[error("with file {file}: {error}")]
    WithFile {
        /// Wrapped error.
        #[source]
        error: Box<Self>,

        /// File the error is about.
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
