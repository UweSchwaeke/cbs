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

//! Build dispatch engine.
//!
//! `try_dispatch` is the core function: called when a build is submitted or a
//! worker becomes idle. It pops the highest-priority pending build, finds a
//! matching idle worker, updates the DB, packs the component tarball, and
//! sends `BuildNew` + binary tarball to the worker.

use axum::extract::ws::Message;
use cbsd_proto::ws::{BuildRevokeReason, ServerMessage};
use cbsd_proto::{BuildId, Priority};
use sqlx::SqlitePool;

use crate::app::{AppState, LogWatchers, WorkerSenders};
use crate::components::tarball;
use crate::db;
use crate::queue::{ActiveBuild, QueuedBuild, SharedBuildQueue};

/// Errors that can occur during dispatch.
#[derive(Debug)]
pub enum DispatchError {
    /// No pending builds or no idle workers — not an error, just nothing to do.
    NothingToDispatch,
    /// Database error during state transition.
    Database(sqlx::Error),
    /// Failed to pack the component tarball.
    Tarball(std::io::Error),
    /// Failed to send message to worker (channel closed).
    Send(String),
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingToDispatch => write!(f, "nothing to dispatch"),
            Self::Database(e) => write!(f, "database error: {e}"),
            Self::Tarball(e) => write!(f, "tarball packing error: {e}"),
            Self::Send(e) => write!(f, "send error: {e}"),
        }
    }
}

/// Attempt to dispatch the next pending build to an idle worker.
///
/// This is the core dispatch loop body. It is called:
/// - When a new build is submitted (from the REST handler)
/// - When a worker becomes idle (build finished, build rejected)
///
/// Returns `Ok(())` if a build was dispatched or there was nothing to do.
/// Returns `Err` on database/IO/send failures.
pub async fn try_dispatch(state: &AppState) -> Result<(), DispatchError> {
    // Step 1-4: Under the queue lock, pop a build and find a matching worker.
    let dispatch_info = {
        let mut queue = state.queue.lock().await;

        // Pop highest-priority pending build.
        let build = match queue.next_pending() {
            Some(b) => b,
            None => return Err(DispatchError::NothingToDispatch),
        };

        let build_arch = build.descriptor.build.arch;

        // Find first idle worker with matching arch.
        // A worker is idle if it's Connected and has no active build.
        let worker = queue
            .workers
            .iter()
            .find(|(cid, ws)| {
                ws.is_dispatch_eligible()
                    && ws.arch() == Some(build_arch)
                    && !queue.active.values().any(|ab| ab.connection_id == **cid)
            })
            .map(|(cid, ws)| {
                (
                    cid.clone(),
                    ws.registered_worker_id().unwrap_or("unknown").to_string(),
                )
            });

        let (connection_id, registered_worker_id) = match worker {
            Some(w) => w,
            None => {
                // No matching worker — push build back to front of its lane.
                queue.enqueue_front(build);
                return Err(DispatchError::NothingToDispatch);
            }
        };

        // Step 5: Generate trace_id.
        let trace_id = uuid::Uuid::new_v4().to_string();

        // Step 6: Update DB under the lock (correctness invariant #1).
        db::builds::set_build_dispatched(
            &state.pool,
            build.build_id.0,
            &trace_id,
            &registered_worker_id,
        )
        .await
        .map_err(DispatchError::Database)?;

        // build_logs row already inserted at submission time.

        // Step 7: Create watch channel for log notifications.
        let (watch_tx, _watch_rx) = tokio::sync::watch::channel(());
        {
            let mut watchers = state.log_watchers.lock().await;
            watchers.insert(build.build_id.0, watch_tx);
        }

        // Step 8: Insert ActiveBuild into queue.active.
        let ack_cancel = tokio_util::sync::CancellationToken::new();
        queue.active.insert(
            build.build_id.0,
            ActiveBuild {
                build_id: build.build_id.0,
                connection_id: connection_id.clone(),
                dispatched_at: tokio::time::Instant::now(),
                trace_id: trace_id.clone(),
                descriptor: build.descriptor.clone(),
                priority: build.priority,
                ack_cancel: ack_cancel.clone(),
                receipt: crate::queue::ActiveAssignmentReceipt::AwaitingReceipt,
            },
        );

        tracing::info!(
            build_id = build.build_id.0,
            connection_id = %connection_id,
            worker_id = %registered_worker_id,
            trace_id = %trace_id,
            arch = %build_arch,
            "build dispatched to worker"
        );

        // Collect info needed outside the lock.
        DispatchInfo {
            build_id: build.build_id,
            priority: build.priority,
            descriptor: build.descriptor.clone(),
            trace_id,
            connection_id,
            queued_at: build.queued_at,
        }
    };
    // Step 9: Lock released here.

    // Dispatch latency spans the out-of-lock pack + send below.
    let dispatch_started = std::time::Instant::now();

    // Step 10: Pack the component tarball (outside lock). Every component the
    // descriptor references is packed under its own `<name>/` prefix into the
    // one tarball, so multi-component builds reach the worker intact (cbscore
    // enumerates the unpack root's subdirectories to find each component).
    let component_names: Vec<&str> = dispatch_info
        .descriptor
        .components
        .iter()
        .map(|c| c.name.as_str())
        .collect();

    // The submission validator guarantees at least one component, but guard
    // here too: an empty list would otherwise ship an empty archive. Funnel it
    // through the same deterministic-failure handling as a pack error below.
    let pack_result = if component_names.is_empty() {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "build has no components",
        ))
    } else {
        tarball::pack_components(&state.config.components_dir, &component_names)
    };

    let (tar_gz_bytes, sha256_hex) = match pack_result {
        Ok(packed) => packed,
        Err(e) => {
            // The pack step had no recovery of its own: by here the build is
            // `dispatched` in the DB and present in `queue.active`, so a pack
            // error left it wedged there until a restart. A pack failure is
            // deterministic (a missing/unreadable component directory stays
            // that way), so — unlike the transient send path — we must NOT
            // re-queue: `enqueue_front` would put the build back at the head of
            // its lane and the next dispatch trigger would re-pop, re-pack, and
            // re-fail forever, blocking the lane (`next_pending` never consults
            // `queue.active`, so the old wedge at least did not do that). Mark
            // the build FAILURE instead so it leaves the queue and surfaces to
            // the user, mirroring the integrity-reject branch of
            // `handle_build_rejected`. No ack timer exists yet (it is spawned
            // only after the send), so removing the active entry is sufficient.
            let reason = format!("failed to pack component tarball: {e}");
            tracing::error!(build_id = dispatch_info.build_id.0, "{reason}");
            crate::ws::handler::cleanup_terminal_state(
                &state.pool,
                &state.queue,
                &state.log_watchers,
                dispatch_info.build_id.0,
                crate::ws::handler::TerminalStatus::Failure,
                &reason,
            )
            .await;
            return Err(DispatchError::Tarball(e));
        }
    };

    tracing::debug!(
        build_id = dispatch_info.build_id.0,
        components = ?component_names,
        tarball_size = tar_gz_bytes.len(),
        sha256 = %sha256_hex,
        "component tarball packed"
    );

    // Step 11: Send BuildNew JSON frame + binary tarball frame to worker.
    let build_new = ServerMessage::BuildNew {
        build_id: dispatch_info.build_id,
        trace_id: dispatch_info.trace_id.clone(),
        priority: dispatch_info.priority,
        descriptor: Box::new(dispatch_info.descriptor.clone()),
        component_sha256: sha256_hex,
    };

    let json_text =
        serde_json::to_string(&build_new).expect("ServerMessage serialization cannot fail");

    // Steps 11-12: send the JSON frame + binary tarball frame; on any send
    // failure roll back per WCP D4 SI-6/SI-13. The extracted helper makes
    // the send-then-rollback wiring testable as one unit.
    send_and_recover(
        &state.pool,
        &state.queue,
        &state.log_watchers,
        &state.worker_senders,
        &dispatch_info,
        json_text,
        tar_gz_bytes,
    )
    .await?;

    // The build is now successfully handed off — record how long it waited in
    // the queue and how long the pack+send took.
    let arch = dispatch_info.descriptor.build.arch;
    crate::metrics::lifecycle::record_dispatch_latency(
        arch,
        dispatch_started.elapsed().as_secs_f64(),
    );
    let wait_secs = (chrono::Utc::now().timestamp() - dispatch_info.queued_at).max(0) as f64;
    crate::metrics::lifecycle::record_queue_wait(dispatch_info.priority, arch, wait_secs);

    // Step 13: Spawn ack timeout task. If the worker doesn't send
    // build_accepted within dispatch_ack_timeout_secs, re-queue the build.
    // The CancellationToken in ActiveBuild is cancelled by handle_build_accepted.
    {
        let ack_timeout_secs = state.config.timeouts.dispatch_ack_timeout_secs;
        let ack_state = state.clone();
        let ack_build_id = dispatch_info.build_id;
        let cancel = {
            let queue = state.queue.lock().await;
            queue
                .active
                .get(&ack_build_id.0)
                .map(|a| a.ack_cancel.clone())
        };
        if let Some(cancel) = cancel {
            tokio::spawn(async move {
                tokio::select! {
                    () = cancel.cancelled() => {
                        // build_accepted received, nothing to do
                    }
                    () = tokio::time::sleep(std::time::Duration::from_secs(ack_timeout_secs)) => {
                        tracing::warn!(
                            build_id = ack_build_id.0,
                            timeout_secs = ack_timeout_secs,
                            "dispatch ack timeout — re-queuing build"
                        );
                        crate::metrics::lifecycle::record_dispatch_ack_timeout();
                        crate::metrics::lifecycle::record_requeue("ack_timeout");
                        rollback_active_to_queued(
                            &ack_state.pool,
                            &ack_state.queue,
                            &ack_state.log_watchers,
                            ack_build_id.0,
                        ).await;
                        // Re-dispatch will be picked up by the periodic sweep
                        // (30s) or the next build_finished event.
                    }
                }
            });
        }
    }

    Ok(())
}

