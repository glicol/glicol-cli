use std::{
    io, mem,
    sync::{Arc, Mutex, MutexGuard},
};

use ringbuf::{HeapRb, Rb};

/// Register a tracing subscriber outputting to a [`ShareableRecentLinesBuffer`] with given capacity.
pub(super) fn register_tracer(capacity: usize) -> ShareableRecentLinesBuffer {
    let buffer = ShareableRecentLinesBuffer(Arc::new(Mutex::new(RecentLinesBuffer::new(capacity))));

    tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_writer(buffer.clone())
        .init();

    buffer
}

/// Keep most recent lines written to it.
pub(crate) struct RecentLinesBuffer {
    /// Lines kept
    lines: HeapRb<String>,

    /// Line currently being written
    current_line: Vec<u8>,
}

impl RecentLinesBuffer {
    /// Create a writer keeping at most `capacity` lines
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: HeapRb::new(capacity),
            current_line: Vec::new(),
        }
    }

    /// Get the most recent lines
    pub fn read(&self) -> impl Iterator<Item = &String> {
        self.lines.iter()
    }
}

impl io::Write for RecentLinesBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut lines = buf.split(|b| *b == b'\n');
        let mut read = 0;

        // complete current line
        let first_line = lines.next().expect("at least a single elem");
        read += first_line.len();
        self.current_line.extend_from_slice(first_line);

        for line in lines {
            read += 1 + line.len(); // with separator

            let previous_line = mem::replace(&mut self.current_line, Vec::from(line));
            self.lines.push_overwrite(
                String::from_utf8(previous_line)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
            );
        }

        Ok(read)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(()) // nothing to do
    }
}

// can be inlined to inner type after merge of
// https://github.com/tokio-rs/tracing/pull/2760
#[derive(Clone)]
pub(crate) struct ShareableRecentLinesBuffer(pub(super) Arc<Mutex<RecentLinesBuffer>>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for ShareableRecentLinesBuffer {
    type Writer = MutexGuardWriter<'a, RecentLinesBuffer>;

    fn make_writer(&'a self) -> Self::Writer {
        MutexGuardWriter(self.0.lock().expect("lock poisoned"))
    }
}

// taken from tracing-subscriber/src/fmt/writer.rs
// can be removed when above is not needed anymore
pub(crate) struct MutexGuardWriter<'a, W: io::Write + 'a>(MutexGuard<'a, W>);

impl<'a, W> io::Write for MutexGuardWriter<'a, W>
where
    W: io::Write + 'a,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.0.write_vectored(bufs)
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.0.write_all(buf)
    }

    fn write_fmt(&mut self, fmt: std::fmt::Arguments<'_>) -> io::Result<()> {
        self.0.write_fmt(fmt)
    }
}

#[cfg(test)]
mod tests {
    use super::RecentLinesBuffer;

    use std::io::Write;

    #[test]
    fn only_show_full_lines() {
        let mut last_written = RecentLinesBuffer::new(10);

        write!(&mut last_written, "not ended line").unwrap();
        assert!(last_written.read().next().is_none());

        writeln!(&mut last_written, " is now ended").unwrap();
        assert_eq!(
            last_written.read().collect::<Vec<_>>(),
            vec!["not ended line is now ended"]
        );
    }

    #[test]
    fn keep_only_recent_lines() {
        let mut last_written = RecentLinesBuffer::new(3);

        writeln!(&mut last_written, "a").unwrap();
        writeln!(&mut last_written, "b").unwrap();
        writeln!(&mut last_written, "c").unwrap();
        assert_eq!(last_written.read().collect::<Vec<_>>(), vec!["a", "b", "c"]);

        writeln!(&mut last_written, "d").unwrap();
        assert_eq!(last_written.read().collect::<Vec<_>>(), vec!["b", "c", "d"]);
    }
}
