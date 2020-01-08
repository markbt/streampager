//! Stream Pager
//!
//! A pager for streams.
#![warn(missing_docs)]

pub use anyhow::Result;
use anyhow::{anyhow, bail};
use std::ffi::OsStr;
use std::io::Read;
use termwiz::caps::{Capabilities, ProbeHintsBuilder};
use termwiz::terminal::{SystemTerminal, Terminal};
use vec_map::VecMap;

mod buffer;
mod command;
pub mod config;
mod direct;
mod display;
mod event;
mod file;
mod line;
mod line_cache;
mod overstrike;
mod progress;
mod prompt;
mod refresh;
mod screen;
mod search;

use config::{Config, InterfaceMode};
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

/// Determine terminal capabilities and open the terminal.
fn open_terminal() -> Result<(SystemTerminal, Capabilities)> {
    // Get terminal capabilities from the environment, but disable mouse
    // reporting, as we don't want to change the terminal's mouse handling.
    let caps = Capabilities::new_with_hints(
        ProbeHintsBuilder::new_from_env()
            .mouse_reporting(Some(false))
            .build()
            .map_err(|s| anyhow!(s))?,
    )?;
    if cfg!(unix) && caps.terminfo_db().is_none() {
        bail!("terminfo database not found (is $TERM correct?)");
    }
    let mut term = SystemTerminal::new(caps.clone())?;
    term.set_raw_mode()?;
    Ok((term, caps))
}

impl Pager {
    /// Build a `Pager` using the system terminal.
    pub fn new_using_system_terminal() -> Result<Pager> {
        let (term, caps) = open_terminal()?;
        let events = EventStream::new(term.waker());
        let files = Vec::new();
        let error_files = VecMap::new();
        let progress = None;
        let config = Config::from_env();

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
        let file = File::new_mapped(index, filename, event_sender)?;
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
