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

//! Database operations for build records and build log metadata.

use serde::Serialize;
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use utoipa::ToSchema;

/// A build record as stored in the database.
///
/// The `build_report` field contains the structured artifact report produced
/// by cbscore after a successful build. It is stored as TEXT in SQLite but
/// deserialized to `serde_json::Value` so the API returns a nested JSON object.
/// The list endpoint excludes this field for performance.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BuildRecord {
    pub id: i64,
    /// Serialized BuildDescriptor; see BuildDescriptor schema.
    #[schema(value_type = Object)]
    pub descriptor: String,
    pub descriptor_version: i64,
    pub user_email: String,
    /// Whether `user_email` belongs to a robot account. Clients use this to
    /// render the submitter's display identity without string-parsing the
    /// synthetic `robot+<name>@robots` email.
    pub is_robot: bool,
    pub priority: String,
    pub state: String,
    pub worker_id: Option<String>,
    pub trace_id: Option<String>,
    pub error: Option<String>,
    pub submitted_at: i64,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    /// Structured artifact report produced by cbscore after a successful build.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>, nullable = true)]
    pub build_report: Option<Value>,
    pub channel_id: Option<i64>,
    pub channel_type_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_type_name: Option<String>,
}

/// A build record for list responses. Identical to `BuildRecord` but
/// without the potentially large `build_report` field.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BuildListRecord {
    pub id: i64,
    /// Serialized BuildDescriptor; see BuildDescriptor schema.
    #[schema(value_type = Object)]
    pub descriptor: String,
    pub descriptor_version: i64,
    pub user_email: String,
    /// Whether `user_email` belongs to a robot account.
    pub is_robot: bool,
    pub priority: String,
    pub state: String,
    pub worker_id: Option<String>,
    pub trace_id: Option<String>,
    pub error: Option<String>,
    pub submitted_at: i64,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub channel_id: Option<i64>,
    pub channel_type_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_type_name: Option<String>,
}

/// Insert a new build in QUEUED state. Returns the auto-generated build ID.
/// `periodic_task_id` is set for scheduler-triggered builds, `None` for manual.
/// `channel_id` and `channel_type_id` track the resolved channel/type mapping.
pub async fn insert_build(
    pool: &SqlitePool,
    descriptor_json: &str,
    user_email: &str,
    priority: &str,
    periodic_task_id: Option<&str>,
    channel_id: Option<i64>,
    channel_type_id: Option<i64>,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!(
        r#"INSERT INTO builds (descriptor, user_email, priority, state, periodic_task_id,
                               channel_id, channel_type_id)
         VALUES (?, ?, ?, 'queued', ?, ?, ?)
         RETURNING id AS "id!""#,
        descriptor_json,
        user_email,
        priority,
        periodic_task_id,
        channel_id,
        channel_type_id,
    )
    .fetch_one(pool)
    .await?;

    Ok(row.id)
}

/// Get a single build by ID, with channel/type names via LEFT JOIN.
pub async fn get_build(pool: &SqlitePool, id: i64) -> Result<Option<BuildRecord>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT
                b.id            AS "id!",
                b.descriptor    AS "descriptor!",
                b.descriptor_version AS "descriptor_version!",
                b.user_email    AS "user_email!",
                u.is_robot      AS "is_robot?: i64",
                b.priority      AS "priority!",
                b.state         AS "state!",
                b.worker_id,
                b.trace_id,
                b.error,
                b.submitted_at  AS "submitted_at!",
                b.queued_at     AS "queued_at!",
                b.started_at,
                b.finished_at,
                b.build_report,
                b.channel_id,
                b.channel_type_id,
                c.name          AS "channel_name?",
                ct.type_name    AS "channel_type_name?"
         FROM builds b
         LEFT JOIN users u ON u.email = b.user_email
         LEFT JOIN channels c ON c.id = b.channel_id
         LEFT JOIN channel_types ct ON ct.id = b.channel_type_id
         WHERE b.id = ?"#,
        id,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        let build_report = r.build_report.and_then(|s| serde_json::from_str(&s).ok());
        BuildRecord {
            id: r.id,
            descriptor: r.descriptor,
            descriptor_version: r.descriptor_version,
            user_email: r.user_email,
            is_robot: r.is_robot.unwrap_or(0) != 0,
            priority: r.priority,
            state: r.state,
            worker_id: r.worker_id,
            trace_id: r.trace_id,
            error: r.error,
            submitted_at: r.submitted_at,
            queued_at: r.queued_at,
            started_at: r.started_at,
            finished_at: r.finished_at,
            build_report,
            channel_id: r.channel_id,
            channel_type_id: r.channel_type_id,
            channel_name: r.channel_name,
            channel_type_name: r.channel_type_name,
        }
    }))
}