/// Handle a `BuildAccepted` message from a worker.
///
/// Cancels the dispatch ack timeout.
pub async fn handle_build_accepted(state: &AppState, connection_id: &str, build_id: i64) {
    let queue = state.queue.lock().await;
    if let Some(active) = queue.active.get(&build_id) {
        active.ack_cancel.cancel();
        tracing::info!(
            build_id = build_id,
            connection_id = %connection_id,
            "build accepted by worker, ack timer cancelled"
        );
    } else {
        tracing::warn!(
            build_id = build_id,
            connection_id = %connection_id,
            "build_accepted for unknown active build"
        );
    }
}

/// Handle a `BuildStarted` message from a worker.
///
/// Updates the DB state to "started" and sets `started_at`.
pub async fn handle_build_started(state: &AppState, build_id: i64) {
    match db::builds::set_build_started(&state.pool, build_id).await {
        Ok(true) => {
            tracing::info!(build_id = build_id, "build started");
        }
        Ok(false) => {
            tracing::warn!(
                build_id = build_id,
                "build started but no DB row updated (stale?)"
            );
        }
        Err(e) => {
            tracing::error!(
                build_id = build_id,
                "failed to update build state to started: {e}"
            );
        }
    }
}

/// Roll an active build back to `queued` after a failed or abandoned
/// dispatch. Per WCP D4: clears the six provenance columns via the
/// dedicated rollback DB operation, cancels the dispatch-ack timer,
/// drops the log watcher, and re-enqueues at the front of the build's
/// priority lane. Returns `true` if the build was active and is now
/// rolled back; `false` if no active entry existed.
///
/// Takes the three pieces of `AppState` it touches as explicit args so
/// callers in tests can drive the path without a full `AppState`.
pub async fn rollback_active_to_queued(
    pool: &SqlitePool,
    queue: &SharedBuildQueue,
    log_watchers: &LogWatchers,
    build_id: i64,
) -> bool {
    let active = {
        let mut q = queue.lock().await;
        q.active.remove(&build_id)
    };

    let Some(ab) = active else {
        return false;
    };

    ab.ack_cancel.cancel();

    if let Err(e) = db::builds::rollback_dispatch_to_queued(pool, build_id).await {
        tracing::error!(build_id, "rollback to queued failed: {e}");
    }

    {
        let mut q = queue.lock().await;
        q.enqueue_front(QueuedBuild {
            build_id: BuildId(build_id),
            priority: ab.priority,
            descriptor: ab.descriptor,
            user_email: String::new(),
            // Re-stamp so the queue-wait metric measures the wait for this
            // re-dispatch attempt, not a 0 sentinel (which would read as a
            // ~55-year wait once dispatched).
            queued_at: chrono::Utc::now().timestamp(),
        });
    }

    {
        let mut watchers = log_watchers.lock().await;
        watchers.remove(&build_id);
    }

    true
}

/// Step-12 send-failure rollback path of `try_dispatch`, extracted into
/// a private helper that takes explicit state pieces so the regression
/// guard for WCP D4 SI-6/SI-13 (review v1 finding F1) can be tested
/// without standing up a full `AppState`. Logs the failure, then
/// delegates to `rollback_active_to_queued` to shed in-memory state and
/// clear all six provenance columns in the DB.
async fn handle_dispatch_send_failure(
    pool: &SqlitePool,
    queue: &SharedBuildQueue,
    log_watchers: &LogWatchers,
    build_id: BuildId,
    connection_id: &str,
    err: &DispatchError,
) {
    tracing::error!(
        build_id = build_id.0,
        connection_id = %connection_id,
        "failed to send build to worker: {err}"
    );
    rollback_active_to_queued(pool, queue, log_watchers, build_id.0).await;
}

/// Send `BuildNew`'s JSON frame and component tarball binary frame to the
/// worker; on any send failure, invoke `handle_dispatch_send_failure` and
/// surface the error. Couples the send and the rollback in one helper so
/// the wiring between them — the load-bearing invariant per WCP D4 — is
/// testable end-to-end without standing up a full `AppState`.
async fn send_and_recover(
    pool: &SqlitePool,
    queue: &SharedBuildQueue,
    log_watchers: &LogWatchers,
    worker_senders: &WorkerSenders,
    dispatch_info: &DispatchInfo,
    json_text: String,
    tar_gz_bytes: Vec<u8>,
) -> Result<(), DispatchError> {
    let send_result = {
        let senders = worker_senders.lock().await;
        if let Some(tx) = senders.get(&dispatch_info.connection_id) {
            tx.send(Message::Text(json_text.into()))
                .and_then(|()| tx.send(Message::Binary(tar_gz_bytes.into())))
                .map_err(|e| DispatchError::Send(e.to_string()))
        } else {
            Err(DispatchError::Send(format!(
                "no sender for connection {}",
                dispatch_info.connection_id
            )))
        }
    };

    if let Err(e) = send_result {
        handle_dispatch_send_failure(
            pool,
            queue,
            log_watchers,
            dispatch_info.build_id,
            &dispatch_info.connection_id,
            &e,
        )
        .await;
        return Err(e);
    }
    Ok(())
}

