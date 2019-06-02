//! Files.
use failure::{Error, ResultExt};
use memmap::Mmap;
use std::borrow::Cow;
use std::cmp::min;
use std::ffi::OsStr;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::io::{FromRawFd, IntoRawFd, RawFd};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;

use crate::buffer::Buffer;
use crate::event::{Event, EventSender};

/// Buffer size to use when loading and parsing files.  This is also the block
/// size when parsing memory mapped files.
const BUFFER_SIZE: usize = 1024 * 1024;

/// The data content of the file.
#[derive(Clone)]
enum FileData {
    /// Data content is being streamed from an input stream, and stored in a
    /// vector of buffers.
    Streamed { buffers: Arc<RwLock<Vec<Buffer>>> },

    /// Data content has been memory mapped.
    Mapped { mmap: Arc<Mmap> },

    /// File is empty.
    Empty,

    /// Static content.
    Static { data: &'static [u8] },
}

/// Metadata about a file that is being loaded.
struct FileMeta {
    /// The index of the file.
    index: usize,

    /// The loaded file's title.  Usually its name.
    title: String,

    /// Information about the file.
    info: RwLock<Vec<String>>,

    /// The length of the file that has been parsed.
    length: AtomicUsize,

    /// The offset of each newline in the file.
    newlines: RwLock<Vec<usize>>,

    /// Set to true when the file has been loaded and parsed.
    finished: AtomicBool,