/// List builds with optional filters on user email and state.
///
/// The list query intentionally omits `build_report` to avoid expensive
/// responses when hundreds of builds each carry KB of report JSON.
/// Includes channel/type names via LEFT JOIN.
pub async fn list_builds(
    pool: &SqlitePool,
    user_filter: Option<&str>,
    state_filter: Option<&str>,
) -> Result<Vec<BuildListRecord>, sqlx::Error> {
    // Build the query dynamically based on filters.
    // NOTE: build_report is intentionally excluded from the list query.
    let base = "SELECT b.id, b.descriptor, b.descriptor_version, b.user_email, u.is_robot,
                       b.priority, b.state, b.worker_id, b.trace_id, b.error, b.submitted_at,
                       b.queued_at, b.started_at, b.finished_at, b.channel_id, b.channel_type_id,
                       c.name AS channel_name, ct.type_name AS channel_type_name
                FROM builds b
                LEFT JOIN users u ON u.email = b.user_email
                LEFT JOIN channels c ON c.id = b.channel_id
                LEFT JOIN channel_types ct ON ct.id = b.channel_type_id";

    let mut conditions: Vec<String> = Vec::new();
    if user_filter.is_some() {
        conditions.push("b.user_email = ?".to_string());
    }
    if state_filter.is_some() {
        conditions.push("b.state = ?".to_string());
    }

    let query_str = if conditions.is_empty() {
        format!("{base} ORDER BY b.id DESC")
    } else {
        format!(
            "{base} WHERE {} ORDER BY b.id DESC",
            conditions.join(" AND ")
        )
    };

    let mut query = sqlx::query(&query_str);

    if let Some(user) = user_filter {
        query = query.bind(user.to_string());
    }
    if let Some(state) = state_filter {
        query = query.bind(state.to_string());
    }

    let rows = query.fetch_all(pool).await?;
    Ok(rows.into_iter().map(row_to_build_list_record).collect())
}

