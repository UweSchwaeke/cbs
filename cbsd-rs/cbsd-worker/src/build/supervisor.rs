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

//! Process-level active-build supervisor.
//!
//! The supervisor outlives any single websocket connection. The websocket
//! loop is a transport client: it forwards inbound `ServerMessage` to the
//! supervisor and pulls outbound `WorkerMessage`s from a per-connection
//! channel the supervisor writes to.
//!
//! Closes gap G6 from the WCP soundness review: active build state is no
//! longer a local variable inside the websocket loop, so a connection drop
//! cannot lose the build. The subprocess survives until `BuildRevoke`, the
//! process exits, or local worker shutdown stops it.
//!
//! State machine (per [WCP "Worker-Side Active Build State"]):
//!
//! - `Accepted` — `BuildNew` received, executor spawned, `BuildAccepted`
//!   reported. No `BuildStarted` yet.
//! - `Started` — `BuildStarted` reported to the server. Output streaming.
//! - `Revoking` — `BuildRevoke` received; executor was sent SIGTERM. We
//!   stay in this phase until the subprocess actually exits.
//! - `TerminalPendingReport` — the subprocess finished while disconnected;
//!   the terminal `BuildFinished` payload (and any trailing output) is
//!   buffered locally until the next websocket connection.
//!
//! Reconnect status is derived from the supervisor: `Building` whenever
//! any non-terminal local state exists, `Idle` only when nothing is
//! pending.

use std::path::{Path, PathBuf};

use cbsd_proto::build::BuildId;
use cbsd_proto::ws::{BuildFinishedStatus, WorkerMessage, WorkerReportedState};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::build::{component, executor};

/// Default local spool budget per active build. The worker MUST NOT
/// continue an unbounded local build while silently dropping evidence —
/// when the budget is exceeded the supervisor kills and awaits the
/// subprocess and records a failure reason.
pub const DEFAULT_SPOOL_CAP_BYTES: u64 = 64 * 1024 * 1024;

/// Local execution phase. Mirrors the four phases in WCP "Worker-Side
/// Active Build State".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildPhase {
    /// `BuildAccepted` reported, executor running, no `BuildStarted` yet.
    Accepted,
    /// `BuildStarted` reported, output streaming.
    Started,
    /// `BuildRevoke` received, SIGTERM sent, awaiting exit.
    Revoking,
    /// Subprocess completed while disconnected; terminal result is
    /// buffered locally.
    TerminalPendingReport,
}

/// Active-build record. Present only when the worker has a build it has
/// not yet fully reported as terminal.
struct ActiveBuild {
    build_id: BuildId,
    phase: BuildPhase,
    /// Cleaned up when the build leaves the supervisor (terminal or
    /// revoke-then-exit).
    component_dir: PathBuf,
    /// Subprocess handle. `kill()` sends SIGTERM to the process group
    /// and schedules SIGKILL escalation. `None` after wait completes.
    executor: Option<executor::BuildExecutor>,
    /// Streaming task draining the subprocess stdout into the supervisor.
    /// Stored so the supervisor can await completion at shutdown.
    output_task: Option<JoinHandle<()>>,
    /// Terminal payload to deliver on the next usable connection. Set in
    /// the `TerminalPendingReport` phase.
    pending_terminal: Option<WorkerMessage>,
    /// Local spool file holding messages produced while disconnected.
    /// `None` until the first spool write.
    spool_path: Option<PathBuf>,
    /// Bytes written to the spool so far, including newline framing.
    spool_bytes: u64,
    /// True once the spool was found to exceed its budget. The
    /// supervisor will not write further messages; the build is being
    /// torn down with a failure reason.
    spool_exhausted: bool,
}

/// Per-connection transport state. Replaced when the websocket loop
/// reconnects.
struct Transport {
    /// Sender drained by the websocket loop. Dropped on disconnect.
    outbound: mpsc::Sender<WorkerMessage>,
}

/// Supervisor state guarded by a single async mutex. The mutex is held
/// only for short synchronous work or per-message spool writes — it is
/// never held across long awaits.
struct SupervisorState {
    transport: Option<Transport>,
    active: Option<ActiveBuild>,
}

