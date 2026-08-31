use std::path::PathBuf;

use frank_agent::{ProviderMessage, RuntimeEvent, RuntimeManager, normalize_provider_frame};
use frank_protocol::{AgentPolicy, Provider};

fn fixture(name: &str) -> serde_json::Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures").join(name)).expect("fixture exists"),
    )
    .expect("fixture is valid JSON")
}

#[test]
fn checked_fixture_set_is_present_for_both_adapters() {
    for name in [
        "claude-system-init.json",
        "claude-assistant-tool-usage.json",
        "claude-result.json",
        "claude-stream-event.json",
        "claude-permission.json",
        "claude-error.json",
        "codex-thread-started.json",
        "codex-thread-resumed.json",
        "codex-delta.json",
        "codex-approval.json",
        "codex-token-usage.json",
        "codex-turn-completed.json",
        "codex-error.json",
    ] {
        assert!(!fixture(name).is_null());
    }
}

#[test]
fn provider_message_is_bounded_and_serializable() {
    let message = ProviderMessage {
        role: "user".into(),
        content: "objective".into(),
        correlation_id: Some("mission".into()),
    };
    assert_eq!(message.role, "user");
    assert!(serde_json::to_vec(&message).is_ok());
}

#[test]
fn checked_frames_normalize_without_text_scraping() {
    let claude = normalize_provider_frame(Provider::Claude, fixture("claude-system-init.json"));
    assert!(
        matches!(claude.as_slice(), [RuntimeEvent::Ready { provider_session_id }] if provider_session_id == "claude-fixture-session")
    );
    let codex = normalize_provider_frame(Provider::Codex, fixture("codex-approval.json"));
    assert!(
        matches!(codex.as_slice(), [RuntimeEvent::ApprovalRequest { operation, .. }] if operation == "shell")
    );
    let claude_stream =
        normalize_provider_frame(Provider::Claude, fixture("claude-stream-event.json"));
    assert!(matches!(
        claude_stream.as_slice(),
        [RuntimeEvent::Text { text }] if text == "streamed"
    ));
    let resumed = normalize_provider_frame(Provider::Codex, fixture("codex-thread-resumed.json"));
    assert!(matches!(
        resumed.as_slice(),
        [RuntimeEvent::Ready { provider_session_id }] if provider_session_id == "codex-resumed-thread"
    ));
    let usage = normalize_provider_frame(Provider::Codex, fixture("codex-turn-completed.json"));
    assert!(matches!(
        usage.as_slice(),
        [RuntimeEvent::Usage(value)] if value.measured_input_tokens == Some(21)
            && value.measured_output_tokens == Some(13)
        && value.cost_micros == Some(42)
    ));
    let current_usage =
        normalize_provider_frame(Provider::Codex, fixture("codex-token-usage.json"));
    assert!(matches!(
        current_usage.as_slice(),
        [RuntimeEvent::Usage(value)] if value.measured_input_tokens == Some(21)
            && value.measured_output_tokens == Some(13)
    ));
}

#[tokio::test]
async fn doctor_reports_structured_provider_capabilities() {
    let manager = RuntimeManager::new();
    let probes = manager.doctor().await;
    assert!(
        probes
            .iter()
            .any(|probe| probe.capability.provider == Provider::Claude)
    );
    assert!(
        probes
            .iter()
            .any(|probe| probe.capability.provider == Provider::Codex)
    );
}

#[test]
fn start_request_policy_is_explicit() {
    let policy = AgentPolicy::default();
    assert!(matches!(
        policy.filesystem,
        frank_protocol::FilesystemPolicy::WorkspaceWrite
    ));
    let _ = RuntimeEvent::Usage(frank_agent::UsageTelemetry::estimated(Some(1), Some(2)));
}
