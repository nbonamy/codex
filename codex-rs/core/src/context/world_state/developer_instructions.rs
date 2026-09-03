use super::PreviousSectionState;
use super::WorldStateHash;
use super::WorldStateSection;
use crate::context::ContextualUserFragment;
use crate::context::DeveloperInstructions;
use codex_utils_string::approx_bytes_for_tokens;
use serde::Deserialize;
use serde::Serialize;
use std::io;

const MAX_DEVELOPER_INSTRUCTIONS_TOKENS: usize = 10_000;
const REPLACEMENT_NOTICE: &str = "These thread-level developer instructions replace all previously provided thread-level developer instructions.";
const REMOVAL_NOTICE: &str = "The previously provided developer instructions no longer apply.";

/// Plain thread-level developer instructions currently visible to the model.
#[derive(Clone, Debug)]
pub(crate) struct DeveloperInstructionsState {
    instructions: Option<DeveloperInstructions>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DeveloperInstructionsSnapshot {
    instructions: Option<WorldStateHash>,
}

impl DeveloperInstructionsState {
    pub(crate) fn new(instructions: Option<&str>) -> io::Result<Self> {
        let rendered_bytes = instructions
            .map_or(0, str::len)
            .saturating_add(REPLACEMENT_NOTICE.len().saturating_add("\n\n".len()));
        if rendered_bytes > approx_bytes_for_tokens(MAX_DEVELOPER_INSTRUCTIONS_TOKENS) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "`developer_instructions` exceeds the model-context limit of {MAX_DEVELOPER_INSTRUCTIONS_TOKENS} estimated tokens"
                ),
            ));
        }
        Ok(Self {
            instructions: instructions
                .filter(|instructions| !instructions.is_empty())
                .map(DeveloperInstructions::new),
        })
    }
}

impl WorldStateSection for DeveloperInstructionsState {
    const ID: &'static str = "developer_instructions";
    type Snapshot = DeveloperInstructionsSnapshot;

    fn snapshot(&self) -> Self::Snapshot {
        DeveloperInstructionsSnapshot {
            instructions: self
                .instructions
                .as_ref()
                .map(WorldStateHash::from_fragment),
        }
    }

    fn matches_current_legacy_fragment(&self, role: &str, _text: &str) -> bool {
        self.instructions.is_some() && role == "developer"
    }

    fn render_diff(
        &self,
        previous: PreviousSectionState<'_, Self::Snapshot>,
    ) -> Option<Box<dyn ContextualUserFragment>> {
        if matches!(previous, PreviousSectionState::Known(previous) if previous == &self.snapshot())
        {
            return None;
        }
        let previous_had_instructions = match previous {
            PreviousSectionState::Absent => false,
            PreviousSectionState::Unknown => true,
            PreviousSectionState::Known(previous) => previous.instructions.is_some(),
        };
        match (&self.instructions, previous_had_instructions) {
            (Some(instructions), true) => Some(Box::new(DeveloperInstructions::new(format!(
                "{REPLACEMENT_NOTICE}\n\n{}",
                instructions.body()
            )))),
            (Some(instructions), false) => Some(Box::new(instructions.clone())),
            (None, true) => Some(Box::new(DeveloperInstructions::new(REMOVAL_NOTICE))),
            (None, false) => None,
        }
    }
}

#[cfg(test)]
#[path = "developer_instructions_tests.rs"]
mod tests;
