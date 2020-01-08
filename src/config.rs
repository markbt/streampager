//! Configuration that affects Pager behaviors.

use std::time::Duration;

/// Specify what interface to use.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum InterfaceMode {
    /// The full screen terminal interface.
    ///
    /// Support text search and other operations.
    ///
    /// Use the alternate screen. The pager UI will disappear completely at
    /// exit (except for terminals without alternate screen support).
    ///
    /// Similar to external command `less` without flags. This is the default.
    FullScreen,

    /// The minimal interface. Output goes to the terminal directly.
    ///
    /// Does not support text search or other fancy operations.
    ///
    /// Does not use the alternate screen. Content will be kept in the terminal
    /// at exit.
    ///
    /// Error messages and progress messages are printed after
    /// outputs.
    ///
    /// Similar to shell command `cat` without buffering.
    Cat,

    /// Hybrid: `Cat` first, `FullScreen` next.
    ///
    /// `Cat` is used initially. When content exceeds one screen, switch to the
    /// `FullScreen` interface.
    ///
    /// Unlike `FullScreen` or `Delayed`, skip initializing the alternate
    /// screen. This is because the initial `Cat` might have "polluted"
    /// the terminal.
    ///
    /// Similar to external command `less -F -X`.
    Hybrid,

    /// Wait to decide.
    ///
    /// If output completes in the delayed time, and is within one screen, print
    /// the output and exit. Otherwise, enter the `FullScreen` interface.
    ///
    /// Unlike `Hybrid`, output is buffered in memory. So the terminal is not
    /// "polluted" and the alternate screen is used for the `FullScreen`
    /// interface.
    ///
    /// If duration is set to infinite, similar to external command `less -F`.
    /// If duration is set to 0, similar to `FullScreen`.
    Delayed(Duration),
}

impl Default for InterfaceMode {
    fn default() -> Self {
        Self::FullScreen
    }
}

impl From<&str> for InterfaceMode {
    fn from(value: &str) -> InterfaceMode {
        match value.to_lowercase().as_ref() {
            "full" | "fullscreen" | "" => InterfaceMode::FullScreen,
            "cat" => InterfaceMode::Cat,
            "hybrid" => InterfaceMode::Hybrid,
            s if s.starts_with("delayed") => {
                let duration = s.rsplit(":").nth(0).unwrap_or("inf");
                let duration = if duration.ends_with("ms") {
                    // ex. delayed:100ms
                    Duration::from_millis(duration.trim_end_matches("ms").parse().unwrap_or(0))
                } else {
                    // ex. delayed:1s, delayed:1, delayed
                    Duration::from_secs(duration.trim_end_matches("s").parse().unwrap_or(1 << 30))
                };
                InterfaceMode::Delayed(duration)
            }
            _ => InterfaceMode::default(),
        }
    }
}

/// A group of configurations.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Config {
    /// Specify when to use fullscreen.
    pub interface_mode: InterfaceMode,
}

impl Config {
    /// Construct [`Config`] from environment variables.
    pub fn from_env() -> Self {
        use std::env::var;
        Self {
            interface_mode: var("SP_INTERFACE_MODE")
                .ok()
                .map(|s| InterfaceMode::from(s.as_ref()))
                .unwrap_or(Default::default()),
        }
    }
}
