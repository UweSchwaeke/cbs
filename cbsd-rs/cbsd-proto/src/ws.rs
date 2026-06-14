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

use serde::{Deserialize, Serialize};

use crate::build::{BuildDescriptor, BuildId, Priority};

// ---------------------------------------------------------------------------
// Server → Worker messages
// ---------------------------------------------------------------------------

/// Messages sent from server to worker over the WebSocket connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Dispatch a build. Followed by a binary frame containing the component
    /// tar.gz. The worker verifies `component_sha256` against the binary frame.
    BuildNew {
        build_id: BuildId,
        trace_id: String,
        priority: Priority,
        descriptor: Box<BuildDescriptor>,
        component_sha256: String,
    },

    /// Cancel a build (running or not yet accepted). If the worker receives
    /// this before sending `build_accepted`, it responds with
    /// `build_finished(revoked)` immediately.
    BuildRevoke { build_id: BuildId },

    /// Connection accepted. Sent after validating the worker's `hello`.
    Welcome {
        protocol_version: u32,
        connection_id: String,
        /// Worker validates its backoff ceiling against this value.
        grace_period_secs: u64,
    },

    /// Connection or protocol error. Server closes the connection after this.
    Error {
        reason: String,
        min_version: Option<u32>,
        max_version: Option<u32>,
    },

    /// Server's reply to a worker lifecycle message that targeted a build the
    /// reporting connection does not own. Non-fatal: the worker MUST NOT
    /// close the connection in response. Per WCP D1/D2.
    UnauthorizedBuildAction {
        build_id: BuildId,
        action: WorkerBuildAction,
        reason: UnauthorizedBuildReason,
    },
}

/// Worker-to-server lifecycle action that can be rejected as unauthorized
/// when the reporting connection does not own the target build. Per WCP
/// D3, `WorkerStatus` is the first normative variant — it covers the case
/// where a reconnecting worker claims `Building` on a build it does not
/// actually own per the persisted `builds.worker_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerBuildAction {
    WorkerStatus,
    BuildAccepted,
    BuildStarted,
    BuildOutput,
    BuildFinished,
    BuildRejected,
}

/// Coarse reason exposed to workers for an unauthorized build action. Detail
/// stays in the server log; the wire enum is intentionally narrow per WCP D2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnauthorizedBuildReason {
    /// The build is not currently assigned to the reporting connection.
    NotAssigned,
}

// ---------------------------------------------------------------------------
// Worker → Server messages
// ---------------------------------------------------------------------------

/// Messages sent from worker to server over the WebSocket connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerMessage {
    /// First message after WebSocket connect (protocol v2). Auth is validated
    /// at HTTP upgrade, not in this message. The server derives worker identity
    /// from the API key used at upgrade — `worker_id` is no longer sent.
    Hello {
        protocol_version: u32,
        arch: crate::arch::Arch,
        cores_total: u32,
        ram_total_mb: u64,
        /// Worker binary version (e.g., "0.1.0+g3a7f2b1").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    },

    /// Sent on reconnect ONLY if the worker is currently executing a build.
    /// Its absence after `hello` implies the worker is idle.
    WorkerStatus {
        state: WorkerReportedState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        build_id: Option<BuildId>,
    },

    /// Worker will run the build.
    BuildAccepted { build_id: BuildId },

    /// Worker cannot run the build (busy, incompatible, integrity failure).
    BuildRejected { build_id: BuildId, reason: String },

    /// Build execution has started (container launched).
    BuildStarted { build_id: BuildId },

    /// Build output. Batched: flushed every 200ms or 50 lines. Per-line seq:
    /// `start_seq`, `start_seq+1`, ..., `start_seq+len(lines)-1`.
    BuildOutput {
        build_id: BuildId,
        start_seq: u64,
        lines: Vec<String>,
    },

    /// Build completed (success, failure, or revoked).
    BuildFinished {
        build_id: BuildId,
        status: BuildFinishedStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        build_report: Option<serde_json::Value>,
    },

    /// Worker is shutting down gracefully (protocol v2). The server identifies
    /// the worker from the connection map — `worker_id` is no longer sent.
    WorkerStopping { reason: String },
}