/// On reconnect-`Building` with DB state `dispatched` (the worker is in the
/// `accepted` phase per audit-rem D11): attach the new connection to the
/// existing active entry, mark the receipt `ReceivedByWorker`, and cancel the
/// dispatch-ack timer — but keep the build `dispatched`.
///
/// This treats the reconnect-`Building` as an authoritative receipt of
/// `build_accepted` (the original may have been lost in the disconnect).
/// Cancelling the ack timer is load-bearing: otherwise a stale timer fires and
/// `rollback_active_to_queued` re-queues a build the worker is already running,
/// redispatching it to a second worker (double execution). SM-S advances to
/// `started` only when the worker's subprocess sends `build_started`; the
/// ownership check at that point (WCP G10) sees the `connection_id` set here.
pub async fn attach_connection_and_mark_received(
    queue: &SharedBuildQueue,
    build_id: i64,
    connection_id: &str,
) {
    let mut q = queue.lock().await;
    if let Some(ab) = q.active.get_mut(&build_id) {
        ab.connection_id = connection_id.to_string();
        ab.ack_cancel.cancel();
        ab.receipt = crate::queue::ActiveAssignmentReceipt::ReceivedByWorker;
    }
}

/// Handle a `BuildFinished` message from a worker.
///
/// Updates DB state (success/failure/revoked), removes from `queue.active`,
/// drops the watch sender from `log_watchers`, and attempts to dispatch the
/// next queued build if the worker is now idle.
///
/// `build_report` is the serialized JSON report produced by cbscore. Only
/// the success path provides a report; all other paths pass `None`.
pub async fn handle_build_finished(
    state: &AppState,
    connection_id: &str,
    build_id: i64,
    status: &str,
    error: Option<&str>,
    build_report: Option<&str>,
) {
    // Update DB.
    match db::builds::set_build_finished(&state.pool, build_id, status, error, build_report).await {
        Ok(true) => {
            tracing::info!(build_id = build_id, status = status, "build finished");
        }
        Ok(false) => {
            tracing::warn!(
                build_id = build_id,
                status = status,
                "build finished but no DB row updated (stale?)"
            );
        }
        Err(e) => {
            tracing::error!(
                build_id = build_id,
                "failed to update build state to {status}: {e}"
            );
        }
    }

    // Remove from active builds.
    {
        let mut queue = state.queue.lock().await;
        queue.active.remove(&build_id);
    }

    // Finalize the build log (drop seq index, mark DB finished).
    crate::logs::writer::finish_build_log(&state.log_writer, &state.pool, build_id).await;

    // Drop watch sender (signals SSE followers that the log is done).
    {
        let mut watchers = state.log_watchers.lock().await;
        watchers.remove(&build_id);
    }

    // Worker is now idle — try to dispatch the next queued build.
    tracing::debug!(
        connection_id = %connection_id,
        "worker idle after build {build_id} — attempting next dispatch"
    );
    if let Err(DispatchError::NothingToDispatch) = try_dispatch(state).await {
        tracing::debug!("no pending builds to dispatch");
    }
}

/// Handle a `BuildRejected` message from a worker.
///
/// If the reason contains "integrity", the build is marked as FAILURE (bad
/// tarball, not worth retrying). Otherwise, the build is re-queued at the
/// front of its priority lane and dispatch is retried with the next worker.
pub async fn handle_build_rejected(
    state: &AppState,
    connection_id: &str,
    build_id: i64,
    reason: &str,
) {
    if reason.to_lowercase().contains("integrity") {
        // Integrity failure — mark as failed, do not re-queue. The
        // canonical `cleanup_terminal_state` helper ensures the
        // `build_logs.finished` flag is set in the same operation, so
        // SSE log streams unblock immediately (review v3 finding NA2).
        tracing::error!(
            build_id = build_id,
            connection_id = %connection_id,
            reason = %reason,
            "build rejected (integrity failure) — marking as failed"
        );

        crate::ws::handler::cleanup_terminal_state(
            &state.pool,
            &state.queue,
            &state.log_watchers,
            build_id,
            crate::ws::handler::TerminalStatus::Failure,
            reason,
        )
        .await;
    } else {
        // Transient rejection — re-queue at front.
        tracing::warn!(
            build_id = build_id,
            connection_id = %connection_id,
            reason = %reason,
            "build rejected — re-queuing"
        );

        crate::metrics::lifecycle::record_requeue("rejected");
        rollback_active_to_queued(&state.pool, &state.queue, &state.log_watchers, build_id).await;

        // Try to dispatch to another worker.
        if let Err(DispatchError::NothingToDispatch) = try_dispatch(state).await {
            tracing::debug!("no workers available to retry rejected build {build_id}");
        }
    }
}

/// Send a `ServerMessage` text frame to a specific connection, ignoring
/// missing senders. Used by reactive responses (`UnauthorizedBuildAction`,
/// reporter-directed `BuildRevoke`) that must not mutate any DB or queue state.
async fn send_text_to_connection(
    worker_senders: &WorkerSenders,
    connection_id: &str,
    msg: &ServerMessage,
) {
    let text = serde_json::to_string(msg).expect("ServerMessage serialization cannot fail");
    let senders = worker_senders.lock().await;
    if let Some(tx) = senders.get(connection_id) {
        let _ = tx.send(Message::Text(text.into()));
    }
}

/// Per WCP D1: check that `connection_id` currently owns `build_id`'s active
/// assignment. On unauthorized, sends `UnauthorizedBuildAction` (and, for
/// execution-evidence actions per WCP D2, a reporter-directed `BuildRevoke`)
/// and returns `false`. On authorized, cancels the dispatch-ack timer and
/// marks the receipt `ReceivedByWorker` under the queue lock.
///
/// Caller MUST early-return on `false`; the response has already been sent
/// and no further handler work should run for this message.
pub async fn authorize_lifecycle_message(
    queue: &SharedBuildQueue,
    worker_senders: &WorkerSenders,
    connection_id: &str,
    build_id: BuildId,
    action: cbsd_proto::ws::WorkerBuildAction,
) -> bool {
    use crate::queue::ActiveAssignmentReceipt;

    let owned = {
        let mut q = queue.lock().await;
        if let Some(ab) = q.active_build_for_connection_mut(build_id.0, connection_id) {
            ab.ack_cancel.cancel();
            ab.receipt = ActiveAssignmentReceipt::ReceivedByWorker;
            true
        } else {
            false
        }
    };

    if !owned {
        tracing::warn!(
            build_id = build_id.0,
            connection_id = %connection_id,
            ?action,
            "unauthorized lifecycle message: build not assigned to this connection"
        );
        send_unauthorized_action(
            worker_senders,
            connection_id,
            build_id,
            action,
            cbsd_proto::ws::UnauthorizedBuildReason::NotAssigned,
        )
        .await;
        if matches!(
            action,
            cbsd_proto::ws::WorkerBuildAction::BuildStarted
                | cbsd_proto::ws::WorkerBuildAction::BuildOutput
        ) {
            send_reporter_directed_revoke(worker_senders, connection_id, build_id).await;
        }
    }

    owned
}

/// Reply to an unauthorized lifecycle message: tells the reporting connection
/// it does not own `build_id`. Per WCP D2 the reply is non-fatal and exposes
/// only a coarse reason. Caller's responsibility to also send a
/// reporter-directed `BuildRevoke` for execution-evidence actions.
pub async fn send_unauthorized_action(
    worker_senders: &WorkerSenders,
    connection_id: &str,
    build_id: BuildId,
    action: cbsd_proto::ws::WorkerBuildAction,
    reason: cbsd_proto::ws::UnauthorizedBuildReason,
) {
    let msg = ServerMessage::UnauthorizedBuildAction {
        build_id,
        action,
        reason,
    };
    send_text_to_connection(worker_senders, connection_id, &msg).await;
}

