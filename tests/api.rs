//! Public API integration tests for `planner`.

use serde_json::Value;
use tftio_planner::inspect::operator_json;
use tftio_planner::model::{HiddenEvidenceRecord, HiddenVerdictJudgment, TaskId};
use tftio_planner::{Mutation, MutationRequest, WorkerPlan, apply_mutation, parse_markdown};

const VALID: &str = include_str!("fixtures/valid-single.md");

#[test]
fn validation_rejects_inconsistent_hidden_verdict_fields() -> Result<(), Box<dyn std::error::Error>>
{
    for (verdict, evidence_needed, rationale, expected) in [
        (
            Some(HiddenVerdictJudgment::Undetermined),
            None,
            Some("pending"),
            "verdict 'undetermined' requires a non-empty evidence_needed",
        ),
        (
            Some(HiddenVerdictJudgment::Undetermined),
            Some(" "),
            Some("pending"),
            "verdict 'undetermined' requires a non-empty evidence_needed",
        ),
        (
            Some(HiddenVerdictJudgment::Pass),
            Some("more evidence"),
            Some("met"),
            "evidence_needed is only permitted when verdict is 'undetermined'",
        ),
        (
            Some(HiddenVerdictJudgment::Fail),
            Some("more evidence"),
            Some("not met"),
            "evidence_needed is only permitted when verdict is 'undetermined'",
        ),
        (
            None,
            Some("more evidence"),
            None,
            "evidence_needed requires a verdict",
        ),
        (None, None, Some("met"), "rationale requires a verdict"),
    ] {
        let mut plan = parse_markdown(VALID)?;
        let criterion = plan
            .tasks
            .first_mut()
            .and_then(|task| task.hidden_criteria.first_mut())
            .ok_or("fixture has no hidden criterion")?;
        criterion.verdict = verdict;
        criterion.evidence_needed = evidence_needed.map(str::to_owned);
        criterion.rationale = rationale.map(str::to_owned);
        let report = tftio_planner::validate_plan(&plan);
        assert!(!report.is_valid());
        assert_eq!(report.diagnostics.len(), 1, "{report:?}");
        let diagnostic = report.diagnostics.first().ok_or("missing diagnostic")?;
        assert_eq!(
            diagnostic.code,
            tftio_planner::DiagnosticCode::InvalidHiddenCriterion
        );
        assert!(
            diagnostic.message.ends_with(expected),
            "{}",
            diagnostic.message
        );
    }
    Ok(())
}

#[test]
fn operator_model_converts_to_worker_model() -> Result<(), Box<dyn std::error::Error>> {
    let plan = parse_markdown(VALID)?;
    let worker = WorkerPlan::from(&plan);

    assert_eq!(worker.metadata.title, "Parser fixture");
    assert_eq!(worker.tasks.len(), 1);
    Ok(())
}

/// `inspect --format json` must carry a hidden criterion's recorded
/// verdict fields (`verdict`, `rationale`, `evidence_needed`,
/// `evidence`), not just its structural fields -- before this test the
/// JSON view silently omitted all four, so a stored verdict was
/// invisible in JSON even though the same plan's Markdown carried it.
#[test]
fn operator_json_carries_a_hidden_criterions_verdict_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/legacy/2026-06-22-hidden-criteria-valid.md"
    ))?;
    let plan = parse_markdown(&source)?;

    let outcome = apply_mutation(
        &plan,
        &MutationRequest {
            date: "2026-09-08".to_owned(),
            mutation: Mutation::RecordHiddenVerdict {
                task_id: TaskId::parse("T001")?,
                index: 0,
                verdict: HiddenVerdictJudgment::Fail,
                rationale: "the worker loosened an assertion".to_owned(),
                evidence_needed: None,
                evidence: Some(vec![HiddenEvidenceRecord {
                    summary: "diff shows an assertion removed".to_owned(),
                    provenance: "judge".to_owned(),
                }]),
            },
        },
    )?;

    let rendered = operator_json(outcome.plan())?;
    let value: Value = serde_json::from_str(&rendered)?;
    let criterion = first_hidden_criterion(&value)?;

    assert_eq!(
        Some("fail"),
        criterion.get("verdict").and_then(Value::as_str)
    );
    assert_eq!(
        Some("the worker loosened an assertion"),
        criterion.get("rationale").and_then(Value::as_str)
    );
    assert!(criterion.get("evidence_needed").is_some_and(Value::is_null));
    let evidence = criterion
        .get("evidence")
        .and_then(Value::as_array)
        .and_then(|records| records.first())
        .ok_or("expected one evidence record")?;
    assert_eq!(
        Some("diff shows an assertion removed"),
        evidence.get("summary").and_then(Value::as_str)
    );
    assert_eq!(
        Some("judge"),
        evidence.get("provenance").and_then(Value::as_str)
    );

    Ok(())
}

/// Walk `{"plan": {"tasks": [{"hidden_criteria": [...]}]}}` down to the
/// first task's first hidden criterion, without ever indexing a
/// [`Value`] directly (`clippy::indexing_slicing`, denied by this
/// crate's lints).
fn first_hidden_criterion(value: &Value) -> Result<&Value, Box<dyn std::error::Error>> {
    value
        .get("plan")
        .and_then(|plan| plan.get("tasks"))
        .and_then(Value::as_array)
        .and_then(|tasks| tasks.first())
        .and_then(|task| task.get("hidden_criteria"))
        .and_then(Value::as_array)
        .and_then(|criteria| criteria.first())
        .ok_or_else(|| "expected plan.tasks[0].hidden_criteria[0]".into())
}

/// The negative case: no verdict recorded at all renders `null`, not an
/// absent key or an empty string, so a JSON consumer can distinguish
/// "not yet judged" from every judged outcome.
#[test]
fn operator_json_renders_null_verdict_fields_before_any_verdict_is_recorded()
-> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/legacy/2026-06-22-hidden-criteria-valid.md"
    ))?;
    let plan = parse_markdown(&source)?;

    let rendered = operator_json(&plan)?;
    let value: Value = serde_json::from_str(&rendered)?;
    let criterion = first_hidden_criterion(&value)?;

    assert!(criterion.get("verdict").is_some_and(Value::is_null));
    assert!(criterion.get("rationale").is_some_and(Value::is_null));
    assert!(criterion.get("evidence_needed").is_some_and(Value::is_null));
    assert_eq!(
        Some(0),
        criterion
            .get("evidence")
            .and_then(Value::as_array)
            .map(Vec::len)
    );

    Ok(())
}
