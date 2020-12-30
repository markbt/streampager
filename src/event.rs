//! Events.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use termwiz::input::InputEvent;
use termwiz::terminal::{Terminal, TerminalWaker};

use crate::error::Error;

/// An event.
///
/// Events drive most of the main processing of `sp`.  This includes user
/// input, state changes, and display refresh requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// An input event.
    Input(InputEvent),
    /// A file has finished loading.
    Loaded(usize),
    /// A file has started loading more data.
    Appending(usize),
    /// A file has started reloading.
    Reloading(usize),
    /// Render an update to the screen.
    Render,
    /// Refresh the whole screen.
    Refresh,
    /// Refresh the overlay.
    RefreshOverlay,
    /// A new progress display is available.
    Progress,
    /// Search has found the first match.
    SearchFirstMatch(usize),
    /// Search has finished.
    SearchFinished(usize),
}

#[derive(Debug, Clone)]
pub struct UniqueInstance(Arc<AtomicBool>);

impl UniqueInstance {
    pub(crate) fn new() -> UniqueInstance {
        UniqueInstance(Arc::new(AtomicBool::new(false)))
    }
}

pub enum Envelope {
    Normal(Event),
    Unique(Event, UniqueInstance),
}

/// An event sender endpoint.
#[derive(Clone)]
pub(crate) struct EventSender(mpsc::Sender<Envelope>, TerminalWaker);

impl EventSender {
    pub(crate) fn send(&self, event: Event) -> Result<(), Error> {
        self.0.send(Envelope::Normal(event))?;
        self.1.wake()?;
        Ok(())
    }
    pub(crate) fn send_unique(&self, event: Event, unique: &UniqueInstance) -> Result<(), Error> {
        if !unique.0.compare_and_swap(false, true, Ordering::SeqCst) {
            self.0.send(Envelope::Unique(event, unique.clone()))?;
            self.1.wake()?;
        }
        Ok(())
    }
}

/// An event stream.  This is a wrapper multi-producer, single-consumer
/// stream of `Event`s.
pub(crate) struct EventStream {
    send: mpsc::Sender<Envelope>,
    recv: mpsc::Receiver<Envelope>,
    waker: TerminalWaker,
}

impl EventStream {
    /// Create a new event stream.
    pub(crate) fn new(waker: TerminalWaker) -> EventStream {
        let (send, recv) = mpsc::channel();
        EventStream { send, recv, waker }
    }

    /// Create a sender for the event stream.
    pub(crate) fn sender(&self) -> EventSender {
        EventSender(self.send.clone(), self.waker.clone())
    }

    fn try_recv(&self) -> Result<Option<Event>, Error> {
        match self.recv.try_recv() {
            Ok(Envelope::Normal(event)) => Ok(Some(event)),
            Ok(Envelope::Unique(event, unique)) => {
                unique.0.store(false, Ordering::SeqCst);
                Ok(Some(event))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get an event, either from the event stream or from the terminal.
    pub(crate) fn get(
        &self,
        term: &mut dyn Terminal,
        wait: Option<Duration>,
    ) -> Result<Option<Event>, Error> {
        loop {
            if let Some(event) = self.try_recv()? {
                return Ok(Some(event));
            }

            // The queue is empty.  Try to get an input event from the terminal.
            match term.poll_input(wait).map_err(Error::Termwiz)? {
                Some(InputEvent::Wake) => {}
                Some(input_event) => return Ok(Some(Event::Input(input_event))),
                None => return Ok(None),
            }
        }
    }
}
