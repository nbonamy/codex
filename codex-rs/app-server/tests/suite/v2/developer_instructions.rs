use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use codex_app_server_protocol::ThreadForkParams;
use codex_app_server_protocol::ThreadForkResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::UserInput;
use core_test_support::responses;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

const ORIGINAL_INSTRUCTIONS: &str = "Follow the original instructions.";
const REPLACEMENT_NOTICE: &str = "These thread-level developer instructions replace all previously provided thread-level developer instructions.";

enum ReloadKind {
    Resume,
    Fork,
}

impl ReloadKind {
    fn name(&self) -> &'static str {
        match self {
            Self::Resume => "resumed",
            Self::Fork => "forked",
        }
    }
}

#[tokio::test]
async fn thread_resume_applies_developer_instructions_override_to_first_turn() -> Result<()> {
    assert_reload_applies_developer_instructions(ReloadKind::Resume).await
}

#[tokio::test]
async fn thread_fork_applies_developer_instructions_override_to_first_turn() -> Result<()> {
    assert_reload_applies_developer_instructions(ReloadKind::Fork).await
}

async fn assert_reload_applies_developer_instructions(kind: ReloadKind) -> Result<()> {
    let updated_instructions = format!("Follow the {} instructions.", kind.name());
    let server = responses::start_mock_server().await;
    let response_mock = responses::mount_sse_sequence(
        &server,
        vec![
            responses::sse(vec![
                responses::ev_response_created("resp-1"),
                responses::ev_completed("resp-1"),
            ]),
            responses::sse(vec![
                responses::ev_response_created("resp-2"),
                responses::ev_completed("resp-2"),
            ]),
        ],
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;

    let mut primary = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    let ThreadStartResponse { thread, .. } = primary
        .start_thread(ThreadStartParams {
            developer_instructions: Some(ORIGINAL_INSTRUCTIONS.to_string()),
            ..Default::default()
        })
        .await?;
    primary
        .start_turn_and_wait_for_completion(TurnStartParams {
            thread_id: thread.id.clone(),
            input: text_input("seed history"),
            ..Default::default()
        })
        .await?;
    primary.shutdown_gracefully().await?;

    let mut secondary = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    let reloaded_thread = match kind {
        ReloadKind::Resume => {
            let request_id = secondary
                .send_thread_resume_request(ThreadResumeParams {
                    thread_id: thread.id,
                    developer_instructions: Some(updated_instructions.clone()),
                    ..Default::default()
                })
                .await?;
            let ThreadResumeResponse { thread, .. } = secondary.read_response(request_id).await?;
            thread
        }
        ReloadKind::Fork => {
            let request_id = secondary
                .send_thread_fork_request(ThreadForkParams {
                    thread_id: thread.id,
                    developer_instructions: Some(updated_instructions.clone()),
                    ..Default::default()
                })
                .await?;
            let ThreadForkResponse { thread, .. } = secondary.read_response(request_id).await?;
            thread
        }
    };
    secondary
        .start_turn_and_wait_for_completion(TurnStartParams {
            thread_id: reloaded_thread.id,
            input: text_input(&format!("first {} turn", kind.name())),
            ..Default::default()
        })
        .await?;

    let requests = response_mock.requests();
    assert_eq!(requests.len(), 2, "expected seed and reloaded requests");
    let initial_developer_text = requests[0].message_input_texts("developer").join("\n");
    assert_eq!(
        initial_developer_text
            .matches(ORIGINAL_INSTRUCTIONS)
            .count(),
        1,
        "initial developer instructions should be injected exactly once"
    );
    let developer_texts = requests[1].message_input_texts("developer");
    assert_eq!(
        developer_texts
            .iter()
            .filter(|text| text.as_str() == ORIGINAL_INSTRUCTIONS)
            .count(),
        1,
        "the steady-state diff should retain the original history; got {developer_texts:?}"
    );
    let expected_update = format!("{REPLACEMENT_NOTICE}\n\n{updated_instructions}");
    let developer_groups = requests[1].message_input_text_groups("developer");
    let latest_developer_group = developer_groups
        .last()
        .expect("expected a developer-instruction update");
    assert_eq!(
        latest_developer_group
            .iter()
            .filter(|text| text.as_str() == expected_update)
            .count(),
        1,
        "expected the replacement in the latest developer update; got {developer_groups:?}"
    );

    Ok(())
}

fn text_input(text: &str) -> Vec<UserInput> {
    vec![UserInput::Text {
        text: text.to_string(),
        text_elements: Vec::new(),
    }]
}
