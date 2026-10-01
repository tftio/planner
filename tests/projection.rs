//! Operator inspection and worker-concealment contract tests.

use tftio_planner::model::TaskFiles;
use tftio_planner::{
    ProjectionError, WorkerPlan, parse_markdown, project_worker_markdown, validate_markdown,
};

const VALID_HIDDEN: &str = include_str!("fixtures/legacy/2026-06-22-hidden-criteria-valid.md");
const HIDDEN_CANARY: &str = "HIDDEN-CANARY-8d554251";

#[test]
fn worker_markdown_removes_hidden_state_and_preserves_surroundings()
-> Result<(), Box<dyn std::error::Error>> {
    let source = VALID_HIDDEN.replace(
        "The worker did not weaken any existing check to satisfy the task.",
        HIDDEN_CANARY,
    );
    assert!(source.contains(HIDDEN_CANARY));
    let projected = project_worker_markdown(&source)?;

    assert!(!projected.contains(HIDDEN_CANARY));
    assert!(!projected.contains("    hidden_criteria:"));
    let (source_prefix, _) = source
        .split_once("<!-- TASK_GRAPH:BEGIN -->")
        .ok_or("source has no task graph marker")?;
    let (projected_prefix, _) = projected
        .split_once("<!-- TASK_GRAPH:BEGIN -->")
        .ok_or("projection has no task graph marker")?;
    let (_, source_suffix) = source
        .split_once("<!-- TASK_GRAPH:END -->")
        .ok_or("source has no task graph end marker")?;
    let (_, projected_suffix) = projected
        .split_once("<!-- TASK_GRAPH:END -->")
        .ok_or("projection has no task graph end marker")?;
    assert_eq!(projected_prefix, source_prefix);
    assert_eq!(projected_suffix, source_suffix);
    assert!(validate_markdown(&projected)?.is_valid());
    assert_eq!(project_worker_markdown(&source)?, projected);

    let untyped_fence = VALID_HIDDEN.replacen("```yaml\n", "```\n", 1);
    assert!(project_worker_markdown(&untyped_fence)?.contains("```\n"));
    Ok(())
}

#[test]
fn worker_json_has_no_hidden_vocabulary() -> Result<(), Box<dyn std::error::Error>> {
    let mut operator = parse_markdown(VALID_HIDDEN)?;
    let task = operator.tasks.first_mut().ok_or("fixture has no task")?;
    task.files = Some(TaskFiles {
        likely_read: vec!["src/lib.rs".to_owned()],
        likely_modify: vec!["src/main.rs".to_owned()],
    });
    let criterion = task
        .hidden_criteria
        .first_mut()
        .ok_or("fixture has no hidden criterion")?;
    criterion.claim = HIDDEN_CANARY.to_owned();
    criterion.why_hidden = HIDDEN_CANARY.to_owned();
    criterion.counterfactual = HIDDEN_CANARY.to_owned();

    let operator_json = tftio_planner::inspect::operator_json(&operator)?;
    let worker = WorkerPlan::from(&operator);
    let worker_json = tftio_planner::inspect::worker_json(&worker)?;
    let worker_value: serde_json::Value = serde_json::from_str(&worker_json)?;

    assert!(operator_json.contains(HIDDEN_CANARY));
    assert_eq!(
        worker_value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        worker_value.get("role").and_then(|v| v.as_str()),
        Some("worker")
    );
    assert!(!worker_json.contains(HIDDEN_CANARY));
    assert_no_concealed_keys(&worker_value);

    let snapshot = include_str!("fixtures/inspect-worker-v1.json");
    let fixture_worker = WorkerPlan::from(&parse_markdown(VALID_HIDDEN)?);
    assert_eq!(
        format!(
            "{}\n",
            tftio_planner::inspect::worker_json(&fixture_worker)?
        ),
        snapshot
    );
    Ok(())
}

#[test]
fn worker_projection_rejects_semantically_invalid_plans() {
    let source = VALID_HIDDEN.replace("depends_on: []", "depends_on: [T001]");
    assert!(matches!(
        project_worker_markdown(&source),
        Err(ProjectionError::InvalidPlan)
    ));
}

#[test]
fn human_summaries_and_embedded_resources_are_available() -> Result<(), Box<dyn std::error::Error>>
{
    let operator = parse_markdown(VALID_HIDDEN)?;
    let worker = WorkerPlan::from(&operator);

    assert!(tftio_planner::inspect::operator_summary(&operator).contains("hidden criteria: 1"));
    assert!(!tftio_planner::inspect::worker_summary(&worker).contains("\nhidden criteria:"));
    assert!(tftio_planner::resources::PLAN_SPEC.contains("Planning Document Format v1"));
    assert!(tftio_planner::resources::PLAN_SPEC.contains("Planning Document Format v2"));
    assert!(tftio_planner::resources::PLAN_TEMPLATE.starts_with("---\n"));
    Ok(())
}

fn assert_no_concealed_keys(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for key in object.keys() {
                assert!(
                    !["hidden_criteria", "why_hidden", "counterfactual"].contains(&key.as_str())
                );
            }
            for child in object.values() {
                assert_no_concealed_keys(child);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                assert_no_concealed_keys(child);
            }
        }
        _ => {}
    }
}