/// Send a `BuildRevoke` to the reporting connection for an unauthorized
/// execution-evidence message (`build_started` / `build_output`). Per WCP D2,
/// this MUST NOT touch the real assignment's DB row, queue entry, ack timer,
/// or log watcher — it is reporter-directed cleanup only. Use
/// `send_build_revoke` for the state-mutating admin-initiated path.
pub async fn send_reporter_directed_revoke(
    worker_senders: &WorkerSenders,
    connection_id: &str,
    build_id: BuildId,
) {
    let msg = ServerMessage::BuildRevoke {
        build_id,
        reason: Some(BuildRevokeReason::UnauthorizedAction),
    };
    send_text_to_connection(worker_senders, connection_id, &msg).await;
}

/// Audit-rem D13 (option A): best-effort stop-work to a superseded same-worker
/// connection, then remove its sender — in that order, under a single
/// `worker_senders` lock so the send-before-remove ordering cannot be lost to a
/// later refactor. Sends `BuildRevoke { reason: MigrationSupersede }` on the OLD
/// sender for each migrated build, then drops the sender entry.
///
/// Reporter-directed cleanup only — does NOT touch DB / queue / ack-timer /
/// log-watcher state; the new connection is already the authoritative owner.
/// Delivery is genuinely best-effort: in the common reconnect case the old
/// connection is already gone (the worker runs one connection at a time and has
/// dropped the old socket), so the revoke is a no-op and only the sender removal
/// takes effect. See design 019 v2.
pub async fn revoke_and_remove_superseded(
    worker_senders: &WorkerSenders,
    old_connection_id: &str,
    build_ids: &[i64],
) {
    let mut senders = worker_senders.lock().await;
    if let Some(tx) = senders.get(old_connection_id) {
        for &build_id in build_ids {
            let msg = ServerMessage::BuildRevoke {
                build_id: BuildId(build_id),
                reason: Some(BuildRevokeReason::MigrationSupersede),
            };
            let json =
                serde_json::to_string(&msg).expect("ServerMessage serialization cannot fail");
            let revoke_send_ok = tx.send(Message::Text(json.into())).is_ok();
            tracing::info!(
                build_id,
                old_connection = %old_connection_id,
                revoke_send_ok,
                "D13: migration-supersede revoke sent on superseded connection"
            );
        }
    } else {
        tracing::info!(
            old_connection = %old_connection_id,
            "D13: superseded connection's sender already gone; migration revoke is a no-op"
        );
    }
    // Always remove the superseded sender, AFTER attempting the revoke. Holding
    // a single lock for send-then-remove makes that ordering un-reorderable.
    senders.remove(old_connection_id);
}

/// Send a `BuildRevoke` message to the worker running a build, transition the
/// build to "revoking" in the DB, and spawn a timeout task that will mark the
/// build REVOKED unilaterally if the worker does not acknowledge in time.
pub async fn send_build_revoke(state: &AppState, build_id: i64) -> Result<(), DispatchError> {
    // Find the connection that owns this build.
    let connection_id = {
        let queue = state.queue.lock().await;
        queue
            .active
            .get(&build_id)
            .map(|ab| ab.connection_id.clone())
    };

    let connection_id = connection_id
        .ok_or_else(|| DispatchError::Send(format!("build {build_id} not in active map")))?;

    // Transition DB to revoking.
    if let Err(e) = db::builds::set_build_revoking(&state.pool, build_id).await {
        tracing::error!(
            build_id = build_id,
            "failed to set build state to revoking: {e}"
        );
        return Err(DispatchError::Database(e));
    }

    // Send BuildRevoke to the worker.
    let msg = ServerMessage::BuildRevoke {
        build_id: BuildId(build_id),
        reason: Some(BuildRevokeReason::Admin),
    };
    let json_text = serde_json::to_string(&msg).expect("ServerMessage serialization cannot fail");

    {
        let senders = state.worker_senders.lock().await;
        if let Some(tx) = senders.get(&connection_id) {
            if let Err(e) = tx.send(Message::Text(json_text.into())) {
                tracing::warn!(
                    build_id = build_id,
                    connection_id = %connection_id,
                    "failed to send build_revoke to worker: {e}"
                );
            }
        } else {
            tracing::warn!(
                build_id = build_id,
                connection_id = %connection_id,
                "no sender for worker — revoke timeout will handle it"
            );
        }
    }

    tracing::info!(
        build_id = build_id,
        connection_id = %connection_id,
        "build_revoke sent — starting ack timeout"
    );

    // Spawn revoke ack timeout.
    let timeout_secs = state.config.timeouts.revoke_ack_timeout_secs;
    let state_clone = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(timeout_secs)).await;
        handle_revoke_timeout(&state_clone, build_id).await;
    });

    Ok(())
}

/// Called when the revoke ack timeout fires. If the build is still in
/// "revoking" state, marks it REVOKED unilaterally.
pub async fn handle_revoke_timeout(state: &AppState, build_id: i64) {
    // Check current DB state.
    let build = match db::builds::get_build(&state.pool, build_id).await {
        Ok(Some(b)) => b,
        Ok(None) => return,
        Err(e) => {
            tracing::error!(build_id = build_id, "revoke timeout DB lookup failed: {e}");
            return;
        }
    };

    if build.state != "revoking" {
        // Already finished (worker acked in time, or something else happened).
        return;
    }

    tracing::warn!(
        build_id = build_id,
        "revoke ack timeout — marking REVOKED unilaterally"
    );
    crate::metrics::lifecycle::record_revoke_ack_timeout();

    crate::ws::handler::cleanup_terminal_state(
        &state.pool,
        &state.queue,
        &state.log_watchers,
        build_id,
        crate::ws::handler::TerminalStatus::Revoked,
        "revoke ack timeout",
    )
    .await;
}

/// Start the periodic re-dispatch sweep. Returns a `JoinHandle` that can be
/// stored for clean shutdown.
///
/// Every 30 seconds, if there are pending builds and idle workers, calls
/// `try_dispatch` until either runs out.
pub fn start_periodic_sweep(state: &AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        // First tick fires immediately; skip it.
        interval.tick().await;

        loop {
            interval.tick().await;

            // Check if there is work to do before attempting dispatch.
            let should_dispatch = {
                let queue = state.queue.lock().await;
                queue.has_pending() && queue.has_idle_workers()
            };

            if should_dispatch {
                tracing::debug!("periodic sweep: pending builds + idle workers — dispatching");
                loop {
                    match try_dispatch(&state).await {
                        Ok(()) => {
                            // Dispatched one; try again.
                            continue;
                        }
                        Err(DispatchError::NothingToDispatch) => break,
                        Err(e) => {
                            tracing::warn!("periodic sweep dispatch error: {e}");
                            break;
                        }
                    }
                }
            }
        }
    })
}

/// Collected info from the queue lock needed for tarball packing and sending.
struct DispatchInfo {
    build_id: BuildId,
    priority: Priority,
    descriptor: cbsd_proto::BuildDescriptor,
    trace_id: String,
    connection_id: String,
    /// Unix time the build entered the queue, for the queue-wait metric.
    queued_at: i64,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use cbsd_proto::{
        Arch, BuildComponent, BuildDescriptor, BuildDestImage, BuildSignedOffBy, BuildTarget,
        Priority,
    };
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use tokio::sync::Mutex;

    use super::*;
    use crate::queue::BuildQueue;

    /// In-memory SQLite pool for one test, isolated by URL.
    async fn test_pool() -> SqlitePool {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let url = format!(
            "file:dispatch_test_{pid}_{id}?mode=memory&cache=shared",
            pid = std::process::id(),
        );
        let options = SqliteConnectOptions::from_str(&url)
            .expect("valid sqlite URL")
            .pragma("foreign_keys", "ON")
            .pragma("busy_timeout", "5000");
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .min_connections(1)
            .connect_with(options)
            .await
            .expect("pool");
        sqlx::migrate!("../migrations")
            .run(&pool)
            .await
            .expect("migrations");
        pool
    }