/// State reported by a worker on reconnect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerReportedState {
    Idle,
    Building,
}

/// Build completion status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildFinishedStatus {
    Success,
    Failure,
    Revoked,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::Arch;
    use crate::build::{
        BuildComponent, BuildDestImage, BuildSignedOffBy, BuildTarget, VersionType,
    };
    use serde_json::{Value, json};
    use strum::IntoEnumIterator;

    #[test]
    fn server_message_build_new_round_trip() {
        let msg = ServerMessage::BuildNew {
            build_id: BuildId(42),
            trace_id: "abc-123".to_string(),
            priority: Priority::High,
            descriptor: Box::new(BuildDescriptor {
                version: "19.2.3".to_string(),
                channel: Some("ces".to_string()),
                version_type: Some(VersionType::Release),
                signed_off_by: BuildSignedOffBy {
                    user: "Alice".to_string(),
                    email: "alice@clyso.com".to_string(),
                },
                dst_image: BuildDestImage {
                    name: "harbor.clyso.com/ces/ceph".to_string(),
                    tag: "v19.2.3".to_string(),
                },
                components: vec![BuildComponent {
                    name: "ceph".to_string(),
                    git_ref: "v19.2.3".to_string(),
                    repo: None,
                }],
                build: BuildTarget {
                    distro: "rockylinux".to_string(),
                    os_version: "el9".to_string(),
                    artifact_type: "rpm".to_string(),
                    arch: Arch::X86_64,
                },
            }),
            component_sha256: "e3b0c44...".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"build_new""#));
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        if let ServerMessage::BuildNew { build_id, .. } = parsed {
            assert_eq!(build_id, BuildId(42));
        } else {
            panic!("expected BuildNew");
        }
    }

    #[test]
    fn server_message_welcome_includes_grace_period() {
        let msg = ServerMessage::Welcome {
            protocol_version: 1,
            connection_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            grace_period_secs: 90,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""grace_period_secs":90"#));
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        if let ServerMessage::Welcome {
            grace_period_secs, ..
        } = parsed
        {
            assert_eq!(grace_period_secs, 90);
        } else {
            panic!("expected Welcome");
        }
    }

    #[test]
    fn worker_message_hello_round_trip() {
        let msg = WorkerMessage::Hello {
            protocol_version: 2,
            arch: Arch::Aarch64,
            cores_total: 16,
            ram_total_mb: 65536,
            version: Some("0.1.0+gtest123".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"hello""#));
        assert!(json.contains(r#""arch":"aarch64""#));
        assert!(!json.contains("worker_id"));
        let parsed: WorkerMessage = serde_json::from_str(&json).unwrap();
        if let WorkerMessage::Hello { arch, version, .. } = parsed {
            assert_eq!(arch, Arch::Aarch64);
            assert_eq!(version.as_deref(), Some("0.1.0+gtest123"));
        } else {
            panic!("expected Hello");
        }
    }

    #[test]
    fn worker_message_hello_arm64_alias() {
        // No version field in JSON — tests backwards compat via serde(default).
        let json = r#"{"type":"hello","protocol_version":2,"arch":"arm64","cores_total":8,"ram_total_mb":32768}"#;
        let parsed: WorkerMessage = serde_json::from_str(json).unwrap();
        if let WorkerMessage::Hello { arch, version, .. } = parsed {
            assert_eq!(arch, Arch::Aarch64);
            assert_eq!(version, None);
        } else {
            panic!("expected Hello");
        }
    }

    #[test]
    fn worker_message_build_output_per_line_seq() {
        let msg = WorkerMessage::BuildOutput {
            build_id: BuildId(7),
            start_seq: 70,
            lines: vec!["line1".into(), "line2".into(), "line3".into()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""start_seq":70"#));
        let parsed: WorkerMessage = serde_json::from_str(&json).unwrap();
        if let WorkerMessage::BuildOutput {
            start_seq, lines, ..
        } = parsed
        {
            assert_eq!(start_seq, 70);
            assert_eq!(lines.len(), 3);
        } else {
            panic!("expected BuildOutput");
        }
    }

    #[test]
    fn worker_message_build_finished_with_error() {
        let msg = WorkerMessage::BuildFinished {
            build_id: BuildId(42),
            status: BuildFinishedStatus::Failure,
            error: Some("RPM build failed".to_string()),
            build_report: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""error":"RPM build failed""#));
        assert!(!json.contains("build_report"));
    }

    #[test]
    fn worker_message_build_finished_no_error() {
        let msg = WorkerMessage::BuildFinished {
            build_id: BuildId(42),
            status: BuildFinishedStatus::Success,
            error: None,
            build_report: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("error"));
        assert!(!json.contains("build_report"));
    }

    #[test]
    fn worker_message_build_finished_with_report() {
        let report = serde_json::json!({
            "report_version": 1,
            "version": "19.2.3",
            "skipped": false,
        });
        let msg = WorkerMessage::BuildFinished {
            build_id: BuildId(42),
            status: BuildFinishedStatus::Success,
            error: None,
            build_report: Some(report),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("build_report"));
        assert!(json.contains("report_version"));
        let parsed: WorkerMessage = serde_json::from_str(&json).unwrap();
        if let WorkerMessage::BuildFinished { build_report, .. } = parsed {
            assert!(build_report.is_some());
        } else {
            panic!("expected BuildFinished");
        }
    }

    #[test]
    fn worker_message_build_finished_missing_report_defaults_none() {
        // Older workers won't send the build_report field.
        let json = r#"{"type":"build_finished","build_id":42,"status":"success"}"#;
        let parsed: WorkerMessage = serde_json::from_str(json).unwrap();
        if let WorkerMessage::BuildFinished { build_report, .. } = parsed {
            assert!(build_report.is_none());
        } else {
            panic!("expected BuildFinished");
        }
    }

    #[test]
    fn worker_status_idle_no_build_id() {
        let msg = WorkerMessage::WorkerStatus {
            state: WorkerReportedState::Idle,
            build_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("build_id"));
    }

    #[test]
    fn worker_status_building_with_id() {
        let msg = WorkerMessage::WorkerStatus {
            state: WorkerReportedState::Building,
            build_id: Some(BuildId(42)),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""build_id":42"#));
    }

    #[test]
    fn worker_build_action_worker_status_serdes_as_snake_case() {
        let msg = ServerMessage::UnauthorizedBuildAction {
            build_id: BuildId(11),
            action: WorkerBuildAction::WorkerStatus,
            reason: UnauthorizedBuildReason::NotAssigned,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""action":"worker_status""#));
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::UnauthorizedBuildAction { action, .. } => {
                assert_eq!(action, WorkerBuildAction::WorkerStatus);
            }
            _ => panic!("expected UnauthorizedBuildAction"),
        }
    }

    #[test]
    fn server_message_unauthorized_build_action_round_trip() {
        let msg = ServerMessage::UnauthorizedBuildAction {
            build_id: BuildId(7),
            action: WorkerBuildAction::BuildStarted,
            reason: UnauthorizedBuildReason::NotAssigned,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"unauthorized_build_action""#));
        assert!(json.contains(r#""action":"build_started""#));
        assert!(json.contains(r#""reason":"not_assigned""#));
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerMessage::UnauthorizedBuildAction {
                build_id,
                action,
                reason,
            } => {
                assert_eq!(build_id, BuildId(7));
                assert_eq!(action, WorkerBuildAction::BuildStarted);
                assert_eq!(reason, UnauthorizedBuildReason::NotAssigned);
            }
            _ => panic!("expected UnauthorizedBuildAction"),
        }
    }

    #[test]
    fn server_message_error_with_version_range() {
        let msg = ServerMessage::Error {
            reason: "unsupported protocol version 3; server supports 1".to_string(),
            min_version: Some(1),
            max_version: Some(1),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""min_version":1"#));
    }

    // -----------------------------------------------------------------------
    // SI-18 / D13-T6: `ServerMessage` must accept unknown fields, or a newer
    // peer's added field breaks deserialization on an older peer mid-rolling-
    // upgrade. serde rejects `#[serde(deny_unknown_fields)]` placed directly on
    // an enum variant at COMPILE time, so this runtime test guards the forms
    // that would otherwise slip through: the attribute on the `ServerMessage`
    // enum container, or on a standalone struct that a variant's payload is
    // refactored into. It also drags any newly added variant through
    // compile-time gates (witness, tag, sentinel) before it can land without a
    // deserialization case. See design 019 D13-T6.
    // -----------------------------------------------------------------------

    /// Build a valid `BuildDescriptor` for SI-18 test payloads. Mirrors
    /// `server_message_build_new_round_trip`'s explicit construction because
    /// `BuildDescriptor` and its nested types do not impl `Default`.
    fn test_descriptor() -> BuildDescriptor {
        BuildDescriptor {
            version: "test".to_string(),
            channel: None,
            version_type: None,
            signed_off_by: BuildSignedOffBy {
                user: "test".to_string(),
                email: "test@example.com".to_string(),
            },
            dst_image: BuildDestImage {
                name: "test-image".to_string(),
                tag: "test-tag".to_string(),
            },
            components: vec![BuildComponent {
                name: "test-component".to_string(),
                git_ref: "v0".to_string(),
                repo: None,
            }],
            build: BuildTarget {
                distro: "rockylinux".to_string(),
                os_version: "el9".to_string(),
                artifact_type: "rpm".to_string(),
                arch: Arch::X86_64,
            },
        }
    }

    /// JSON form of `test_descriptor()`. Always succeeds because
    /// `BuildDescriptor` derives `Serialize`.
    fn test_descriptor_json() -> Value {
        serde_json::to_value(test_descriptor()).unwrap()
    }

    /// Test-only companion enum mirroring `ServerMessage`'s variants without
    /// their associated data. `strum::EnumIter` derives `iter()`, which yields
    /// every variant — the runtime-enumeration mechanism that closes the
    /// "witness updated, case forgotten" gap.
    ///
    /// `Hash` is intentionally NOT derived: `cases()` is keyed by the wire
    /// string, not by this enum, so a `Hash` impl would be unused.
    #[derive(strum::EnumIter, Debug, Clone, Copy, PartialEq, Eq)]
    enum ServerMessageTag {
        BuildNew,
        BuildRevoke,
        Welcome,
        Error,
        UnauthorizedBuildAction,
    }

    impl ServerMessageTag {
        /// Compile-time witness. Exhaustive on `ServerMessage` — `rustc`
        /// rejects this with a non-exhaustive-match error if a new variant is
        /// added to `ServerMessage` without a corresponding arm. Each arm maps
        /// to a `ServerMessageTag` variant, so a missing tag-enum variant
        /// surfaces as an "unknown variant" compile error here too.
        fn from_message(msg: &ServerMessage) -> Self {
            match msg {
                ServerMessage::BuildNew { .. } => Self::BuildNew,
                ServerMessage::BuildRevoke { .. } => Self::BuildRevoke,
                ServerMessage::Welcome { .. } => Self::Welcome,
                ServerMessage::Error { .. } => Self::Error,
                ServerMessage::UnauthorizedBuildAction { .. } => Self::UnauthorizedBuildAction,
            }
        }

        /// Wire-format tag (the serde `"type"` discriminator). Exhaustive on
        /// `Self` — `rustc` forces an arm when a variant is added to
        /// `ServerMessageTag`.
        fn as_wire(&self) -> &'static str {
            match self {
                Self::BuildNew => "build_new",
                Self::BuildRevoke => "build_revoke",
                Self::Welcome => "welcome",
                Self::Error => "error",
                Self::UnauthorizedBuildAction => "unauthorized_build_action",
            }
        }
    }

    /// Construct a sentinel `ServerMessage` for a given tag. Exhaustive on the
    /// tag enum, so a new tag variant is compile-forced to add an arm. Every
    /// field is explicit because the underlying types do not impl `Default`.
    fn sentinel_for_tag(tag: ServerMessageTag) -> ServerMessage {
        match tag {
            ServerMessageTag::BuildNew => ServerMessage::BuildNew {
                build_id: BuildId(0),
                trace_id: String::new(),
                priority: Priority::default(),
                descriptor: Box::new(test_descriptor()),
                component_sha256: String::new(),
            },
            ServerMessageTag::BuildRevoke => ServerMessage::BuildRevoke {
                build_id: BuildId(0),
            },
            ServerMessageTag::Welcome => ServerMessage::Welcome {
                protocol_version: 2,
                connection_id: String::new(),
                grace_period_secs: 0,
            },
            ServerMessageTag::Error => ServerMessage::Error {
                reason: String::new(),
                min_version: None,
                max_version: None,
            },
            ServerMessageTag::UnauthorizedBuildAction => ServerMessage::UnauthorizedBuildAction {
                build_id: BuildId(0),
                action: WorkerBuildAction::WorkerStatus,
                reason: UnauthorizedBuildReason::NotAssigned,
            },
        }
    }

    /// JSON payloads for the SI-18 deserialization check, keyed by wire-format
    /// tag. Each payload matches its variant's field schema plus an injected
    /// `future_field` to exercise the unknown-field path.
    ///
    /// `cases()` is the only coordinated list NOT compile-forced; a missing
    /// entry trips the runtime assertion in
    /// `no_deny_unknown_fields_on_server_message`.
    fn cases() -> Vec<(&'static str, Value)> {
        vec![
            (
                "build_new",
                json!({
                    "type": "build_new",
                    "build_id": 42,
                    "trace_id": "00000000-0000-0000-0000-000000000000",
                    "priority": "normal",
                    "descriptor": test_descriptor_json(),
                    "component_sha256": "0".repeat(64),
                    "future_field": "x",
                }),
            ),
            (
                "build_revoke",
                json!({
                    "type": "build_revoke",
                    "build_id": 42,
                    "future_field": "x",
                }),
            ),
            (
                "welcome",
                json!({
                    "type": "welcome",
                    "protocol_version": 2,
                    "connection_id": "test-conn-id",
                    "grace_period_secs": 60,
                    "future_field": "x",
                }),
            ),
            (
                "error",
                json!({
                    "type": "error",
                    "reason": "test",
                    "min_version": null,
                    "max_version": null,
                    "future_field": "x",
                }),
            ),
            (
                "unauthorized_build_action",
                json!({
                    "type": "unauthorized_build_action",
                    "build_id": 42,
                    "action": "worker_status",
                    "reason": "not_assigned",
                    "future_field": "x",
                }),
            ),
        ]
    }

    #[test]
    fn no_deny_unknown_fields_on_server_message() {
        let cases_map: std::collections::HashMap<&'static str, Value> =
            cases().into_iter().collect();

        // Runtime exhaustiveness over ALL ServerMessageTag variants.
        // `iter()` (strum) auto-extends when a variant is added. The sentinel
        // match is compile-forced; verify the witness round-trips the sentinel
        // and that a case exists for the tag.
        for tag in ServerMessageTag::iter() {
            let wire = tag.as_wire();
            let sentinel = sentinel_for_tag(tag);
            let witnessed = ServerMessageTag::from_message(&sentinel);
            assert_eq!(
                tag, witnessed,
                "sentinel/witness drift for tag `{wire}`: from_message returned \
                 a different ServerMessageTag",
            );
            assert!(
                cases_map.contains_key(wire),
                "ServerMessageTag::{tag:?} (wire `{wire}`) has no entry in \
                 cases() — SI-18 is not enforced for this variant. Add a case \
                 to cases() in cbsd-proto/src/ws.rs.",
            );
        }

        // Per-variant deserialization: each payload carries an unknown
        // `future_field`; deserialization must succeed. Fails if
        // `deny_unknown_fields` is added to the `ServerMessage` enum (or to a
        // standalone struct a variant's payload is refactored into).
        for (wire, payload) in cases_map {
            let result: Result<ServerMessage, _> = serde_json::from_value(payload);
            assert!(
                result.is_ok(),
                "ServerMessage `{wire}` rejected an unknown field — likely \
                 `#[serde(deny_unknown_fields)]` was added to the ServerMessage \
                 enum or to this variant's payload struct; that violates SI-18 \
                 and breaks rolling upgrades. See design 019 D13-T6. Error: {:?}",
                result.err(),
            );
            // Confirm the deserialized variant matches its expected wire tag.
            let msg = result.unwrap();
            let witnessed = ServerMessageTag::from_message(&msg).as_wire();
            assert_eq!(
                wire, witnessed,
                "case payload tagged `{wire}` deserialized to wire tag \
                 `{witnessed}` — case-tag/payload-type drift",
            );
        }
    }
}
