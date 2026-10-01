//! Worker-safe Markdown projection.

use serde_yaml_ng::{Mapping, Value};
use thiserror::Error;

use crate::markdown::ParseError;
use crate::validate::validate_markdown;

const TASK_GRAPH_BEGIN: &str = "<!-- TASK_GRAPH:BEGIN -->";
const TASK_GRAPH_END: &str = "<!-- TASK_GRAPH:END -->";

/// Worker-projection failure.
#[derive(Debug, Error)]
pub enum ProjectionError {
    /// The source does not parse as a planning document.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// The parsed document violates the v1 validation contract.
    #[error("planning document is invalid")]
    InvalidPlan,
    /// The task-graph marker or YAML-fence structure is malformed.
    #[error("{0}")]
    Structure(&'static str),
    /// The task-graph YAML could not be decoded or encoded.
    #[error(transparent)]
    Yaml(#[from] serde_yaml_ng::Error),
}

/// Remove all hidden criteria from task-graph YAML while preserving surrounding Markdown.
///
/// # Errors
///
/// Returns an error when the source is invalid or its task-graph region cannot be projected.
pub fn project_worker_markdown(source: &str) -> Result<String, ProjectionError> {
    if !validate_markdown(source)?.is_valid() {
        return Err(ProjectionError::InvalidPlan);
    }
    let begin = source
        .find(TASK_GRAPH_BEGIN)
        .ok_or(ProjectionError::Structure(
            "missing TASK_GRAPH:BEGIN marker",
        ))?;
    let end = source
        .find(TASK_GRAPH_END)
        .filter(|end| *end > begin)
        .ok_or(ProjectionError::Structure(
            "missing or misplaced TASK_GRAPH:END marker",
        ))?;
    let block = source
        .get(begin..end)
        .ok_or(ProjectionError::Structure("invalid task-graph marker span"))?;
    let fence_start = block
        .find("```yaml\n")
        .or_else(|| block.find("```\n"))
        .ok_or(ProjectionError::Structure(
            "task graph markers contain no YAML fence",
        ))?;
    let opening_end = block
        .get(fence_start..)
        .and_then(|tail| tail.find('\n'))
        .map(|offset| fence_start.saturating_add(offset).saturating_add(1))
        .ok_or(ProjectionError::Structure(
            "unterminated YAML fence opening",
        ))?;
    let closing_start = block
        .get(opening_end..)
        .and_then(|tail| tail.find("\n```"))
        .map(|offset| opening_end.saturating_add(offset))
        .ok_or(ProjectionError::Structure("unterminated YAML fence"))?;
    let yaml = block
        .get(opening_end..closing_start)
        .ok_or(ProjectionError::Structure("invalid YAML fence span"))?;
    let mut graph: Value = serde_yaml_ng::from_str(yaml)?;
    strip_hidden_criteria(&mut graph)?;
    let rendered = serde_yaml_ng::to_string(&graph)?;
    let mut projected_block = String::new();
    projected_block.push_str(block.get(..opening_end).unwrap_or_default());
    projected_block.push_str(rendered.trim_end());
    projected_block.push_str(block.get(closing_start..).unwrap_or_default());

    let mut output = String::new();
    output.push_str(source.get(..begin).unwrap_or_default());
    output.push_str(&projected_block);
    output.push_str(source.get(end..).unwrap_or_default());
    Ok(output)
}

fn strip_hidden_criteria(graph: &mut Value) -> Result<(), ProjectionError> {
    let mapping = graph.as_mapping_mut().ok_or(ProjectionError::Structure(
        "task graph YAML must be a mapping",
    ))?;
    let tasks = mapping
        .get_mut(Value::String("tasks".to_owned()))
        .and_then(Value::as_sequence_mut)
        .ok_or(ProjectionError::Structure(
            "task graph YAML must contain a tasks list",
        ))?;
    for task in tasks {
        if let Some(task_mapping) = task.as_mapping_mut() {
            remove_hidden(task_mapping);
        }
    }
    Ok(())
}

fn remove_hidden(task: &mut Mapping) {
    task.remove(Value::String("hidden_criteria".to_owned()));
}
