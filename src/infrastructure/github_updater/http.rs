//! The HTTP side of updating: a client with a timeout and User-Agent, the
//! release-metadata request, and a streaming download that reports progress.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use super::{DOWNLOAD_TIMEOUT, USER_AGENT};
use crate::domain::update::UpdateError;

fn client(timeout: Duration) -> Result<reqwest::blocking::Client, UpdateError> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| UpdateError::Network(e.to_string()))
}

pub(super) fn get_json(url: &str, timeout: Duration) -> Result<serde_json::Value, UpdateError> {
    let response = client(timeout)?
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    if !response.status().is_success() {
        return Err(UpdateError::Network(format!(
            "GitHub answered {}",
            response.status()
        )));
    }

    response
        .json()
        .map_err(|e| UpdateError::Network(format!("unreadable release data: {e}")))
}

pub(super) fn download(
    url: &str,
    dest: &Path,
    expected_size: Option<u64>,
    on_progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<(), UpdateError> {
    let mut response = client(DOWNLOAD_TIMEOUT)?
        .get(url)
        .header(reqwest::header::ACCEPT, "application/octet-stream")
        .send()
        .map_err(|e| UpdateError::Download(e.to_string()))?;

    if !response.status().is_success() {
        return Err(UpdateError::Download(format!(
            "the download answered {}",
            response.status()
        )));
    }

    let total = expected_size.or_else(|| response.content_length());

    let file = fs::File::create(dest)
        .map_err(|e| UpdateError::Download(format!("could not write the download: {e}")))?;

    let mut counter = ProgressWriter {
        inner: io::BufWriter::new(file),
        written: 0,
        total,
        report: on_progress,
    };

    on_progress(0, total);
    io::copy(&mut response, &mut counter)
        .map_err(|e| UpdateError::Download(format!("the download stopped early: {e}")))?;

    counter
        .inner
        .flush()
        .map_err(|e| UpdateError::Download(format!("could not finish writing: {e}")))?;
    Ok(())
}

struct ProgressWriter<'a, W: Write> {
    inner: W,
    written: u64,
    total: Option<u64>,
    report: &'a (dyn Fn(u64, Option<u64>) + Send + Sync),
}

impl<W: Write> Write for ProgressWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.written += n as u64;
        (self.report)(self.written, self.total);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counting_writer_reports_every_chunk_and_totals_correctly() {
        use std::sync::Mutex;

        let seen: Mutex<Vec<(u64, Option<u64>)>> = Mutex::new(Vec::new());
        let report = |received: u64, total: Option<u64>| {
            seen.lock().unwrap().push((received, total));
        };

        let mut sink = ProgressWriter {
            inner: Vec::new(),
            written: 0,
            total: Some(6),
            report: &report,
        };
        sink.write_all(b"abc").unwrap();
        sink.write_all(b"def").unwrap();

        assert_eq!(sink.written, 6);
        assert_eq!(sink.inner, b"abcdef");
        assert_eq!(
            *seen.lock().unwrap(),
            vec![(3, Some(6)), (6, Some(6))],
            "bytes are cumulative, not per-chunk"
        );
    }
}
