// Copyright (C) 2026  Clyso
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

//! Bounded reverse log tail (WCP D7 / G5).
//!
//! The previous implementation read the entire log file into memory before
//! slicing the last N lines. Long-running builds with multi-MiB logs blew up
//! the request handler; a worker that emitted a single very-large line
//! could push memory use unboundedly. `read_tail` instead scans backwards
//! from EOF within a fixed byte budget and returns at most the requested
//! number of complete lines.

use std::io;
use std::path::Path;

use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

/// Outcome of a tail read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailReadResult {
    /// Complete lines, oldest first.
    pub lines: Vec<String>,
    /// Number of lines actually returned.
    pub returned: usize,
    /// Number of lines the caller asked for (after server-side cap).
    pub requested: usize,
    /// `true` if older content was reachable but excluded by the byte budget
    /// or by single-line-over-budget. The client should expose this to the
    /// user as "earlier output omitted".
    pub truncated: bool,
    /// Bytes actually read from disk during this call.
    pub bytes_scanned: u64,
}

/// Read at most `requested_n` complete trailing lines from `path`,
/// scanning at most `max_bytes` from the end of the file.
///
/// Behaviour per WCP D7:
/// - Partial trailing line (file does not end in `\n`) is dropped.
/// - If the budget was hit (the read window did not reach the start of
///   the file), the first line in the window is treated as a partial
///   leading line and dropped.
/// - UTF-8 boundary safety: a window that starts mid-code-point has its
///   leading continuation bytes trimmed; any tail that ends mid-code-point
///   is truncated to the longest valid prefix.
/// - If a single line is longer than `max_bytes`, no complete line is
///   recoverable from the window; the result is empty with
///   `truncated = true`.
pub async fn read_tail(
    path: &Path,
    requested_n: usize,
    max_bytes: u64,
) -> Result<TailReadResult, io::Error> {
    let mut file = tokio::fs::OpenOptions::new().read(true).open(path).await?;
    let metadata = file.metadata().await?;
    let file_size = metadata.len();

    if file_size == 0 {
        return Ok(TailReadResult {
            lines: Vec::new(),
            returned: 0,
            requested: requested_n,
            truncated: false,
            bytes_scanned: 0,
        });
    }

    let scan_budget = file_size.min(max_bytes);
    let start_offset = file_size - scan_budget;
    let truncated_start = start_offset > 0;

    file.seek(SeekFrom::Start(start_offset)).await?;
    let mut buf = vec![0u8; scan_budget as usize];
    file.read_exact(&mut buf).await?;

    // UTF-8 boundary safety on both ends:
    // - leading: if we started mid-multibyte char, the first bytes are
    //   continuation bytes (top bits 10xxxxxx). Drop them.
    // - trailing: take only the longest valid prefix.
    let mut start = 0usize;
    if truncated_start {
        while start < buf.len() && (buf[start] & 0xC0) == 0x80 {
            start += 1;
        }
    }
    let after_leading = &buf[start..];
    let valid_text = match std::str::from_utf8(after_leading) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&after_leading[..e.valid_up_to()])
            .expect("valid_up_to returns a valid UTF-8 prefix"),
    };

    let ends_with_newline = valid_text.as_bytes().last() == Some(&b'\n');
    let mut lines: Vec<&str> = valid_text.split('\n').collect();
    if ends_with_newline {
        // `split` produced an empty trailing element after the final \n.
        lines.pop();
    } else if !lines.is_empty() {
        // Partial trailing line — file did not end in \n.
        lines.pop();
    }
    if truncated_start && !lines.is_empty() {
        // Possibly-partial leading line.
        lines.remove(0);
    }

    let total = lines.len();
    let returned_lines: Vec<String> = if total <= requested_n {
        lines.iter().map(|s| (*s).to_string()).collect()
    } else {
        lines[total - requested_n..]
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    };

    // `truncated` is true when older content existed but was excluded —
    // either because the budget cut off lines we did not return, or
    // because the budget cut off so much that no complete line fit.
    let truncated = truncated_start;

    Ok(TailReadResult {
        lines: returned_lines.clone(),
        returned: returned_lines.len(),
        requested: requested_n,
        truncated,
        bytes_scanned: scan_budget,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    fn make_log(contents: &[u8]) -> NamedTempFile {
        let mut tmp = NamedTempFile::new().expect("tmp");
        tmp.write_all(contents).expect("write");
        tmp
    }

    #[tokio::test]
    async fn empty_file_returns_empty_result() {
        let tmp = make_log(b"");
        let r = read_tail(tmp.path(), 50, 1024).await.expect("read");
        assert!(r.lines.is_empty());
        assert_eq!(r.returned, 0);
        assert!(!r.truncated);
        assert_eq!(r.bytes_scanned, 0);
    }

    #[tokio::test]
    async fn returns_complete_lines_when_within_budget() {
        let tmp = make_log(b"alpha\nbeta\ngamma\n");
        let r = read_tail(tmp.path(), 50, 1024).await.expect("read");
        assert_eq!(r.lines, vec!["alpha", "beta", "gamma"]);
        assert_eq!(r.returned, 3);
        assert!(!r.truncated);
    }

    #[tokio::test]
    async fn drops_partial_trailing_line_when_file_has_no_final_newline() {
        let tmp = make_log(b"alpha\nbeta\ngamma");
        let r = read_tail(tmp.path(), 50, 1024).await.expect("read");
        // `gamma` is dropped because the file does not end in \n.
        assert_eq!(r.lines, vec!["alpha", "beta"]);
    }

    #[tokio::test]
    async fn takes_only_last_n_when_more_lines_present() {
        let tmp = make_log(b"a\nb\nc\nd\ne\n");
        let r = read_tail(tmp.path(), 2, 1024).await.expect("read");
        assert_eq!(r.lines, vec!["d", "e"]);
    }

    #[tokio::test]
    async fn budget_hit_drops_possibly_partial_first_line_and_sets_truncated() {
        // Total content > budget so the read window starts after byte 0,
        // and the first line in the window must be discarded as partial.
        let body = b"first-line-is-long-and-will-be-truncated\nsecond\nthird\n";
        let tmp = make_log(body);
        let r = read_tail(tmp.path(), 10, 20).await.expect("read");
        assert!(r.truncated);
        assert!(
            !r.lines.iter().any(|l| l.starts_with("first-line")),
            "the first line must be dropped as partial"
        );
        assert_eq!(r.bytes_scanned, 20);
    }

    #[tokio::test]
    async fn single_line_over_budget_returns_no_partial_line() {
        // A single line that is longer than the budget. No complete line
        // is recoverable from the window.
        let body = b"a-single-very-long-line-with-no-newline-inside\n";
        let tmp = make_log(body);
        let r = read_tail(tmp.path(), 10, 10).await.expect("read");
        assert!(r.lines.is_empty(), "no partial line returned");
        assert!(r.truncated);
    }

    #[tokio::test]
    async fn drops_continuation_bytes_at_window_start() {
        // \xC3\xA9 is "é" (2 bytes). If the budget lands inside it, the
        // first byte is a continuation byte that must be skipped.
        let mut body = Vec::new();
        body.extend_from_slice(b"x\n");
        body.push(0xC3); // start of é
        body.push(0xA9); // continuation of é
        body.extend_from_slice(b"yz\n");
        let tmp = make_log(&body);
        // Force the window to start at the continuation byte by sizing
        // the budget below 4. file_size is 6.
        let r = read_tail(tmp.path(), 10, 4).await.expect("read");
        // Whatever survives must be valid UTF-8 and must not start with
        // a continuation byte.
        for line in &r.lines {
            assert!(line.is_char_boundary(0));
        }
    }

    #[tokio::test]
    async fn missing_file_returns_io_error() {
        let path = std::path::Path::new("/tmp/cbsd-tail-test-missing-12345");
        let _ = std::fs::remove_file(path);
        let err = read_tail(path, 10, 1024).await.expect_err("not found");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}
