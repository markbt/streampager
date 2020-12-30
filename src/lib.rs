//! Stream Pager
//!
//! A pager for streams.
#![warn(missing_docs)]
#![recursion_limit = "1024"]
#![allow(clippy::comparison_chain)]

use std::ffi::OsStr;
use std::io::Read;
use std::sync::Arc;

pub use error::{Error, Result};
use termwiz::caps::ColorLevel;
use termwiz::caps::{Capabilities, ProbeHints};
use termwiz::terminal::{SystemTerminal, Terminal};
use vec_map::VecMap;

mod bar;
pub mod bindings;
mod buffer;
mod buffer_cache;
mod command;
pub mod config;
mod direct;
mod display;
pub mod error;
mod event;
mod file;
mod help;
#[cfg(feature = "keymap-file")]
mod keymap_file;
#[macro_use]
mod keymap_macro;
mod keymaps;
mod line;
mod line_cache;
mod line_drawing;
mod overstrike;
mod progress;
mod prompt;
mod prompt_history;
mod refresh;
mod ruler;
mod screen;
mod search;
mod util;

use bindings::Keymap;
use config::{Config, InterfaceMode, KeymapConfig, WrappingMode};
use event::EventStream;
use file::File;
use progress::Progress;

/// The main pager state.
pub struct Pager {
    /// The Terminal.
    term: SystemTerminal,

    /// The Terminal's capabilites.
    caps: Capabilities,

    /// Event Stream to process.
    events: EventStream,

    /// Files to load.
    files: Vec<File>,

    /// Error file mapping.  Maps file indices to the associated error files.
    error_files: VecMap<File>,

    /// Progress indicators to display.
    progress: Option<Progress>,

    /// Configuration.
    config: Config,
}

/// Determine terminal capabilities.
fn termcaps() -> Result<Capabilities> {
    // Get terminal capabilities from the environment, but disable mouse
    // reporting, as we don't want to change the terminal's mouse handling.
    // Enable TrueColor support, which is backwards compatible with 16
    // or 256 colors. Applications can still limit themselves to 16 or
    // 256 colors if they want.
    let hints = ProbeHints::new_from_env()
        .color_level(Some(ColorLevel::TrueColor))
        .mouse_reporting(Some(false));
    let caps = Capabilities::new_with_hints(hints).map_err(Error::Termwiz)?;
    if cfg!(unix) && caps.terminfo_db().is_none() {
        Err(Error::TerminfoDatabaseMissing)
    } else {
        Ok(caps)
    }
}

impl Pager {
    /// Build a `Pager` using the system terminal.
    pub fn new_using_system_terminal() -> Result<Self> {
        Self::new_with_terminal_func(move |caps| SystemTerminal::new(caps).map_err(Error::Termwiz))
    }

    /// Build a `Pager` using the system stdio.
    pub fn new_using_stdio() -> Result<Self> {
        Self::new_with_terminal_func(move |caps| {
            SystemTerminal::new_from_stdio(caps).map_err(Error::Termwiz)
        })
    }

    #[cfg(unix)]
    /// Build a `Pager` using the specified terminal input and output.
    pub fn new_with_input_output(
        input: &impl std::os::unix::io::AsRawFd,
        output: &impl std::os::unix::io::AsRawFd,
    ) -> Result<Self> {
        Self::new_with_terminal_func(move |caps| {
            SystemTerminal::new_with(caps, input, output).map_err(Error::Termwiz)
        })
    }

    #[cfg(windows)]
    /// Build a `Pager` using the specified terminal input and output.
    pub fn new_with_input_output(
        input: impl std::io::Read + termwiz::istty::IsTty + std::os::windows::io::AsRawHandle,
        output: impl std::io::Write + termwiz::istty::IsTty + std::os::windows::io::AsRawHandle,
    ) -> Result<Self> {
        Self::new_with_terminal_func(move |caps| SystemTerminal::new_with(caps, input, output))
    }

