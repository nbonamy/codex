use super::*;
use crate::context::world_state::WorldState;
use pretty_assertions::assert_eq;

#[test]
fn developer_instructions_have_a_hard_context_limit() {
    let oversized = "x".repeat(approx_bytes_for_tokens(MAX_DEVELOPER_INSTRUCTIONS_TOKENS));

    let error = DeveloperInstructionsState::new(Some(&oversized))
        .expect_err("oversized developer instructions must be rejected");

    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn legacy_developer_instructions_are_explicitly_replaced() -> io::Result<()> {
    let legacy = ContextualUserFragment::into(DeveloperInstructions::new("old instructions"));
    let mut world_state = WorldState::default();
    world_state.add_section(DeveloperInstructionsState::new(Some("new instructions"))?);

    let updates = world_state
        .render_history_diff(/*previous*/ None, std::slice::from_ref(&legacy))
        .into_iter()
        .map(ContextualUserFragment::into_boxed_response_item)
        .collect::<Vec<_>>();

    assert_eq!(
        updates,
        vec![ContextualUserFragment::into(DeveloperInstructions::new(
            format!("{REPLACEMENT_NOTICE}\n\nnew instructions")
        ))]
    );
    Ok(())
}