/// Process-level supervisor. Construct one instance per worker process
/// and share it across reconnects.
pub struct Supervisor {
    state: Mutex<SupervisorState>,
    /// Directory under which per-build spool files live.
    spool_root: PathBuf,
    /// Per-build spool cap in bytes.
    spool_cap_bytes: u64,
}

/// Errors surfaced by supervisor public methods that callers may need to
/// react to (typically: the websocket loop should report `BuildRejected`
/// to the server when `start_build` fails because another build is
/// already active).
#[derive(Debug)]
pub enum SupervisorError {
    /// Another build is already active. The caller should respond with
    /// `BuildRejected { reason: "worker is busy" }`.
    Busy { active: BuildId },
}

impl std::fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy { active } => {
                write!(f, "worker is busy with build {active}")
            }
        }
    }
}

impl std::error::Error for SupervisorError {}

impl Supervisor {
    /// Construct a fresh supervisor with the default 64 MiB spool cap.
    pub fn new(spool_root: PathBuf) -> Self {
        Self::with_cap(spool_root, DEFAULT_SPOOL_CAP_BYTES)
    }

    /// Construct a supervisor with an explicit spool cap. Used by tests
    /// to drive the overflow path without allocating 64 MiB.
    pub fn with_cap(spool_root: PathBuf, spool_cap_bytes: u64) -> Self {
        Self {
            state: Mutex::new(SupervisorState {
                transport: None,
                active: None,
            }),
            spool_root,
            spool_cap_bytes,
        }
    }

    /// Register a freshly accepted build. Returns `Err(Busy)` if another
    /// build is already active. Takes ownership of `executor` and
    /// `component_dir`.
    pub async fn register_accepted(
        &self,
        build_id: BuildId,
        executor: executor::BuildExecutor,
        component_dir: PathBuf,
    ) -> Result<(), SupervisorError> {
        let mut state = self.state.lock().await;
        if let Some(ref ab) = state.active {
            return Err(SupervisorError::Busy {
                active: ab.build_id,
            });
        }
        state.active = Some(ActiveBuild {
            build_id,
            phase: BuildPhase::Accepted,
            component_dir,
            executor: Some(executor),
            output_task: None,
            pending_terminal: None,
            spool_path: None,
            spool_bytes: 0,
            spool_exhausted: false,
        });
        Ok(())
    }

    /// Record the streaming task handle that drains subprocess output.
    /// Called once per build immediately after `register_accepted`.
    pub async fn attach_output_task(&self, build_id: BuildId, task: JoinHandle<()>) {
        let mut state = self.state.lock().await;
        if let Some(ref mut ab) = state.active
            && ab.build_id == build_id
        {
            ab.output_task = Some(task);
        }
    }

    /// Advance the phase to `Started`. Called when the supervisor emits
    /// `BuildStarted` to the server (or queues it for spool).
    pub async fn mark_started(&self, build_id: BuildId) {
        let mut state = self.state.lock().await;
        if let Some(ref mut ab) = state.active
            && ab.build_id == build_id
            && ab.phase == BuildPhase::Accepted
        {
            ab.phase = BuildPhase::Started;
        }
    }

    /// Handle a `BuildRevoke` for the given build. The matching active
    /// build is sent SIGTERM (the subprocess streamer will observe exit
    /// and the supervisor will produce a terminal `BuildFinished`).
    /// Non-matching revokes are reported back via `Outcome` so the
    /// caller can synthesize the pre-accept reply.
    pub async fn on_build_revoke(&self, build_id: BuildId) -> RevokeOutcome {
        let mut state = self.state.lock().await;
        match state.active {
            Some(ref mut ab) if ab.build_id == build_id => {
                // Always transition to Revoking, even if a terminal
                // result is pending — per WCP, a `BuildRevoke` overrides
                // a pending terminal result and replaces it with a
                // `revoked` outcome on the next usable connection.
                ab.phase = BuildPhase::Revoking;
                ab.pending_terminal = None;
                if let Some(ref exec) = ab.executor {
                    exec.kill();
                }
                RevokeOutcome::RevokingActive
            }
            Some(ref ab) => RevokeOutcome::NonActive {
                active: ab.build_id,
            },
            None => RevokeOutcome::Idle,
        }
    }