    fn sample_descriptor() -> BuildDescriptor {
        BuildDescriptor {
            version: "19.2.3".to_string(),
            channel: None,
            version_type: None,
            signed_off_by: BuildSignedOffBy {
                user: "u".to_string(),
                email: "u@e.com".to_string(),
            },
            dst_image: BuildDestImage {
                name: "img".to_string(),
                tag: "tag".to_string(),
            },
            components: vec![BuildComponent {
                name: "c".to_string(),
                git_ref: "main".to_string(),
                repo: None,
            }],
            build: BuildTarget {
                distro: "fedora".to_string(),
                os_version: "42".to_string(),
                artifact_type: "rpm".to_string(),
                arch: Arch::X86_64,
            },
        }
    }

    fn empty_queue() -> SharedBuildQueue {
        Arc::new(Mutex::new(BuildQueue::new()))
    }

    fn empty_log_watchers() -> LogWatchers {
        Arc::new(Mutex::new(HashMap::new()))
    }

    async fn seed_user(pool: &SqlitePool, email: &str) {
        sqlx::query!(
            "INSERT INTO users (email, name, active, is_robot) VALUES (?, ?, 1, 0)",
            email,
            email,
        )
        .execute(pool)
        .await
        .expect("seed user");
    }

    async fn insert_dispatched(pool: &SqlitePool) -> i64 {
        seed_user(pool, "u@e.com").await;
        let id = db::builds::insert_build(
            pool,
            r#"{"name":"t"}"#,
            "u@e.com",
            "normal",
            None,
            None,
            None,
        )
        .await
        .expect("insert");
        db::builds::set_build_dispatched(pool, id, "trace-1", "worker-1")
            .await
            .expect("dispatched");
        id
    }

    fn make_active(build_id: i64, connection_id: &str) -> ActiveBuild {
        ActiveBuild {
            build_id,
            connection_id: connection_id.to_string(),
            dispatched_at: tokio::time::Instant::now(),
            trace_id: "trace-1".to_string(),
            descriptor: sample_descriptor(),
            priority: Priority::Normal,
            ack_cancel: tokio_util::sync::CancellationToken::new(),
            receipt: crate::queue::ActiveAssignmentReceipt::AwaitingReceipt,
        }
    }

