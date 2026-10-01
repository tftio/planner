//! Markdown mutation preparation and deterministic previews.

use std::fmt::Write as _;
use std::path::Path;

use thiserror::Error;

use crate::diagnostic::Diagnostic;
use crate::markdown::{ParseError, RenderError, parse_markdown, render_markdown};
use crate::state::{MutationRequest, StateError, apply_mutation};
use crate::validate::validate_markdown;

/// Failure while preparing a validated Markdown mutation.
#[derive(Debug, Error)]
pub enum MutationError {
    /// The source cannot be parsed.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// The source parses but violates the planning-document contract.
    #[error("source planning document is invalid")]
    InvalidSource {
        /// Stable validation diagnostics.
        diagnostics: Vec<Diagnostic>,
    },
    /// The lifecycle state machine rejects the requested operation.
    #[error(transparent)]
    State(#[from] StateError),
    /// The canonical Markdown adapter cannot render the changed model.
    #[error(transparent)]
    Render(#[from] RenderError),
    /// The rendered result violates the planning-document contract.
    #[error("mutation produced an invalid planning document")]
    InvalidResult {
        /// Stable validation diagnostics.
        diagnostics: Vec<Diagnostic>,
    },
}

/// Fully rendered, validated mutation ready for preview or compare-before-write.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PreparedMutation {
    original: String,
    replacement: String,
    summary: String,
}

impl PreparedMutation {
    /// Borrow the exact source bytes captured before mutation preparation.
    #[must_use]
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Borrow the canonical replacement bytes.
    #[must_use]
    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    /// Borrow the operation summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Render a unified whole-document diff whose added side is the exact replacement.
    #[must_use]
    pub fn unified_diff(&self, path: &Path) -> String {
        let label = path.display();
        let old_lines = self.original.lines().count();
        let new_lines = self.replacement.lines().count();
        let mut output = format!("--- {label}\n+++ {label}\n@@ -1,{old_lines} +1,{new_lines} @@\n");
        for line in self.original.lines() {
            let _ = writeln!(output, "-{line}");
        }
        for line in self.replacement.lines() {
            let _ = writeln!(output, "+{line}");
        }
        output
    }
}

/// Parse, validate, mutate, canonically render, and revalidate a Markdown plan.
///
/// This function performs no filesystem I/O. Its returned original bytes are the
/// compare-before-write identity used by [`crate::write::apply_prepared`].
///
/// # Errors
///
/// Returns a typed parse, validation, state-machine, or rendering failure. No partial
/// model or replacement text is returned on failure.
pub fn prepare_markdown_mutation(
    source: &str,
    request: &MutationRequest,
) -> Result<PreparedMutation, MutationError> {
    let source_report = validate_markdown(source)?;
    if !source_report.is_valid() {
        return Err(MutationError::InvalidSource {
            diagnostics: source_report.diagnostics,
        });
    }
    let plan = parse_markdown(source)?;
    let outcome = apply_mutation(&plan, request)?;
    let replacement = render_markdown(outcome.plan())?;
    validate_replacement(&replacement)?;
    Ok(PreparedMutation {
        original: source.to_owned(),
        replacement,
        summary: outcome.summary().to_owned(),
    })
}

fn validate_replacement(replacement: &str) -> Result<(), MutationError> {
    let result_report = validate_markdown(replacement)?;
    if !result_report.is_valid() {
        return Err(MutationError::InvalidResult {
            diagnostics: result_report.diagnostics,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_rendered_results_are_rejected() {
        let source = include_str!("../tests/fixtures/legacy/2026-06-22-hidden-criteria-valid.md")
            .replace("depends_on: []", "depends_on: [T001]");
        assert!(matches!(
            validate_replacement(&source),
            Err(MutationError::InvalidResult { .. })
        ));
    }
}