    /// Forward a subprocess-produced message (`BuildOutput` /
    /// `BuildFinished`) toward the server.
    ///
    /// When a transport is attached, messages flow directly through it.
    /// When the worker is disconnected, messages are appended to the
    /// per-build spool file. `BuildFinished` messages produced while
    /// disconnected become the supervisor's `TerminalPendingReport` and
    /// are delivered on the next reconnect.
    pub async fn on_output_message(&self, msg: WorkerMessage) {
        // Defensive: this should never be called when the active build
        // does not match the message's build_id, but we still want to
        // do the right thing under racy shutdown.
        let build_id = match &msg {
            WorkerMessage::BuildOutput { build_id, .. }
            | WorkerMessage::BuildFinished { build_id, .. } => *build_id,
            _ => {
                // Lifecycle messages other than output/finished are not
                // produced by the streaming task; nothing to spool.
                self.send_or_spool(msg).await;
                return;
            }
        };

        let mut state = self.state.lock().await;
        let Some(active) = state.active.as_mut() else {
            // No active build — drop, since we can no longer make any
            // claim about ownership.
            tracing::debug!(%build_id, "dropping orphan output message");
            return;
        };
        if active.build_id != build_id {
            tracing::warn!(
                msg_build = %build_id,
                active_build = %active.build_id,
                "dropping output for non-active build"
            );
            return;
        }

        // Is this the terminal? If so we may need to enter
        // TerminalPendingReport if no transport exists.
        let is_terminal = matches!(msg, WorkerMessage::BuildFinished { .. });

        if let Some(ref t) = state.transport {
            // Connected: forward directly.
            if let Err(err) = t.outbound.send(msg).await {
                // Connection died between the upper match and the send.
                // Push the message into the spool so it survives the
                // reconnect.
                tracing::debug!(%err, "outbound channel closed mid-forward, spooling");
                let recovered = match err.0 {
                    m @ WorkerMessage::BuildOutput { .. }
                    | m @ WorkerMessage::BuildFinished { .. } => m,
                    _ => return,
                };
                state.transport = None;
                self.spool_and_finalize(&mut state, recovered, is_terminal)
                    .await;
            }
            return;
        }

        // Disconnected: spool, and on terminal flip phase.
        self.spool_and_finalize(&mut state, msg, is_terminal).await;
    }

    /// Drain pending state out the new transport. Returns the ordered
    /// list of messages to send on reconnect:
    ///
    /// 1. `WorkerStatus(Building, build_id)` if the supervisor has any
    ///    non-terminal local state.
    /// 2. Spooled output messages in arrival order.
    /// 3. The pending terminal `BuildFinished`, if any.
    ///
    /// Or, when no local state exists, a single `WorkerStatus(Idle)`.
    ///
    /// The supervisor must also receive `attach_transport` for the
    /// caller-provided outbound sender so subsequent live output flows
    /// directly. `attach_transport` is called by the caller before
    /// `take_reconnect_messages` so that any output produced concurrent
    /// with the drain still reaches the server in order.
    pub async fn take_reconnect_messages(&self) -> Vec<WorkerMessage> {
        // Phase 1: snapshot the data we need under the lock, consuming
        // the pending terminal. The lock is released before the spool
        // file I/O so other supervisor operations are not serialised
        // behind the drain (review F5 / CLAUDE.md correctness invariant
        // #2 on async mutex + I/O).
        struct Snapshot {
            build_id: BuildId,
            spool_path: Option<std::path::PathBuf>,
            pending_terminal: Option<WorkerMessage>,
        }

        let snapshot = {
            let mut state = self.state.lock().await;
            let Some(active) = state.active.as_mut() else {
                return vec![WorkerMessage::WorkerStatus {
                    state: WorkerReportedState::Idle,
                    build_id: None,
                }];
            };

            let spool_path = if active.spool_bytes > 0 {
                active.spool_path.clone()
            } else {
                None
            };
            let pending_terminal = if matches!(active.phase, BuildPhase::TerminalPendingReport) {
                active.pending_terminal.take()
            } else {
                None
            };

            Snapshot {
                build_id: active.build_id,
                spool_path,
                pending_terminal,
            }
        };

        // Phase 2: build the message list. No lock held while we touch
        // the filesystem.
        let mut out = Vec::new();
        out.push(WorkerMessage::WorkerStatus {
            state: WorkerReportedState::Building,
            build_id: Some(snapshot.build_id),
        });

        if let Some(ref path) = snapshot.spool_path {
            match drain_spool(path).await {
                Ok(msgs) => out.extend(msgs),
                Err(err) => {
                    tracing::error!(
                        path = %path.display(),
                        %err,
                        "failed to drain spool; spooled output will be lost"
                    );
                }
            }
            if let Err(err) = tokio::fs::remove_file(path).await
                && err.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(
                    path = %path.display(),
                    %err,
                    "failed to remove drained spool file"
                );
            }

            // Phase 3: reflect the drain in the supervisor state.
            // Re-acquire the lock briefly. Guard against the build
            // having been retired and a new one registered concurrently
            // by comparing build_id against the snapshot. The current
            // call graph (reconnect runs before any handler loop, and
            // `register_accepted` only fires from inside the handler
            // loop) makes that race unreachable today, but the guard
            // future-proofs the assumption (review v2 finding N4).
            let mut state = self.state.lock().await;
            if let Some(active) = state.active.as_mut()
                && active.build_id == snapshot.build_id
            {
                active.spool_bytes = 0;
                active.spool_path = None;
            }
        }