    #[tokio::test]
    async fn rollback_active_to_queued_resets_db_and_reenqueues_at_front() {
        let pool = test_pool().await;
        let queue = empty_queue();
        let watchers = empty_log_watchers();
        let build_id = insert_dispatched(&pool).await;

        let ack = tokio_util::sync::CancellationToken::new();
        {
            let mut q = queue.lock().await;
            q.active.insert(
                build_id,
                ActiveBuild {
                    ack_cancel: ack.clone(),
                    ..make_active(build_id, "conn-1")
                },
            );
        }
        {
            let (tx, _rx) = tokio::sync::watch::channel(());
            watchers.lock().await.insert(build_id, tx);
        }

        let rolled = rollback_active_to_queued(&pool, &queue, &watchers, build_id).await;
        assert!(rolled);
        assert!(ack.is_cancelled(), "ack timer should be cancelled");

        let q = queue.lock().await;
        assert!(!q.active.contains_key(&build_id));
        assert!(q.contains(BuildId(build_id)), "build re-enqueued");
        drop(q);

        let watchers = watchers.lock().await;
        assert!(!watchers.contains_key(&build_id));
        drop(watchers);

        // All six WCP D4 SI-6/SI-13 provenance columns must be NULL — this
        // is the regression guard for F1 (send-failure rollback skipping
        // the DB cleanup). The helper is now invoked from `try_dispatch`
        // step 12 on send failure, so the same invariant applies there.
        let build = db::builds::get_build(&pool, build_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(build.state, "queued");
        assert!(build.worker_id.is_none());
        assert!(build.trace_id.is_none());
        assert!(build.error.is_none());
        assert!(build.started_at.is_none());
        assert!(build.finished_at.is_none());
        assert!(build.build_report.is_none());
    }

    #[tokio::test]
    async fn rollback_active_to_queued_returns_false_when_not_active() {
        let pool = test_pool().await;
        let queue = empty_queue();
        let watchers = empty_log_watchers();

        let rolled = rollback_active_to_queued(&pool, &queue, &watchers, 1234).await;
        assert!(!rolled);
    }

    /// Regression guard for review v1 finding F1: `try_dispatch` step 12
    /// must clear the DB on send failure, not just the in-memory queue.
    /// The production code's step 12 is now a single call to
    /// `handle_dispatch_send_failure`, so reverting step 12 would replace
    /// this helper call with a divergent implementation and any change
    /// is visible in code review. This test pins the helper's contract:
    /// after invocation, all six WCP D4 provenance columns are NULL and
    /// the build is re-enqueued at front.
    #[tokio::test]
    async fn handle_dispatch_send_failure_clears_six_columns_and_requeues() {
        let pool = test_pool().await;
        let queue = empty_queue();
        let watchers = empty_log_watchers();
        let build_id = insert_dispatched(&pool).await;

        // Mirror production state at the point step 12 fires: the build
        // row in DB is `dispatched` with worker_id + trace_id set
        // (`set_build_dispatched` already did that in `insert_dispatched`),
        // and the in-memory queue holds an `ActiveBuild` + log watcher.
        let ack = tokio_util::sync::CancellationToken::new();
        {
            let mut q = queue.lock().await;
            q.active.insert(
                build_id,
                ActiveBuild {
                    ack_cancel: ack.clone(),
                    ..make_active(build_id, "conn-bad")
                },
            );
        }
        {
            let (tx, _rx) = tokio::sync::watch::channel(());
            watchers.lock().await.insert(build_id, tx);
        }

        let err = DispatchError::Send("simulated broken sender".to_string());
        handle_dispatch_send_failure(
            &pool,
            &queue,
            &watchers,
            BuildId(build_id),
            "conn-bad",
            &err,
        )
        .await;

        let build = db::builds::get_build(&pool, build_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(build.state, "queued");
        assert!(build.worker_id.is_none(), "worker_id must be cleared");
        assert!(build.trace_id.is_none(), "trace_id must be cleared");
        assert!(build.error.is_none());
        assert!(build.started_at.is_none());
        assert!(build.finished_at.is_none());
        assert!(build.build_report.is_none());

        let q = queue.lock().await;
        assert!(!q.active.contains_key(&build_id), "active entry removed");
        assert!(
            q.contains(BuildId(build_id)),
            "build re-enqueued at front of priority lane"
        );
        drop(q);

        let watchers = watchers.lock().await;
        assert!(
            !watchers.contains_key(&build_id),
            "log watcher must be removed"
        );
        assert!(ack.is_cancelled(), "ack timer must be cancelled");
    }

    /// Stage shared by the `send_and_recover_*` tests: insert a build in
    /// `dispatched` state, place its `ActiveBuild` + log watcher in the
    /// queue, and return the build id and ack cancellation token so
    /// callers can assert cancellation.
    async fn stage_dispatched_active(
        pool: &SqlitePool,
        queue: &SharedBuildQueue,
        log_watchers: &LogWatchers,
        connection_id: &str,
    ) -> (i64, tokio_util::sync::CancellationToken) {
        let build_id = insert_dispatched(pool).await;
        let ack = tokio_util::sync::CancellationToken::new();
        {
            let mut q = queue.lock().await;
            q.active.insert(
                build_id,
                ActiveBuild {
                    ack_cancel: ack.clone(),
                    ..make_active(build_id, connection_id)
                },
            );
        }
        {
            let (tx, _rx) = tokio::sync::watch::channel(());
            log_watchers.lock().await.insert(build_id, tx);
        }
        (build_id, ack)
    }

    fn make_dispatch_info(build_id: i64, connection_id: &str) -> DispatchInfo {
        DispatchInfo {
            build_id: BuildId(build_id),
            priority: Priority::Normal,
            descriptor: sample_descriptor(),
            trace_id: "trace-1".to_string(),
            connection_id: connection_id.to_string(),
            queued_at: 0,
        }
    }

    fn assert_db_rolled_back_to_queued(build: &crate::db::builds::BuildRecord) {
        assert_eq!(build.state, "queued");
        assert!(build.worker_id.is_none());
        assert!(build.trace_id.is_none());
        assert!(build.error.is_none());
        assert!(build.started_at.is_none());
        assert!(build.finished_at.is_none());
        assert!(build.build_report.is_none());
    }

    /// End-to-end regression guard for review v3 finding NA1: the send
    /// path AND the rollback wiring are exercised together by driving
    /// `send_and_recover` with a sender whose receiver has been dropped,
    /// which forces `tx.send` to fail. Pinning both the send attempt and
    /// the rollback together prevents a maintainer from severing step 12
    /// from `handle_dispatch_send_failure` without a test catching it.
    #[tokio::test]
    async fn send_and_recover_with_closed_receiver_rolls_back_db() {
        let pool = test_pool().await;
        let queue = empty_queue();
        let watchers = empty_log_watchers();
        let senders: WorkerSenders = Arc::new(Mutex::new(HashMap::new()));
        let (build_id, ack) =
            stage_dispatched_active(&pool, &queue, &watchers, "conn-broken").await;

        // Register a sender, then drop its receiver to force `tx.send`
        // to fail on the next call.
        {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            drop(rx);
            senders.lock().await.insert("conn-broken".to_string(), tx);
        }

        let info = make_dispatch_info(build_id, "conn-broken");
        let result = send_and_recover(
            &pool,
            &queue,
            &watchers,
            &senders,
            &info,
            "{}".to_string(),
            vec![0u8; 4],
        )
        .await;
        assert!(
            matches!(result, Err(DispatchError::Send(_))),
            "must surface a Send error, got {result:?}"
        );

        let build = db::builds::get_build(&pool, build_id)
            .await
            .expect("get")
            .expect("row");
        assert_db_rolled_back_to_queued(&build);

        let q = queue.lock().await;
        assert!(!q.active.contains_key(&build_id), "active entry removed");
        assert!(q.contains(BuildId(build_id)), "build re-enqueued");
        drop(q);

        assert!(
            !watchers.lock().await.contains_key(&build_id),
            "log watcher must be removed"
        );
        assert!(ack.is_cancelled(), "ack timer must be cancelled");
    }

    /// Companion to the closed-receiver test: covers the branch where the
    /// `WorkerSenders` map has no entry at all for the dispatched
    /// connection. `send_and_recover` must still take the rollback path.
    #[tokio::test]
    async fn send_and_recover_with_no_sender_for_connection_rolls_back_db() {
        let pool = test_pool().await;
        let queue = empty_queue();
        let watchers = empty_log_watchers();
        let senders: WorkerSenders = Arc::new(Mutex::new(HashMap::new()));
        let (build_id, ack) =
            stage_dispatched_active(&pool, &queue, &watchers, "conn-missing").await;

        // No sender registered for "conn-missing".
        let info = make_dispatch_info(build_id, "conn-missing");
        let result = send_and_recover(
            &pool,
            &queue,
            &watchers,
            &senders,
            &info,
            "{}".to_string(),
            vec![0u8; 4],
        )
        .await;
        assert!(
            matches!(result, Err(DispatchError::Send(_))),
            "must surface a Send error, got {result:?}"
        );

        let build = db::builds::get_build(&pool, build_id)
            .await
            .expect("get")
            .expect("row");
        assert_db_rolled_back_to_queued(&build);

        let q = queue.lock().await;
        assert!(!q.active.contains_key(&build_id), "active entry removed");
        assert!(
            q.contains(BuildId(build_id)),
            "build re-enqueued at front of priority lane"
        );
        drop(q);

        assert!(
            !watchers.lock().await.contains_key(&build_id),
            "log watcher must be removed"
        );
        assert!(ack.is_cancelled(), "ack timer must be cancelled");
    }

    #[tokio::test]
    async fn attach_connection_and_mark_received_keeps_dispatched_and_marks_receipt() {
        let pool = test_pool().await;
        let queue = empty_queue();
        let build_id = insert_dispatched(&pool).await;
        let ack = tokio_util::sync::CancellationToken::new();

        {
            let mut q = queue.lock().await;
            q.active.insert(
                build_id,
                ActiveBuild {
                    ack_cancel: ack.clone(),
                    ..make_active(build_id, "old-conn")
                },
            );
        }

        attach_connection_and_mark_received(&queue, build_id, "new-conn").await;

        // In-memory: new connection attached, receipt advanced, ack cancelled.
        {
            let q = queue.lock().await;
            let ab = q.active.get(&build_id).expect("still active");
            assert_eq!(ab.connection_id, "new-conn");
            assert_eq!(
                ab.receipt,
                crate::queue::ActiveAssignmentReceipt::ReceivedByWorker
            );
        }
        assert!(
            ack.is_cancelled(),
            "dispatch-ack timer must be cancelled so it cannot requeue a build \
             the worker is already running (D11 double-exec guard)"
        );

        // DB stays `dispatched`: SM-S advances to `started` only on a real
        // build_started, not on the reconnect itself.
        let build = db::builds::get_build(&pool, build_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(build.state, "dispatched");
        assert!(
            build.started_at.is_none(),
            "must not be marked started on reconnect"
        );
    }

    /// Register a synthetic worker sender for `connection_id`, returning the
    /// matching receiver so tests can assert on outgoing messages.
    async fn register_sender(
        worker_senders: &WorkerSenders,
        connection_id: &str,
    ) -> tokio::sync::mpsc::UnboundedReceiver<Message> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        worker_senders
            .lock()
            .await
            .insert(connection_id.to_string(), tx);
        rx
    }

    fn assert_unauthorized(
        msg: Option<Message>,
        expected_action: cbsd_proto::ws::WorkerBuildAction,
    ) {
        let Some(Message::Text(text)) = msg else {
            panic!("expected Text frame, got {msg:?}");
        };
        let parsed: ServerMessage = serde_json::from_str(text.as_str()).expect("parse");
        match parsed {
            ServerMessage::UnauthorizedBuildAction { action, reason, .. } => {
                assert_eq!(action, expected_action);
                assert_eq!(reason, cbsd_proto::ws::UnauthorizedBuildReason::NotAssigned);
            }
            other => panic!("expected UnauthorizedBuildAction, got {other:?}"),
        }
    }

    fn assert_build_revoke(msg: Option<Message>) {
        let Some(Message::Text(text)) = msg else {
            panic!("expected Text frame, got {msg:?}");
        };
        let parsed: ServerMessage = serde_json::from_str(text.as_str()).expect("parse");
        assert!(
            matches!(parsed, ServerMessage::BuildRevoke { .. }),
            "expected BuildRevoke, got {parsed:?}"
        );
    }

    #[tokio::test]
    async fn revoke_and_remove_superseded_sends_then_removes() {
        let senders: WorkerSenders = Arc::new(Mutex::new(HashMap::new()));
        let mut rx = register_sender(&senders, "old-conn").await;

        revoke_and_remove_superseded(&senders, "old-conn", &[7, 9]).await;

        // The revokes were delivered — which proves they were sent BEFORE the
        // sender was removed (a remove-first ordering would drop the sender and
        // the rx would receive nothing).
        for expected in [7i64, 9] {
            let msg = rx.try_recv().expect("a revoke per migrated build");
            let Message::Text(text) = msg else {
                panic!("expected Text frame, got {msg:?}");
            };
            let parsed: ServerMessage = serde_json::from_str(text.as_str()).expect("parse");
            match parsed {
                ServerMessage::BuildRevoke { build_id, reason } => {
                    assert_eq!(build_id, BuildId(expected));
                    assert_eq!(reason, Some(BuildRevokeReason::MigrationSupersede));
                }
                other => panic!("expected BuildRevoke, got {other:?}"),
            }
        }
        // ...and the superseded sender is gone afterward.
        assert!(
            !senders.lock().await.contains_key("old-conn"),
            "superseded sender must be removed after the revoke"
        );
    }

    #[tokio::test]
    async fn revoke_and_remove_superseded_noop_when_sender_absent() {
        let senders: WorkerSenders = Arc::new(Mutex::new(HashMap::new()));
        // No sender for "gone-conn" — the common reconnect case where the old
        // socket is already dropped. Must not panic; removing an absent key is
        // harmless.
        revoke_and_remove_superseded(&senders, "gone-conn", &[7]).await;
        assert!(!senders.lock().await.contains_key("gone-conn"));
    }

    #[tokio::test]
    async fn authorize_lifecycle_message_passes_owned_build_and_updates_receipt() {
        let queue = empty_queue();
        let senders: WorkerSenders = Arc::new(Mutex::new(HashMap::new()));
        let ack = tokio_util::sync::CancellationToken::new();
        {
            let mut q = queue.lock().await;
            q.active.insert(
                42,
                ActiveBuild {
                    ack_cancel: ack.clone(),
                    ..make_active(42, "owner")
                },
            );
        }

        let ok = authorize_lifecycle_message(
            &queue,
            &senders,
            "owner",
            BuildId(42),
            cbsd_proto::ws::WorkerBuildAction::BuildAccepted,
        )
        .await;
        assert!(ok);
        assert!(ack.is_cancelled());

        let q = queue.lock().await;
        let ab = q.active.get(&42).expect("still active");
        assert_eq!(
            ab.receipt,
            crate::queue::ActiveAssignmentReceipt::ReceivedByWorker
        );
    }

    #[tokio::test]
    async fn authorize_lifecycle_message_rejects_stranger_and_sends_unauthorized_only() {
        let queue = empty_queue();
        let senders: WorkerSenders = Arc::new(Mutex::new(HashMap::new()));
        let mut rx = register_sender(&senders, "stranger").await;
        {
            let mut q = queue.lock().await;
            q.active.insert(42, make_active(42, "owner"));
        }

        let ok = authorize_lifecycle_message(
            &queue,
            &senders,
            "stranger",
            BuildId(42),
            cbsd_proto::ws::WorkerBuildAction::BuildAccepted,
        )
        .await;
        assert!(!ok);

        assert_unauthorized(
            rx.recv().await,
            cbsd_proto::ws::WorkerBuildAction::BuildAccepted,
        );
        // No reporter-directed revoke for `build_accepted` per WCP D2.
        assert!(
            rx.try_recv().is_err(),
            "build_accepted must not trigger reporter-directed revoke"
        );

        let q = queue.lock().await;
        let ab = q.active.get(&42).expect("real assignment untouched");
        assert_eq!(
            ab.receipt,
            crate::queue::ActiveAssignmentReceipt::AwaitingReceipt
        );
        assert_eq!(ab.connection_id, "owner");
    }

    #[tokio::test]
    async fn authorize_lifecycle_message_unauthorized_build_started_also_sends_reporter_revoke() {
        let queue = empty_queue();
        let senders: WorkerSenders = Arc::new(Mutex::new(HashMap::new()));
        let mut rx = register_sender(&senders, "stranger").await;
        {
            let mut q = queue.lock().await;
            q.active.insert(42, make_active(42, "owner"));
        }

        let ok = authorize_lifecycle_message(
            &queue,
            &senders,
            "stranger",
            BuildId(42),
            cbsd_proto::ws::WorkerBuildAction::BuildStarted,
        )
        .await;
        assert!(!ok);

        assert_unauthorized(
            rx.recv().await,
            cbsd_proto::ws::WorkerBuildAction::BuildStarted,
        );
        assert_build_revoke(rx.recv().await);
    }

    #[tokio::test]
    async fn authorize_lifecycle_message_unauthorized_build_output_also_sends_reporter_revoke() {
        let queue = empty_queue();
        let senders: WorkerSenders = Arc::new(Mutex::new(HashMap::new()));
        let mut rx = register_sender(&senders, "stranger").await;
        {
            let mut q = queue.lock().await;
            q.active.insert(42, make_active(42, "owner"));
        }

        let ok = authorize_lifecycle_message(
            &queue,
            &senders,
            "stranger",
            BuildId(42),
            cbsd_proto::ws::WorkerBuildAction::BuildOutput,
        )
        .await;
        assert!(!ok);

        assert_unauthorized(
            rx.recv().await,
            cbsd_proto::ws::WorkerBuildAction::BuildOutput,
        );
        assert_build_revoke(rx.recv().await);
    }

    #[tokio::test]
    async fn authorize_lifecycle_message_non_evidence_actions_skip_revoke() {
        // Spoof rejection for BuildAccepted, BuildFinished, BuildRejected:
        // each sends UnauthorizedBuildAction but does NOT send a
        // reporter-directed BuildRevoke, since only execution-evidence
        // actions (build_started, build_output) trigger the revoke.
        for action in [
            cbsd_proto::ws::WorkerBuildAction::BuildAccepted,
            cbsd_proto::ws::WorkerBuildAction::BuildFinished,
            cbsd_proto::ws::WorkerBuildAction::BuildRejected,
        ] {
            let queue = empty_queue();
            let senders: WorkerSenders = Arc::new(Mutex::new(HashMap::new()));
            let mut rx = register_sender(&senders, "stranger").await;
            {
                let mut q = queue.lock().await;
                q.active.insert(42, make_active(42, "owner"));
            }

            let ok = authorize_lifecycle_message(&queue, &senders, "stranger", BuildId(42), action)
                .await;
            assert!(!ok, "action={action:?}");

            assert_unauthorized(rx.recv().await, action);
            assert!(
                rx.try_recv().is_err(),
                "action={action:?} must not trigger reporter-directed revoke"
            );
        }
    }

    /// End-to-end regression guard for review v4 finding NB1 (Phase 1
    /// carry-over): drives `try_dispatch` itself with an idle worker
    /// whose outbound channel is closed, forcing step 11's `tx.send` to
    /// fail. Asserts step 12 cleared all six WCP D4 SI-6/SI-13
    /// provenance columns, removed the active entry and log watcher,
    /// and re-enqueued the build in its priority lane.
    #[tokio::test]
    async fn try_dispatch_send_failure_rolls_back_db_end_to_end() {
        use crate::routes::test_support::{temp_component_dir, test_app_state_with_components_dir};
        use crate::ws::liveness::WorkerState;
        use cbsd_proto::Arch;

        let pool = test_pool().await;
        let component_name = "c"; // matches `sample_descriptor`'s component
        let tempdir = temp_component_dir(component_name);
        let state = test_app_state_with_components_dir(pool.clone(), tempdir.path().to_path_buf());

        seed_user(&pool, "u@e.com").await;
        let descriptor = sample_descriptor();
        let descriptor_json = serde_json::to_string(&descriptor).expect("serialize");
        let build_id = db::builds::insert_build(
            &pool,
            &descriptor_json,
            "u@e.com",
            "normal",
            None,
            None,
            None,
        )
        .await
        .expect("insert");

        {
            let mut q = state.queue.lock().await;
            q.enqueue(QueuedBuild {
                build_id: BuildId(build_id),
                priority: Priority::Normal,
                descriptor: descriptor.clone(),
                user_email: "u@e.com".to_string(),
                queued_at: 0,
            });
            q.workers.insert(
                "test-conn".to_string(),
                WorkerState::Connected {
                    registered_worker_id: "worker-1".to_string(),
                    worker_name: "test-worker".to_string(),
                    arch: Arch::X86_64,
                    cores_total: 4,
                    ram_total_mb: 1024,
                    version: None,
                },
            );
        }

        // Drop the receiver so step 11's `tx.send` fails synchronously.
        {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            drop(rx);
            state
                .worker_senders
                .lock()
                .await
                .insert("test-conn".to_string(), tx);
        }

        let result = try_dispatch(&state).await;
        assert!(
            matches!(result, Err(DispatchError::Send(_))),
            "expected DispatchError::Send, got {result:?}"
        );

        let build = db::builds::get_build(&pool, build_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(build.state, "queued");
        assert!(build.worker_id.is_none(), "worker_id must be NULL");
        assert!(build.trace_id.is_none(), "trace_id must be NULL");
        assert!(build.error.is_none());
        assert!(build.started_at.is_none());
        assert!(build.finished_at.is_none());
        assert!(build.build_report.is_none());

        let q = state.queue.lock().await;
        assert!(!q.active.contains_key(&build_id));
        assert!(
            q.contains(BuildId(build_id)),
            "build re-enqueued at front of priority lane"
        );
        drop(q);

        assert!(
            !state.log_watchers.lock().await.contains_key(&build_id),
            "log watcher must be removed"
        );

        drop(tempdir); // keep the component dir alive until after assertions
    }

    /// Regression guard: a deterministic dispatch tarball-pack failure must
    /// mark the build FAILURE — not re-queue it. Re-queueing would push the
    /// build back to its lane head (`enqueue_front`) and the next dispatch
    /// trigger would re-pop and re-fail forever, since the missing component
    /// directory never appears. Drives `try_dispatch` with a `components_dir`
    /// that does not contain the referenced component subdir, so
    /// `pack_component`'s `append_dir_all` fails on the missing source
    /// directory.
    #[tokio::test]
    async fn try_dispatch_pack_failure_fails_build_end_to_end() {
        use crate::routes::test_support::test_app_state_with_components_dir;
        use crate::ws::liveness::WorkerState;
        use cbsd_proto::Arch;

        let pool = test_pool().await;
        // Empty components dir: no subdir for the descriptor's "c" component,
        // so packing fails on the missing source directory.
        let tempdir = tempfile::TempDir::new().expect("tempdir");
        let state = test_app_state_with_components_dir(pool.clone(), tempdir.path().to_path_buf());

        seed_user(&pool, "u@e.com").await;
        let descriptor = sample_descriptor();
        let descriptor_json = serde_json::to_string(&descriptor).expect("serialize");
        let build_id = db::builds::insert_build(
            &pool,
            &descriptor_json,
            "u@e.com",
            "normal",
            None,
            None,
            None,
        )
        .await
        .expect("insert");

        {
            let mut q = state.queue.lock().await;
            q.enqueue(QueuedBuild {
                build_id: BuildId(build_id),
                priority: Priority::Normal,
                descriptor: descriptor.clone(),
                user_email: "u@e.com".to_string(),
                queued_at: 0,
            });
            q.workers.insert(
                "test-conn".to_string(),
                WorkerState::Connected {
                    registered_worker_id: "worker-1".to_string(),
                    worker_name: "test-worker".to_string(),
                    arch: Arch::X86_64,
                    cores_total: 4,
                    ram_total_mb: 1024,
                    version: None,
                },
            );
        }

        let result = try_dispatch(&state).await;
        assert!(
            matches!(result, Err(DispatchError::Tarball(_))),
            "expected DispatchError::Tarball, got {result:?}"
        );

        // The build is terminal FAILURE, with the failure reason recorded.
        let build = db::builds::get_build(&pool, build_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(build.state, "failure");
        assert!(build.finished_at.is_some(), "finished_at must be set");
        assert!(
            build.error.as_deref().is_some_and(|e| e.contains("pack")),
            "error must record the pack failure, got {:?}",
            build.error
        );

        // It must NOT be re-enqueued (no poison-pill loop) and the active
        // entry + log watcher must be gone.
        let q = state.queue.lock().await;
        assert!(!q.active.contains_key(&build_id), "active entry removed");
        assert!(
            !q.contains(BuildId(build_id)),
            "failed build must not be re-enqueued"
        );
        drop(q);

        assert!(
            !state.log_watchers.lock().await.contains_key(&build_id),
            "log watcher must be removed"
        );

        drop(tempdir);
    }

    /// End-to-end: a multi-component build must pack EVERY referenced component
    /// into the single tarball sent to the worker. Drives `try_dispatch` with a
    /// 2-component descriptor and a live worker sender, then decodes the emitted
    /// binary frame and asserts both component subdirs are present. Before the
    /// fix only the first component was packed, so the second never reached the
    /// worker and cbscore failed with "unknown component".
    #[tokio::test]
    async fn try_dispatch_packs_all_components_into_tarball() {
        use crate::routes::test_support::{
            temp_components_dir, test_app_state_with_components_dir,
        };
        use crate::ws::liveness::WorkerState;
        use cbsd_proto::Arch;
        use flate2::read::GzDecoder;
        use std::collections::BTreeSet;

        let pool = test_pool().await;
        let tempdir = temp_components_dir(&["alpha", "beta"]);
        let state = test_app_state_with_components_dir(pool.clone(), tempdir.path().to_path_buf());

        seed_user(&pool, "u@e.com").await;
        let mut descriptor = sample_descriptor();
        descriptor.components = vec![
            BuildComponent {
                name: "alpha".to_string(),
                git_ref: "main".to_string(),
                repo: None,
            },
            BuildComponent {
                name: "beta".to_string(),
                git_ref: "main".to_string(),
                repo: None,
            },
        ];
        let descriptor_json = serde_json::to_string(&descriptor).expect("serialize");
        let build_id = db::builds::insert_build(
            &pool,
            &descriptor_json,
            "u@e.com",
            "normal",
            None,
            None,
            None,
        )
        .await
        .expect("insert");

        let mut rx = register_sender(&state.worker_senders, "test-conn").await;

        {
            let mut q = state.queue.lock().await;
            q.enqueue(QueuedBuild {
                build_id: BuildId(build_id),
                priority: Priority::Normal,
                descriptor: descriptor.clone(),
                user_email: "u@e.com".to_string(),
                queued_at: 0,
            });
            q.workers.insert(
                "test-conn".to_string(),
                WorkerState::Connected {
                    registered_worker_id: "worker-1".to_string(),
                    worker_name: "test-worker".to_string(),
                    arch: Arch::X86_64,
                    cores_total: 4,
                    ram_total_mb: 1024,
                    version: None,
                },
            );
        }

        try_dispatch(&state).await.expect("dispatch succeeds");

        // First frame: BuildNew JSON. Second frame: the component tarball.
        let first = rx.try_recv().expect("BuildNew frame");
        assert!(matches!(first, Message::Text(_)), "expected Text BuildNew");
        let Message::Binary(tarball) = rx.try_recv().expect("tarball frame") else {
            panic!("expected Binary tarball frame");
        };

        let decoder = GzDecoder::new(&tarball[..]);
        let mut archive = tar::Archive::new(decoder);
        let mut tops = BTreeSet::new();
        for entry in archive.entries().unwrap() {
            let entry = entry.unwrap();
            let path = entry.path().unwrap().into_owned();
            if let Some(seg) = path.components().next() {
                tops.insert(seg.as_os_str().to_string_lossy().into_owned());
            }
        }
        assert!(
            tops.contains("alpha"),
            "alpha/ missing from tarball: {tops:?}"
        );
        assert!(
            tops.contains("beta"),
            "beta/ missing from tarball: {tops:?}"
        );

        drop(tempdir);
    }
}
