//! Buffer Cache.
use lru::LruCache;
use std::borrow::Cow;
use std::fs::File as StdFile;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::buffer::Buffer;
use crate::error::Error;

pub(crate) struct BufferCache {
    path: PathBuf,
    file: Option<StdFile>,
    cache: LruCache<usize, Buffer>,
    block_size: usize,
}

impl BufferCache {
    pub(crate) fn new<P: AsRef<Path>>(path: P, block_size: usize, capacity: usize) -> Self {
        let path = path.as_ref();
        BufferCache {
            path: path.to_path_buf(),
            file: None,
            cache: LruCache::new(capacity),
            block_size,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.cache.clear();
        self.file = None;
    }

    fn open_file(&mut self) -> Result<(), Error> {
        if self.file.is_none() {
            self.file = Some(StdFile::open(&self.path)?);
        }
        Ok(())
    }

    fn get_buffer(&mut self, start: usize, end: usize) -> Result<Option<&mut Buffer>, Error> {
        let block_index = start / self.block_size;
        let block_offset = start % self.block_size;
        self.open_file()?;
        if let Some(buffer) = self.cache.get_mut(&block_index) {
            fill_buffer(
                self.file.as_mut().expect("file is open"),
                buffer,
                start,
                block_offset,
                end - start,
            )?;
        } else {
            let mut buffer = Buffer::new(self.block_size);
            fill_buffer(
                self.file.as_mut().expect("file is open"),
                &mut buffer,
                block_index * self.block_size,
                0,
                self.block_size,
            )?;
            self.cache.put(block_index, buffer);
        }
        Ok(self.cache.get_mut(&block_index))
    }

    fn get_data(&mut self, start: usize, end: usize) -> Result<&[u8], Error> {
        let block_size = self.block_size;
        if let Some(buffer) = self.get_buffer(start, end)? {
            let data = buffer.read();
            let data_start = data.len().min(start % block_size);
            let data_end = (data.len()).min((end - 1) % block_size + 1);
            Ok(&data[data_start..data_end])
        } else {
            Ok(&[])
        }
    }

    pub(crate) fn with_slice<T, F>(
        &mut self,
        start: usize,
        end: usize,
        mut call: F,
    ) -> Result<T, Error>
    where
        F: FnMut(Cow<'_, [u8]>) -> T,
    {
        let start_block = start / self.block_size;
        let end_block = (end - 1) / self.block_size;
        if start_block == end_block {
            Ok(call(Cow::Borrowed(self.get_data(start, end)?)))
        } else {
            // The data spans multiple buffers, so we must make a copy to make it contiguous.
            // Ensure we fill in any gaps that might occur.
            let mut v = Vec::with_capacity(end - start);
            let first_end = (start_block + 1) * self.block_size;
            let first_slice = self.get_data(start, first_end)?;
            v.extend_from_slice(first_slice);
            v.resize(first_end - start, 0);
            for b in start_block + 1..end_block {
                let block_start = b * self.block_size;
                let block_end = block_start + self.block_size;
                let block_slice = self.get_data(block_start, block_end)?;
                v.extend_from_slice(block_slice);
                v.resize(block_end - start, 0);
            }
            let end_start = end_block * self.block_size;
            let end_slice = self.get_data(end_start, end)?;
            v.extend_from_slice(end_slice);
            v.resize(end - start, 0);
            Ok(call(Cow::Owned(v)))
        }
    }
}

fn fill_buffer(
    file: &mut StdFile,
    buffer: &mut Buffer,
    file_offset: usize,
    buffer_offset: usize,
    len: usize,
) -> Result<(), Error> {
    if buffer_offset + len <= buffer.available() {
        return Ok(());
    }
    let mut write = buffer.write();
    if file.seek(SeekFrom::Start(file_offset as u64)).is_err() {
        // Ignore seek errors, treat them as though the data isn't there.
        return Ok(());
    }
    loop {
        match file.read(&mut write) {
            Ok(0) => {
                // We're at the end of the file.  Nothing to do.
                break;
            }
            Ok(len) => {
                // Some data has been read.
                write.written(len);
                break;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => {
                return Err(e.into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn basic() -> Result<(), Error> {
        let mut t = NamedTempFile::new()?;
        write!(t, "HERE IS SOME DATA")?;
        t.flush()?;
        let mut c = BufferCache::new(t.path(), 4, 2);
        let mut read_range = |start, end| c.with_slice(start, end, |data| data.into_owned());
        assert_eq!(read_range(0, 4)?.as_slice(), b"HERE");
        assert_eq!(read_range(5, 7)?.as_slice(), b"IS");
        assert_eq!(read_range(3, 9)?.as_slice(), b"E IS S");
        assert_eq!(read_range(0, 17)?.as_slice(), b"HERE IS SOME DATA");
        assert_eq!(read_range(0, 20)?.as_slice(), b"HERE IS SOME DATA\0\0\0");
        Ok(())
    }
}