        if let Some(terminal) = snapshot.pending_terminal {
            out.push(terminal);
        }

        out
    }

    /// Attach the websocket loop's outbound channel. Replaces any
    /// previous transport. Called by the handler immediately on a fresh
    /// connection, before `take_reconnect_messages`.
    pub async fn attach_transport(&self, outbound: mpsc::Sender<WorkerMessage>) {
        let mut state = self.state.lock().await;
        state.transport = Some(Transport { outbound });
    }

    /// Detach the current transport. Called when the websocket loop
    /// observes a send/receive error and is about to return so the
    /// caller can reconnect. Does NOT touch the active build — by
    /// design, disconnects do not kill builds.
    pub async fn detach_transport(&self) {
        let mut state = self.state.lock().await;
        state.transport = None;
    }

    /// Retire a build whose terminal `BuildFinished` has been fully
    /// emitted to the server. The websocket loop calls this after
    /// observing the terminal message on the outbound channel.
    pub async fn retire(&self, build_id: BuildId) {
        let mut state = self.state.lock().await;
        let Some(active) = state.active.as_mut() else {
            return;
        };
        if active.build_id != build_id {
            return;
        }

        // Await the streaming task. The terminal message was the last
        // thing it produced, so the task is either finishing or already
        // done.
        let task = active.output_task.take();
        let executor = active.executor.take();
        let spool_path = active.spool_path.take();
        let component_dir = active.component_dir.clone();

        // Drop the mutex before awaiting child / streamer.
        drop(state);

        if let Some(t) = task {
            // We hold no locks here; safe to await.
            let _ = t.await;
        }
        if let Some(mut exec) = executor {
            let exit_code = exec.wait().await;
            tracing::info!(
                %build_id,
                ?exit_code,
                "build subprocess exited"
            );
        }
        component::cleanup(&component_dir);
        if let Some(ref path) = spool_path
            && let Err(err) = tokio::fs::remove_file(path).await
            && err.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                path = %path.display(),
                %err,
                "failed to remove spool on retire"
            );
        }

        // Drop the active record last so a concurrent `on_output_message`
        // racing with retire sees the still-present build_id rather than
        // an orphan.
        let mut state = self.state.lock().await;
        if let Some(ref ab) = state.active
            && ab.build_id == build_id
        {
            state.active = None;
        }
    }

    /// Kill any active build and await it. Called on local worker
    /// shutdown (the only stop-work signal besides `BuildRevoke`).
    pub async fn shutdown(&self) {
        let (build_id, executor, task, component_dir, spool_path) = {
            let mut state = self.state.lock().await;
            let Some(active) = state.active.as_mut() else {
                return;
            };
            if let Some(ref exec) = active.executor {
                exec.kill();
            }
            (
                active.build_id,
                active.executor.take(),
                active.output_task.take(),
                active.component_dir.clone(),
                active.spool_path.take(),
            )
        };

        if let Some(t) = task {
            let _ = t.await;
        }
        if let Some(mut exec) = executor {
            let _ = exec.wait().await;
        }
        component::cleanup(&component_dir);
        if let Some(ref path) = spool_path
            && let Err(err) = tokio::fs::remove_file(path).await
            && err.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                path = %path.display(),
                %err,
                "failed to remove spool on shutdown"
            );
        }

        // Drop the record.
        let mut state = self.state.lock().await;
        if let Some(ref ab) = state.active
            && ab.build_id == build_id
        {
            state.active = None;
        }
    }

    /// Returns the id of the currently-active build, if any. Used by
    /// the handler to short-circuit duplicate `BuildNew` dispatches
    /// before reading the tarball binary frame.
    pub async fn active_build_id(&self) -> Option<BuildId> {
        self.state.lock().await.active.as_ref().map(|a| a.build_id)
    }

    /// Test/diagnostic accessor for the current phase.
    #[cfg(test)]
    pub async fn current_phase(&self) -> Option<BuildPhase> {
        self.state.lock().await.active.as_ref().map(|a| a.phase)
    }

    /// Test/diagnostic accessor for spool bytes consumed.
    #[cfg(test)]
    pub async fn spool_bytes(&self) -> u64 {
        self.state
            .lock()
            .await
            .active
            .as_ref()
            .map_or(0, |a| a.spool_bytes)
    }

    // ---------------------------------------------------------------
    // Internals
    // ---------------------------------------------------------------

    /// Persist a subprocess-produced message while disconnected.
    ///
    /// Terminal `BuildFinished` messages never enter the spool file —
    /// they go straight to `pending_terminal` so the reconnect drain
    /// path can deliver them exactly once after the spooled output.
    /// `BuildOutput` messages are appended to the per-build spool file
    /// under the supervisor's spool budget; an over-budget or I/O
    /// failure kills the subprocess and synthesizes a failure terminal.
    /// Caller holds the supervisor lock.
    async fn spool_and_finalize(
        &self,
        state: &mut SupervisorState,
        msg: WorkerMessage,
        is_terminal: bool,
    ) {
        let Some(active) = state.active.as_mut() else {
            return;
        };
        if active.spool_exhausted {
            // Budget already exceeded; the cleanup path is in flight.
            return;
        }
        let build_id = active.build_id;

        // Terminal short-circuit: never write the terminal to the spool
        // file. The reconnect drain emits spool output first, then the
        // pending terminal exactly once, so storing it on disk too
        // would risk a duplicate `BuildFinished` if the truncate-back
        // failed.
        if is_terminal {
            active.phase = BuildPhase::TerminalPendingReport;
            active.pending_terminal = Some(msg);
            return;
        }

        let serialized = match serde_json::to_vec(&msg) {
            Ok(v) => v,
            Err(err) => {
                tracing::error!(%build_id, %err, "failed to serialize spool message");
                return;
            }
        };
        // +1 for newline framing.
        let new_total = active
            .spool_bytes
            .saturating_add(serialized.len() as u64 + 1);
        if new_total > self.spool_cap_bytes {
            active.spool_exhausted = true;
            tracing::error!(
                %build_id,
                cap = self.spool_cap_bytes,
                attempted = new_total,
                "worker disconnected output spool exceeded; killing build"
            );
            // Kill+await the subprocess and synthesize the failure
            // terminal. The worker will deliver it on the next
            // reconnect.
            if let Some(ref exec) = active.executor {
                exec.kill();
            }
            active.phase = BuildPhase::TerminalPendingReport;
            active.pending_terminal = Some(WorkerMessage::BuildFinished {
                build_id,
                status: BuildFinishedStatus::Failure,
                error: Some("worker disconnected output spool exceeded".to_string()),
                build_report: None,
            });
            return;
        }

        let spool_path = match active.spool_path.clone() {
            Some(p) => p,
            None => {
                let p = self.spool_root.join(format!("build-{build_id}.spool"));
                active.spool_path = Some(p.clone());
                p
            }
        };

        match append_spool(&spool_path, &serialized).await {
            Ok(()) => {
                active.spool_bytes = new_total;
            }
            Err(err) => {
                tracing::error!(
                    %build_id,
                    path = %spool_path.display(),
                    %err,
                    "failed to write spool; killing build"
                );
                active.spool_exhausted = true;
                if let Some(ref exec) = active.executor {
                    exec.kill();
                }
                active.phase = BuildPhase::TerminalPendingReport;
                active.pending_terminal = Some(WorkerMessage::BuildFinished {
                    build_id,
                    status: BuildFinishedStatus::Failure,
                    error: Some(format!("worker spool write error: {err}")),
                    build_report: None,
                });
            }
        }
    }

    /// Forward a non-output message via the live transport if any, or
    /// drop it. Lifecycle messages other than output/finished do not go
    /// to the spool because the protocol does not allow replaying them
    /// (they are sent at most once per connection).
    async fn send_or_spool(&self, msg: WorkerMessage) {
        let state = self.state.lock().await;
        if let Some(ref t) = state.transport
            && t.outbound.send(msg).await.is_err()
        {
            tracing::debug!("outbound channel closed; dropping non-output message");
        }
    }
}

