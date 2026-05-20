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

//! Per-build output batcher.
//!
//! Drains the [`mpsc::UnboundedReceiver<String>`] populated by the
//! [`super::dispatch::BuildDispatchLayer`] and emits
//! `WorkerMessage::BuildOutput` frames over the existing
//! handler-side `mpsc::Sender<WorkerMessage>` queue.
//!
//! Batching contract — preserved bit-for-bit from the pre-cutover
//! `stream_output` implementation: flush every 50 lines OR every
//! 200 ms, whichever fires first. The final-flush-on-channel-close
//! rule (per the plan §"Subscriber layer design") is **load-
//! bearing**: when the per-build `UnboundedSender` is dropped (the
//! WS handler calls `BuildDispatch::unregister` after the
//! `runner::run` future returns), `recv().await` returns `None`;
//! the batcher MUST flush any pending partial batch before exiting
//! so no log lines are silently lost on build completion.

use std::time::Duration;

use cbsd_proto::build::BuildId;
use cbsd_proto::ws::WorkerMessage;
use tokio::sync::mpsc;

/// Maximum lines per batch before flushing.
const BATCH_MAX_LINES: usize = 50;

/// Maximum time to accumulate lines before flushing.
const BATCH_FLUSH_INTERVAL: Duration = Duration::from_millis(200);

/// Errors emitted by [`run_batcher`].
#[derive(Debug)]
pub(crate) enum BatcherError {
    /// Failed to send a `BuildOutput` message via the handler's
    /// `mpsc::Sender<WorkerMessage>` queue.
    Send(mpsc::error::SendError<WorkerMessage>),
}

impl std::fmt::Display for BatcherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Send(err) => write!(f, "failed to send output message: {err}"),
        }
    }
}

impl std::error::Error for BatcherError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Send(err) => Some(err),
        }
    }
}

/// Drain per-build event lines from `rx`, batch them, and forward
/// as `WorkerMessage::BuildOutput` frames via `sender`.
///
/// Returns when `rx` is closed (the dispatch handle dropped the
/// per-build sender). On exit, any pending partial batch is
/// flushed first.
pub(crate) async fn run_batcher(
    mut rx: mpsc::UnboundedReceiver<String>,
    build_id: BuildId,
    sender: mpsc::Sender<WorkerMessage>,
) -> Result<(), BatcherError> {
    let mut line_count: u64 = 0;
    let mut batch: Vec<String> = Vec::with_capacity(BATCH_MAX_LINES);
    let mut batch_start_seq: u64 = 0;

    let flush_timer = tokio::time::sleep(BATCH_FLUSH_INTERVAL);
    tokio::pin!(flush_timer);

    loop {
        tokio::select! {
            line = rx.recv() => {
                match line {
                    Some(line) => {
                        if batch.is_empty() {
                            batch_start_seq = line_count;
                            flush_timer.as_mut().reset(
                                tokio::time::Instant::now() + BATCH_FLUSH_INTERVAL,
                            );
                        }
                        batch.push(line);
                        line_count += 1;
                        if batch.len() >= BATCH_MAX_LINES {
                            flush_batch(build_id, &mut batch, batch_start_seq, &sender).await?;
                        }
                    }
                    None => {
                        // Channel closed — flush remaining and exit.
                        if !batch.is_empty() {
                            flush_batch(build_id, &mut batch, batch_start_seq, &sender).await?;
                        }
                        return Ok(());
                    }
                }
            }
            () = &mut flush_timer, if !batch.is_empty() => {
                flush_batch(build_id, &mut batch, batch_start_seq, &sender).await?;
                flush_timer.as_mut().reset(
                    tokio::time::Instant::now() + BATCH_FLUSH_INTERVAL,
                );
            }
        }
    }
}

/// Emit the current batch as a `BuildOutput` frame.
async fn flush_batch(
    build_id: BuildId,
    batch: &mut Vec<String>,
    start_seq: u64,
    sender: &mpsc::Sender<WorkerMessage>,
) -> Result<(), BatcherError> {
    let msg = WorkerMessage::BuildOutput {
        build_id,
        start_seq,
        lines: std::mem::take(batch),
    };
    sender.send(msg).await.map_err(BatcherError::Send)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn batcher_flushes_partial_batch_on_channel_close() {
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        let (out_tx, mut out_rx) = mpsc::channel::<WorkerMessage>(8);
        let build_id = BuildId(42);

        // Push 3 lines (less than BATCH_MAX_LINES); close.
        tx.send("line 1".into()).unwrap();
        tx.send("line 2".into()).unwrap();
        tx.send("line 3".into()).unwrap();
        drop(tx);

        let task = tokio::spawn(run_batcher(rx, build_id, out_tx));

        // The batcher's 200ms timer would also flush; assert we
        // get one BuildOutput with all 3 lines (either via the
        // timer or via the channel-close arm).
        let msg = out_rx.recv().await.expect("batcher emits one frame");
        match msg {
            WorkerMessage::BuildOutput {
                build_id: bid,
                start_seq,
                lines,
            } => {
                assert_eq!(bid, build_id);
                assert_eq!(start_seq, 0);
                assert_eq!(lines, vec!["line 1", "line 2", "line 3"]);
            }
            other => panic!("expected BuildOutput, got {other:?}"),
        }

        task.await.unwrap().expect("batcher returns Ok on close");
        // No further messages after close.
        assert!(out_rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn batcher_flushes_at_max_lines() {
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        let (out_tx, mut out_rx) = mpsc::channel::<WorkerMessage>(8);
        let build_id = BuildId(7);

        // Push exactly BATCH_MAX_LINES lines.
        for i in 0..BATCH_MAX_LINES {
            tx.send(format!("line {i}")).unwrap();
        }

        let task = tokio::spawn(run_batcher(rx, build_id, out_tx));

        let msg = out_rx.recv().await.expect("first batch flushed");
        match msg {
            WorkerMessage::BuildOutput {
                start_seq, lines, ..
            } => {
                assert_eq!(start_seq, 0);
                assert_eq!(lines.len(), BATCH_MAX_LINES);
            }
            other => panic!("expected BuildOutput, got {other:?}"),
        }

        // Close, ensure no additional messages and the batcher exits.
        drop(tx);
        task.await.unwrap().expect("batcher returns Ok");
        assert!(out_rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn batcher_exits_cleanly_on_empty_channel_close() {
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        let (out_tx, mut out_rx) = mpsc::channel::<WorkerMessage>(8);
        drop(tx);
        let task = tokio::spawn(run_batcher(rx, BuildId(1), out_tx));
        task.await.unwrap().expect("clean exit");
        // No messages emitted.
        assert!(out_rx.recv().await.is_none());
    }
}