/// Update a build's state. Optionally sets the error message.
/// Returns `true` if a row was updated.
pub async fn update_build_state(
    pool: &SqlitePool,
    id: i64,
    new_state: &str,
    error: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE builds SET state = ?, error = COALESCE(?, error)
         WHERE id = ?",
        new_state,
        error,
        id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Roll a build back to QUEUED after a failed or abandoned dispatch,
/// clearing every assignment-provenance column in a single statement.
///
/// Per WCP D4 (SI-6, SI-13), a build that returns to `queued` from an
/// abandoned dispatch must not retain stale provenance from the attempt:
/// `worker_id`, `trace_id`, `error`, `started_at`, `finished_at`, and
/// `build_report` are all reset to NULL. The generic `update_build_state`
/// helper does not own this reset list and is the wrong tool for the job.
///
/// Returns `true` if a row was updated.
pub async fn rollback_dispatch_to_queued(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE builds
         SET state = 'queued',
             worker_id = NULL,
             trace_id = NULL,
             error = NULL,
             started_at = NULL,
             finished_at = NULL,
             build_report = NULL
         WHERE id = ?",
        id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Insert a build log metadata row. Called at submission time so the
/// SSE follow endpoint can find the row before dispatch.
pub async fn insert_build_log_row(
    pool: &SqlitePool,
    build_id: i64,
    log_path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO build_logs (build_id, log_path) VALUES (?, ?)",
        build_id,
        log_path,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Set the trace_id and worker_id on a build, and mark it as dispatched.
/// Returns `true` if a row was updated.
pub async fn set_build_dispatched(
    pool: &SqlitePool,
    id: i64,
    trace_id: &str,
    worker_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE builds SET state = 'dispatched', trace_id = ?, worker_id = ?
         WHERE id = ?",
        trace_id,
        worker_id,
        id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Mark a build as started and set started_at to the current time.
/// Returns `true` if a row was updated.
pub async fn set_build_started(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE builds SET state = 'started', started_at = unixepoch()
         WHERE id = ?",
        id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Mark a build as finished (success, failure, or revoked) and set finished_at.
/// Optionally records an error message and a build artifact report.
/// Returns `true` if a row was updated.
///
/// This is the universal terminal choke point — every finish path (worker
/// `build_finished`, integrity reject, dead-worker resolution, revoke timeout,
/// drain) routes through here — so it is also where the build-result counter
/// and duration histogram are emitted, each build counted exactly once.
pub async fn set_build_finished(
    pool: &SqlitePool,
    id: i64,
    state: &str,
    error: Option<&str>,
    build_report: Option<&str>,
) -> Result<bool, sqlx::Error> {
    // RETURNING gives the label values for the metrics in the same round-trip:
    // `arch` comes straight off the stored descriptor JSON; `periodic` is just
    // whether the build was scheduler-triggered.
    let row = sqlx::query!(
        r#"UPDATE builds
           SET state = ?, finished_at = unixepoch(),
               error = COALESCE(?, error), build_report = ?
           WHERE id = ?
           RETURNING
               worker_id AS "worker_id?: String",
               json_extract(descriptor, '$.build.arch') AS "arch?: String",
               (periodic_task_id IS NOT NULL) AS "periodic!: bool",
               started_at AS "started_at?: i64",
               finished_at AS "finished_at!: i64""#,
        state,
        error,
        build_report,
        id,
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(false);
    };

    // `finished_at` was just set by this UPDATE, so it is always present here;
    // only `started_at` can be absent (the F6 guard handles that).
    crate::metrics::builds::record_build_finished(
        state,
        row.arch.as_deref().unwrap_or("unknown"),
        row.worker_id.as_deref().unwrap_or("unknown"),
        row.periodic,
        row.started_at,
        Some(row.finished_at),
    );

    Ok(true)
}

/// Set a build's state to "revoking".
/// Returns `true` if a row was updated.
pub async fn set_build_revoking(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!("UPDATE builds SET state = 'revoking' WHERE id = ?", id,)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Mark a build log as finished (`build_logs.finished = 1`).
pub async fn set_build_log_finished(pool: &SqlitePool, build_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE build_logs SET finished = 1, updated_at = unixepoch() WHERE build_id = ?",
        build_id,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Map a sqlx Row to a `BuildListRecord`.
///
/// Used by `list_builds` which constructs dynamic SQL and returns untyped rows.
/// Does NOT read `build_report` — the list query omits it for performance.
fn row_to_build_list_record(r: sqlx::sqlite::SqliteRow) -> BuildListRecord {
    // `is_robot` comes from a LEFT JOIN on `users`; it is NULL only when the
    // build's user_email has no matching users row, which is unreachable in
    // a correctly-maintained DB but safely defaults to false.
    let is_robot_i64: Option<i64> = r.get("is_robot");
    BuildListRecord {
        id: r.get("id"),
        descriptor: r.get("descriptor"),
        descriptor_version: r.get("descriptor_version"),
        user_email: r.get("user_email"),
        is_robot: is_robot_i64.unwrap_or(0) != 0,
        priority: r.get("priority"),
        state: r.get("state"),
        worker_id: r.get("worker_id"),
        trace_id: r.get("trace_id"),
        error: r.get("error"),
        submitted_at: r.get("submitted_at"),
        queued_at: r.get("queued_at"),
        started_at: r.get("started_at"),
        finished_at: r.get("finished_at"),
        channel_id: r.get("channel_id"),
        channel_type_id: r.get("channel_type_id"),
        channel_name: r.get("channel_name"),
        channel_type_name: r.get("channel_type_name"),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::atomic::{AtomicU64, Ordering};

    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use super::*;

    /// Per-test in-memory SQLite pool with the project migrations applied.
    /// Each call returns a fresh, isolated DB.
    async fn test_pool() -> SqlitePool {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let url = format!(
            "file:builds_test_{pid}_{id}?mode=memory&cache=shared",
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

    #[tokio::test]
    async fn rollback_dispatch_to_queued_clears_all_provenance_columns() {
        let pool = test_pool().await;
        seed_user(&pool, "u@e.com").await;
        let id = insert_build(
            &pool,
            r#"{"name":"t"}"#,
            "u@e.com",
            "normal",
            None,
            None,
            None,
        )
        .await
        .expect("insert");

        // Drive the row into a "dirty" post-dispatch state covering every
        // column that the rollback operation must reset.
        sqlx::query!(
            "UPDATE builds
             SET state = 'started',
                 worker_id = 'w1',
                 trace_id = 't1',
                 error = 'e1',
                 started_at = 1,
                 finished_at = 2,
                 build_report = '{\"r\":1}'
             WHERE id = ?",
            id,
        )
        .execute(&pool)
        .await
        .expect("dirty");

        let rolled = rollback_dispatch_to_queued(&pool, id)
            .await
            .expect("rollback");
        assert!(rolled);

        let row = sqlx::query!(
            "SELECT state, worker_id, trace_id, error, started_at, finished_at, build_report
             FROM builds WHERE id = ?",
            id,
        )
        .fetch_one(&pool)
        .await
        .expect("fetch");
        assert_eq!(row.state, "queued");
        assert!(row.worker_id.is_none());
        assert!(row.trace_id.is_none());
        assert!(row.error.is_none());
        assert!(row.started_at.is_none());
        assert!(row.finished_at.is_none());
        assert!(row.build_report.is_none());
    }

    #[tokio::test]
    async fn rollback_dispatch_to_queued_returns_false_for_unknown_id() {
        let pool = test_pool().await;
        let rolled = rollback_dispatch_to_queued(&pool, 9999).await.expect("ok");
        assert!(!rolled);
    }

    #[tokio::test]
    async fn set_build_finished_extracts_arch_and_reports_row_match() {
        let pool = test_pool().await;
        seed_user(&pool, "u@e.com").await;
        // A descriptor whose `build.arch` the terminal metric query reads back.
        let descriptor = r#"{"build":{"arch":"x86_64"}}"#;
        let id = insert_build(&pool, descriptor, "u@e.com", "normal", None, None, None)
            .await
            .expect("insert");
        sqlx::query!(
            "UPDATE builds SET worker_id = 'w1', started_at = 100 WHERE id = ?",
            id,
        )
        .execute(&pool)
        .await
        .expect("dirty");

        // Lock the JSON path the RETURNING clause depends on.
        let arch: Option<String> = sqlx::query_scalar::<_, Option<String>>(
            "SELECT json_extract(descriptor, '$.build.arch') FROM builds WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("json_extract");
        assert_eq!(arch.as_deref(), Some("x86_64"));

        let updated = set_build_finished(&pool, id, "success", None, None)
            .await
            .expect("finish");
        assert!(updated, "an existing row must report a match");

        let missing = set_build_finished(&pool, 999_999, "success", None, None)
            .await
            .expect("finish-missing");
        assert!(!missing, "a missing row must report no match");
    }
}