    fn new_with_terminal_func(
        create_term: impl FnOnce(Capabilities) -> Result<SystemTerminal>,
    ) -> Result<Self> {
        let caps = termcaps()?;
        let mut term = create_term(caps.clone())?;
        term.set_raw_mode().map_err(Error::Termwiz)?;

        let events = EventStream::new(term.waker());
        let files = Vec::new();
        let error_files = VecMap::new();
        let progress = None;
        let config = Config::from_config_file().with_env();

        Ok(Self {
            term,
            caps,
            events,
            files,
            error_files,
            progress,
            config,
        })
    }

    /// Add an output file to be paged.
    pub fn add_output_stream(
        &mut self,
        stream: impl Read + Send + 'static,
        title: &str,
    ) -> Result<&mut Self> {
        let index = self.files.len();
        let event_sender = self.events.sender();
        let file = File::new_streamed(index, stream, title, event_sender)?;
        self.files.push(file);
        Ok(self)
    }

    /// Attach an error stream to the previously added output stream.
    pub fn add_error_stream(
        &mut self,
        stream: impl Read + Send + 'static,
        title: &str,
    ) -> Result<&mut Self> {
        let index = self.files.len();
        let event_sender = self.events.sender();
        let file = File::new_streamed(index, stream, title, event_sender)?;
        if let Some(out_file) = self.files.last() {
            self.error_files.insert(out_file.index(), file.clone());
        }
        self.files.push(file);
        Ok(self)
    }

    /// Attach a file from disk.
    pub fn add_output_file(&mut self, filename: &OsStr) -> Result<&mut Self> {
        let index = self.files.len();
        let event_sender = self.events.sender();
        let file = File::new_file(index, filename, event_sender)?;
        self.files.push(file);
        Ok(self)
    }

    /// Attach the output and error streams from a subprocess.
    pub fn add_subprocess<I, S>(
        &mut self,
        command: &OsStr,
        args: I,
        title: &str,
    ) -> Result<&mut Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let index = self.files.len();
        let event_sender = self.events.sender();
        let (out_file, err_file) = File::new_command(index, command, args, title, event_sender)?;
        self.error_files.insert(index, err_file.clone());
        self.files.push(out_file);
        self.files.push(err_file);
        Ok(self)
    }

    /// Set the progress stream.
    pub fn set_progress_stream(&mut self, stream: impl Read + Send + 'static) -> &mut Self {
        let event_sender = self.events.sender();
        self.progress = Some(Progress::new(stream, event_sender));
        self
    }

    /// Set when to use full screen mode. See [`InterfaceMode`] for details.
    pub fn set_interface_mode(&mut self, value: impl Into<InterfaceMode>) -> &mut Self {
        self.config.interface_mode = value.into();
        self
    }

    /// Set whether scrolling can past end of file.
    pub fn set_scroll_past_eof(&mut self, value: bool) -> &mut Self {
        self.config.scroll_past_eof = value;
        self
    }

    /// Set how many lines to read ahead.
    pub fn set_read_ahead_lines(&mut self, lines: usize) -> &mut Self {
        self.config.read_ahead_lines = lines;
        self
    }

    /// Set default wrapping mode. See [`WrappingMode`] for details.
    pub fn set_wrapping_mode(&mut self, value: impl Into<WrappingMode>) -> &mut Self {
        self.config.wrapping_mode = value.into();
        self
    }

    /// Set keymap name.
    pub fn set_keymap_name(&mut self, keymap: impl Into<String>) -> &mut Self {
        self.config.keymap = KeymapConfig::Name(keymap.into());
        self
    }

    /// Set keymap.
    pub fn set_keymap(&mut self, keymap: Keymap) -> &mut Self {
        self.config.keymap = KeymapConfig::Keymap(Arc::new(keymap));
        self
    }

    /// Run Stream Pager.
    pub fn run(self) -> Result<()> {
        display::start(
            self.term,
            self.caps,
            self.events,
            self.files,
            self.error_files,
            self.progress,
            self.config,
        )
    }
}