/// Outcome of an inbound `BuildRevoke`. The websocket handler uses this
/// to synthesize the pre-accept reply when no matching local build
/// exists.
#[derive(Debug)]
pub enum RevokeOutcome {
    /// The active build matched and is now being revoked. The terminal
    /// `BuildFinished(revoked)` will flow from the streaming task.
    RevokingActive,
    /// A different build is active. The handler should log a warning
    /// and may want to send `BuildFinished(revoked)` for the requested
    /// build id since the worker never accepted it.
    NonActive { active: BuildId },
    /// No active build. The handler synthesizes an immediate
    /// `BuildFinished(revoked)` for the requested build id.
    Idle,
}

/// Append a serialized message + newline to the spool file. Opens the
/// file in append mode, creating it if needed.
async fn append_spool(path: &Path, payload: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    f.write_all(payload).await?;
    f.write_all(b"\n").await?;
    f.flush().await?;
    Ok(())
}

/// Read the spool file line-by-line and parse each as a `WorkerMessage`.
/// Malformed lines are logged and skipped; the goal is best-effort
/// recovery, not strict validation.
async fn drain_spool(path: &Path) -> std::io::Result<Vec<WorkerMessage>> {
    let f = File::open(path).await?;
    let reader = BufReader::new(f);
    let mut lines = reader.lines();
    let mut out = Vec::new();
    while let Some(line) = lines.next_line().await? {
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<WorkerMessage>(&line) {
            Ok(msg) => out.push(msg),
            Err(err) => {
                tracing::warn!(%err, "skipping malformed spool line");
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cbsd_proto::build::BuildId;

    use super::*;

    fn supervisor_with_cap(tmp: &tempfile::TempDir, cap: u64) -> Arc<Supervisor> {
        Arc::new(Supervisor::with_cap(tmp.path().to_path_buf(), cap))
    }

    /// Build a `BuildOutput` of approximately `target_bytes` once
    /// serialized. Used to drive the spool over its cap.
    fn output_of_size(build_id: BuildId, target_bytes: usize) -> WorkerMessage {
        // serde overhead is ~80-100 bytes; pad the line accordingly.
        let line = "x".repeat(target_bytes.saturating_sub(100));
        WorkerMessage::BuildOutput {
            build_id,
            start_seq: 0,
            lines: vec![line],
        }
    }

    #[tokio::test]
    async fn idle_reconnect_reports_idle() {
        let tmp = tempfile::tempdir().unwrap();
        let sup = supervisor_with_cap(&tmp, DEFAULT_SPOOL_CAP_BYTES);
        let msgs = sup.take_reconnect_messages().await;
        assert_eq!(msgs.len(), 1);
        assert!(matches!(
            msgs[0],
            WorkerMessage::WorkerStatus {
                state: WorkerReportedState::Idle,
                build_id: None
            }
        ));
    }

    #[tokio::test]
    async fn revoke_with_no_active_build_reports_idle() {
        let tmp = tempfile::tempdir().unwrap();
        let sup = supervisor_with_cap(&tmp, DEFAULT_SPOOL_CAP_BYTES);
        let outcome = sup.on_build_revoke(BuildId(7)).await;
        assert!(matches!(outcome, RevokeOutcome::Idle));
    }

    /// Set the supervisor up in an "accepted" state without spawning
    /// a real subprocess. The build_id-only mutations let us cover the
    /// state-machine surface without flaky subprocess fixtures.
    async fn force_active(sup: &Supervisor, build_id: BuildId, phase: BuildPhase) {
        let mut state = sup.state.lock().await;
        state.active = Some(ActiveBuild {
            build_id,
            phase,
            component_dir: PathBuf::from("/tmp/cbsd-fake"),
            executor: None,
            output_task: None,
            pending_terminal: None,
            spool_path: None,
            spool_bytes: 0,
            spool_exhausted: false,
        });
    }

    #[tokio::test]
    async fn building_phase_reports_building_on_reconnect() {
        let tmp = tempfile::tempdir().unwrap();
        let sup = supervisor_with_cap(&tmp, DEFAULT_SPOOL_CAP_BYTES);
        force_active(&sup, BuildId(42), BuildPhase::Started).await;

        let msgs = sup.take_reconnect_messages().await;
        assert_eq!(msgs.len(), 1);
        assert!(matches!(
            msgs[0],
            WorkerMessage::WorkerStatus {
                state: WorkerReportedState::Building,
                build_id: Some(BuildId(42)),
            }
        ));
    }

    #[tokio::test]
    async fn output_while_disconnected_is_spooled_and_replayed() {
        let tmp = tempfile::tempdir().unwrap();
        let sup = supervisor_with_cap(&tmp, DEFAULT_SPOOL_CAP_BYTES);
        force_active(&sup, BuildId(11), BuildPhase::Started).await;

        // No transport attached → spool.
        sup.on_output_message(WorkerMessage::BuildOutput {
            build_id: BuildId(11),
            start_seq: 0,
            lines: vec!["hello".to_string(), "world".to_string()],
        })
        .await;
        sup.on_output_message(WorkerMessage::BuildOutput {
            build_id: BuildId(11),
            start_seq: 2,
            lines: vec!["again".to_string()],
        })
        .await;

        assert!(sup.spool_bytes().await > 0);

        let msgs = sup.take_reconnect_messages().await;
        // [Building, BuildOutput(0..2), BuildOutput(2..3)]
        assert_eq!(msgs.len(), 3);
        assert!(matches!(
            msgs[0],
            WorkerMessage::WorkerStatus {
                state: WorkerReportedState::Building,
                build_id: Some(BuildId(11)),
            }
        ));
        match &msgs[1] {
            WorkerMessage::BuildOutput {
                build_id,
                start_seq,
                lines,
                ..
            } => {
                assert_eq!(build_id, &BuildId(11));
                assert_eq!(start_seq, &0u64);
                assert_eq!(lines.len(), 2);
            }
            other => panic!("expected BuildOutput, got {other:?}"),
        }
        match &msgs[2] {
            WorkerMessage::BuildOutput { start_seq, .. } => {
                assert_eq!(start_seq, &2u64);
            }
            other => panic!("expected BuildOutput, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn terminal_during_disconnect_is_pending_and_replayed_last() {
        let tmp = tempfile::tempdir().unwrap();
        let sup = supervisor_with_cap(&tmp, DEFAULT_SPOOL_CAP_BYTES);
        force_active(&sup, BuildId(5), BuildPhase::Started).await;

        sup.on_output_message(WorkerMessage::BuildOutput {
            build_id: BuildId(5),
            start_seq: 0,
            lines: vec!["partial".to_string()],
        })
        .await;
        sup.on_output_message(WorkerMessage::BuildFinished {
            build_id: BuildId(5),
            status: BuildFinishedStatus::Success,
            error: None,
            build_report: None,
        })
        .await;

        assert_eq!(
            sup.current_phase().await,
            Some(BuildPhase::TerminalPendingReport)
        );

        let msgs = sup.take_reconnect_messages().await;
        // [Building, BuildOutput, BuildFinished(success)]
        assert_eq!(msgs.len(), 3);
        assert!(matches!(
            msgs[0],
            WorkerMessage::WorkerStatus {
                state: WorkerReportedState::Building,
                build_id: Some(BuildId(5)),
            }
        ));
        assert!(matches!(msgs[1], WorkerMessage::BuildOutput { .. }));
        assert!(matches!(
            msgs[2],
            WorkerMessage::BuildFinished {
                status: BuildFinishedStatus::Success,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn spool_overflow_produces_failure_terminal() {
        let tmp = tempfile::tempdir().unwrap();
        // 1 KiB cap so a single ~600-byte line then a second push
        // exceeds it.
        let sup = supervisor_with_cap(&tmp, 1024);
        force_active(&sup, BuildId(99), BuildPhase::Started).await;

        // First message: ~600 bytes — fits.
        sup.on_output_message(output_of_size(BuildId(99), 600))
            .await;
        // Second message: another ~600 bytes — pushes past 1 KiB.
        sup.on_output_message(output_of_size(BuildId(99), 600))
            .await;

        assert_eq!(
            sup.current_phase().await,
            Some(BuildPhase::TerminalPendingReport)
        );

        let msgs = sup.take_reconnect_messages().await;
        // Status, possibly one BuildOutput that fit, then the synthetic
        // failure terminal.
        let last = msgs.last().expect("at least the status");
        match last {
            WorkerMessage::BuildFinished { status, error, .. } => {
                assert_eq!(status, &BuildFinishedStatus::Failure);
                assert!(
                    error
                        .as_deref()
                        .is_some_and(|e| e.contains("spool exceeded")),
                    "unexpected error message: {error:?}"
                );
            }
            other => panic!("expected synthetic BuildFinished, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn revoke_active_build_transitions_to_revoking() {
        let tmp = tempfile::tempdir().unwrap();
        let sup = supervisor_with_cap(&tmp, DEFAULT_SPOOL_CAP_BYTES);
        force_active(&sup, BuildId(3), BuildPhase::Started).await;

        let outcome = sup.on_build_revoke(BuildId(3)).await;
        assert!(matches!(outcome, RevokeOutcome::RevokingActive));
        assert_eq!(sup.current_phase().await, Some(BuildPhase::Revoking));
    }

    #[tokio::test]
    async fn revoke_clears_pending_terminal() {
        let tmp = tempfile::tempdir().unwrap();
        let sup = supervisor_with_cap(&tmp, DEFAULT_SPOOL_CAP_BYTES);
        force_active(&sup, BuildId(8), BuildPhase::TerminalPendingReport).await;
        // Manually plant a pending terminal.
        {
            let mut state = sup.state.lock().await;
            state.active.as_mut().unwrap().pending_terminal = Some(WorkerMessage::BuildFinished {
                build_id: BuildId(8),
                status: BuildFinishedStatus::Success,
                error: None,
                build_report: None,
            });
        }

        let outcome = sup.on_build_revoke(BuildId(8)).await;
        assert!(matches!(outcome, RevokeOutcome::RevokingActive));

        // Pending terminal must be discarded.
        let state = sup.state.lock().await;
        assert!(state.active.as_ref().unwrap().pending_terminal.is_none());
        assert_eq!(state.active.as_ref().unwrap().phase, BuildPhase::Revoking);
    }

    #[tokio::test]
    async fn revoke_for_non_active_build_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let sup = supervisor_with_cap(&tmp, DEFAULT_SPOOL_CAP_BYTES);
        force_active(&sup, BuildId(1), BuildPhase::Started).await;

        let outcome = sup.on_build_revoke(BuildId(2)).await;
        assert!(matches!(
            outcome,
            RevokeOutcome::NonActive { active: BuildId(1) }
        ));
    }

    #[tokio::test]
    async fn connected_output_forwards_through_transport() {
        let tmp = tempfile::tempdir().unwrap();
        let sup = supervisor_with_cap(&tmp, DEFAULT_SPOOL_CAP_BYTES);
        force_active(&sup, BuildId(20), BuildPhase::Started).await;

        let (tx, mut rx) = mpsc::channel(8);
        sup.attach_transport(tx).await;

        sup.on_output_message(WorkerMessage::BuildOutput {
            build_id: BuildId(20),
            start_seq: 0,
            lines: vec!["live".to_string()],
        })
        .await;

        let got = rx.recv().await.expect("forwarded");
        assert!(matches!(got, WorkerMessage::BuildOutput { .. }));
        // No spool writes while connected.
        assert_eq!(sup.spool_bytes().await, 0);
    }

    #[tokio::test]
    async fn output_for_non_active_build_is_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let sup = supervisor_with_cap(&tmp, DEFAULT_SPOOL_CAP_BYTES);
        force_active(&sup, BuildId(50), BuildPhase::Started).await;

        sup.on_output_message(WorkerMessage::BuildOutput {
            build_id: BuildId(51),
            start_seq: 0,
            lines: vec!["wrong build".to_string()],
        })
        .await;
        assert_eq!(sup.spool_bytes().await, 0);
    }
}