    /// The most recent error encountered when loading the file.
    error: RwLock<Option<Error>>,
}

impl FileMeta {
    /// Create new file metadata.
    fn new(index: usize, title: String) -> FileMeta {
        FileMeta {
            index,
            title,
            info: RwLock::new(Vec::new()),
            length: AtomicUsize::new(0usize),
            newlines: RwLock::new(Vec::new()),
            finished: AtomicBool::new(false),
            error: RwLock::new(None),
        }
    }
}

impl FileData {
    /// Create a new streamed file.
    ///
    /// A background thread is started to read from `input` and store the
    /// content in buffers.  Metadata about loading is written to `meta`.
    ///
    /// Returns `FileData` containing the buffers that the background thread
    /// is loading into.
    fn new_streamed(
        mut input: impl Read + Send + 'static,
        meta: Arc<FileMeta>,
        event_sender: EventSender,
    ) -> Result<(FileData), Error> {
        let buffers = Arc::new(RwLock::new(Vec::new()));
        thread::spawn({
            let buffers = buffers.clone();
            move || {
                let mut offset = 0usize;
                let mut total_buffer_size = 0usize;
                loop {
                    // Check if a new buffer must be allocated.
                    if offset == total_buffer_size {
                        let mut buffers = buffers.write().unwrap();
                        buffers.push(Buffer::new(BUFFER_SIZE));
                        total_buffer_size += BUFFER_SIZE;
                    }
                    let buffers = buffers.read().unwrap();
                    let mut write = buffers.last().unwrap().write();
                    match input.read(&mut write) {
                        Ok(0) => {
                            // The end of the file has been reached.  Complete.
                            meta.finished.store(true, Ordering::SeqCst);
                            event_sender.send(Event::Loaded(meta.index)).unwrap();
                            return;
                        }
                        Ok(len) => {
                            // Some data has been read.  Parse its newlines.
                            let mut newlines = meta.newlines.write().unwrap();
                            for i in 0..len {
                                if write[i] == b'\n' {
                                    newlines.push(offset + i);
                                }
                            }
                            offset += len;
                            write.written(len);
                            meta.length.fetch_add(len, Ordering::SeqCst);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(e) => {
                            let mut error = meta.error.write().unwrap();
                            *error = Some(e.into());
                        }
                    }
                }
            }
        });
        Ok(FileData::Streamed { buffers })
    }

    /// Create a new memory mapped file.
    ///
    /// The `file` is memory mapped and then a background thread is started to
    /// parse the newlines in the file.  The parsing progress is stored in
    /// `meta`.
    ///
    /// Returns `FileData` containing the memory map.
    fn new_mapped(
        file: std::fs::File,
        meta: Arc<FileMeta>,
        event_sender: EventSender,
    ) -> Result<FileData, Error> {
        // We can't mmap empty files, so just return an empty filedata if the
        // file's length is 0.
        if file.metadata()?.len() == 0 {
            meta.finished.store(true, Ordering::SeqCst);
            event_sender.send(Event::Loaded(meta.index))?;
            return Ok(FileData::Empty);
        }
        let mmap = Arc::new(unsafe { Mmap::map(&file)? });
        thread::spawn({
            let mmap = mmap.clone();
            move || {
                let len = mmap.len();
                let blocks = (len + BUFFER_SIZE - 1) / BUFFER_SIZE;
                for block in 0..blocks {
                    let mut newlines = meta.newlines.write().unwrap();
                    for i in block * BUFFER_SIZE..min((block + 1) * BUFFER_SIZE, len) {
                        if mmap[i] == b'\n' {
                            newlines.push(i);
                        }
                    }
                }
                meta.length.store(len, Ordering::SeqCst);
                meta.finished.store(true, Ordering::SeqCst);
                event_sender.send(Event::Loaded(meta.index)).unwrap();
            }
        });
        Ok(FileData::Mapped { mmap })
    }

    /// Create a new file from static data.
    ///
    /// Returns `FileData` containing the static data.
    fn new_static(
        data: &'static [u8],
        meta: Arc<FileMeta>,
        event_sender: EventSender,
    ) -> Result<FileData, Error> {
        thread::spawn({
            move || {
                let len = data.len();
                let blocks = (len + BUFFER_SIZE - 1) / BUFFER_SIZE;
                for block in 0..blocks {
                    let mut newlines = meta.newlines.write().unwrap();
                    for i in block * BUFFER_SIZE..min((block + 1) * BUFFER_SIZE, len) {
                        if data[i] == b'\n' {
                            newlines.push(i);
                        }
                    }
                }
                meta.length.store(len, Ordering::SeqCst);
                meta.finished.store(true, Ordering::SeqCst);
                event_sender.send(Event::Loaded(meta.index)).unwrap();
            }
        });
        Ok(FileData::Static { data })
    }

    /// Runs the `call` function, passing it a slice of the data from `start` to `end`.
    /// Tries to avoid copying the data if possible.
    fn with_slice<T, F>(&self, start: usize, end: usize, mut call: F) -> T
    where
        F: FnMut(Cow<'_, [u8]>) -> T,
    {
        match self {
            FileData::Streamed { buffers } => {
                let start_buffer = start / BUFFER_SIZE;
                let end_buffer = (end - 1) / BUFFER_SIZE;
                let buffers = buffers.read().unwrap();
                if start_buffer == end_buffer {
                    let data = buffers[start_buffer].read();
                    call(Cow::Borrowed(
                        &data[start % BUFFER_SIZE..=(end - 1) % BUFFER_SIZE],
                    ))
                } else {
                    // The data spans multiple buffers, so we must make a copy to make it contiguous.
                    let mut v = Vec::with_capacity(end - start);
                    v.extend_from_slice(&buffers[start_buffer].read()[start % BUFFER_SIZE..]);
                    for b in start_buffer + 1..end_buffer {
                        v.extend_from_slice(&buffers[b].read()[..]);
                    }
                    v.extend_from_slice(&buffers[end_buffer].read()[..=(end - 1) % BUFFER_SIZE]);
                    call(Cow::Owned(v))
                }
            }
            FileData::Mapped { mmap } => call(Cow::Borrowed(&mmap[start..end])),
            FileData::Empty => call(Cow::Borrowed(&[])),
            FileData::Static { data } => call(Cow::Borrowed(&data[start..end])),
        }
    }
}

/// A loaded file.
#[derive(Clone)]
pub(crate) struct File {
    /// The data for the file.
    data: FileData,

    /// Metadata about the loading of the file.
    meta: Arc<FileMeta>,
}

impl File {
    /// Load stdin.
    pub(crate) fn new_stdin(
        index: usize,
        title: &str,
        event_sender: EventSender,
    ) -> Result<File, Error> {
        let meta = Arc::new(FileMeta::new(index, title.to_string()));
        let data = FileData::new_streamed(std::io::stdin(), meta.clone(), event_sender)?;
        Ok(File { data, meta })
    }

    /// Load an input fd
    pub(crate) fn new_fd(
        index: usize,
        fd: RawFd,
        title: &str,
        event_sender: EventSender,
    ) -> Result<File, Error> {
        let meta = Arc::new(FileMeta::new(index, title.to_string()));
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        let data = FileData::new_streamed(file, meta.clone(), event_sender)?;
        Ok(File { data, meta })
    }

    /// Load a file by memory mapping it if possible.
    pub(crate) fn new_mapped(
        index: usize,
        filename: &OsStr,
        event_sender: EventSender,
    ) -> Result<File, Error> {
        let title = filename.to_string_lossy().into_owned();
        let meta = Arc::new(FileMeta::new(index, title.clone()));
        let mut file = std::fs::File::open(filename).context(title)?;
        // Determine whether this file is a real file, or some kind of pipe, by
        // attempting to do a no-op seek.  If it fails, assume we can't mmap
        // it.
        let data = match file.seek(SeekFrom::Current(0)) {
            Ok(_) => FileData::new_mapped(file, meta.clone(), event_sender)?,
            Err(_) => FileData::new_streamed(file, meta.clone(), event_sender)?,
        };
        Ok(File { data, meta })
    }

    /// Load the output and error of a command
    pub(crate) fn new_command<I, S>(
        index: usize,
        command: &OsStr,
        args: I,
        title: &str,
        event_sender: EventSender,
    ) -> Result<(File, File), Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let title_err = format!("STDERR for {}", title);
        let mut process = Command::new(command)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(command.to_string_lossy().into_owned())?;
        let out_fd = process.stdout.take().unwrap().into_raw_fd();
        let err_fd = process.stderr.take().unwrap().into_raw_fd();
        let out_file = File::new_fd(index, out_fd, &title, event_sender.clone())?;
        let err_file = File::new_fd(index + 1, err_fd, &title_err, event_sender.clone())?;
        thread::spawn({
            let out_file = out_file.clone();
            move || {
                if let Ok(rc) = process.wait() {
                    if !rc.success() {
                        let mut info = out_file.meta.info.write().unwrap();
                        match rc.code() {
                            Some(code) => info.push(format!("rc: {}", code)),
                            None => info.push("killed!".to_string()),
                        }
                        event_sender.send(Event::RefreshOverlay).unwrap();
                    }
                }
            }
        });
        Ok((out_file, err_file))
    }

    /// Load a file from static data.
    pub(crate) fn new_static(
        index: usize,
        title: &str,
        data: &'static [u8],
        event_sender: EventSender,
    ) -> Result<File, Error> {
        let meta = Arc::new(FileMeta::new(index, title.to_string()));
        let data = FileData::new_static(data, meta.clone(), event_sender)?;
        Ok(File { data, meta })
    }

    /// The file's index.
    pub(crate) fn index(&self) -> usize {
        self.meta.index
    }

    /// The file's title.
    pub(crate) fn title(&self) -> &str {
        &self.meta.title
    }

    /// The file's info.
    pub(crate) fn info(&self) -> String {
        let info = self.meta.info.read().unwrap();
        info.join(" ")
    }

    /// True once the file is loaded and all newlines have been parsed.
    pub(crate) fn loaded(&self) -> bool {
        self.meta.finished.load(Ordering::SeqCst)
    }

    /// Returns the number of lines in the file.
    pub(crate) fn lines(&self) -> usize {
        let newlines = self.meta.newlines.read().unwrap();
        let mut lines = newlines.len();
        let after_last_newline_offset = if lines == 0 {
            0
        } else {
            newlines[lines - 1] + 1
        };
        if self.meta.length.load(Ordering::SeqCst) > after_last_newline_offset {
            lines += 1;
        }
        lines
    }

    /// Returns the maximum width in characters of line numbers for this file.
    pub(crate) fn line_number_width(&self) -> usize {
        let lines = self.lines();
        let mut lw = 1;
        let mut ll = 10;
        while ll <= lines {
            ll *= 10;
            lw += 1;
        }
        lw
    }

    /// Runs the `call` function, passing it the contents of line `index`.
    /// Tries to avoid copying the data if possible, however the borrowed
    /// line only lasts as long as the function call.
    pub(crate) fn with_line<T, F>(&self, index: usize, call: F) -> Option<T>
    where
        F: FnMut(Cow<'_, [u8]>) -> T,
    {
        let newlines = self.meta.newlines.read().unwrap();
        if index > newlines.len() {
            return None;
        }
        let start = if index == 0 {
            0
        } else {
            newlines[index - 1] + 1
        };
        let end = if index < newlines.len() {
            newlines[index] + 1
        } else {
            self.meta.length.load(Ordering::SeqCst)
        };
        if start == end {
            return None;
        }
        Some(self.data.with_slice(start, end, call))
    }
}
