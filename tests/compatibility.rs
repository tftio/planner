//! Compatibility fixtures for the extracted format-v1 validator.

use std::path::Path;

use tftio_planner::model::{Evaluator, FormatVersion, PlanMode, TaskId, TaskStatus};
use tftio_planner::{
    DiagnosticCode, DiagnosticSeverity, parse_markdown, validate_markdown, validate_markdown_path,
    validate_plan,
};

const EXAMPLE: &str = include_str!("../resources/examples/2026-06-15-replace-auth-middleware.md");
const VALID_HIDDEN: &str = include_str!("fixtures/legacy/2026-06-22-hidden-criteria-valid.md");
const EVALUATOR_MISMATCH: &str =
    include_str!("fixtures/legacy/2026-06-22-hidden-criteria-evaluator-mismatch.md");
const MISSING_WHY: &str =
    include_str!("fixtures/legacy/2026-06-22-hidden-criteria-missing-why-hidden.md");
const MISSING_COUNTERFACTUAL: &str =
    include_str!("fixtures/legacy/2026-06-22-hidden-criteria-missing-counterfactual.md");
const V2_EXAMPLE: &str = include_str!("../resources/examples/2026-08-04-cache-session-lookups.md");

#[test]
fn canonical_and_hidden_criteria_examples_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    assert!(validate_markdown(EXAMPLE)?.is_valid());
    assert!(validate_markdown(VALID_HIDDEN)?.is_valid());
    Ok(())
}

#[test]
fn legacy_negative_fixtures_have_stable_codes() -> Result<(), Box<dyn std::error::Error>> {
    let mismatch = validate_markdown(EVALUATOR_MISMATCH)?;
    assert!(mismatch.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::InvalidHiddenCriterion
            && diagnostic.severity == DiagnosticSeverity::Error
    }));

    for source in [MISSING_WHY, MISSING_COUNTERFACTUAL] {
        let error = validate_markdown(source)
            .err()
            .ok_or("missing hidden field unexpectedly parsed")?;
        assert_eq!(
            error.diagnostics.first().map(|diagnostic| diagnostic.code),
            Some(DiagnosticCode::InvalidYaml)
        );
    }
    Ok(())
}

#[test]
fn v2_project_diagnostics_are_stable_across_the_compatibility_validator()
-> Result<(), Box<dyn std::error::Error>> {
    let bad_slug = V2_EXAMPLE.replacen("  slug: gateway\n", "  slug: GATEWAY\n", 1);
    let error = parse_markdown(&bad_slug)
        .err()
        .ok_or("malformed project.slug unexpectedly parsed")?;
    assert_eq!(
        error.diagnostics.first().map(|diagnostic| diagnostic.code),
        Some(DiagnosticCode::InvalidProject)
    );

    let unnormalized_remote = V2_EXAMPLE.replacen(
        "  remote: github.com/example/gateway\n",
        "  remote: git@github.com:example/gateway.git\n",
        1,
    );
    let error = parse_markdown(&unnormalized_remote)
        .err()
        .ok_or("unnormalized project.remote unexpectedly parsed")?;
    assert_eq!(
        error.diagnostics.first().map(|diagnostic| diagnostic.code),
        Some(DiagnosticCode::InvalidProject)
    );

    let empty_remote = V2_EXAMPLE.replacen(
        "  remote: github.com/example/gateway\n",
        "  remote: \"\"\n",
        1,
    );
    let error = parse_markdown(&empty_remote)
        .err()
        .ok_or("empty project.remote unexpectedly parsed")?;
    assert_eq!(
        error.diagnostics.first().map(|diagnostic| diagnostic.code),
        Some(DiagnosticCode::InvalidProject)
    );
    Ok(())
}

#[test]
fn duplicated_detail_title_mismatch_is_warning_only() -> Result<(), Box<dyn std::error::Error>> {
    let source = EXAMPLE.replace(
        "## T001 — Add regression test for expired token handling",
        "## T001 — Different title",
    );
    let report = validate_markdown(&source)?;

    assert!(report.is_valid());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::DuplicatedStateMismatch
            && diagnostic.severity == DiagnosticSeverity::Warning
    }));
    Ok(())
}

#[test]
fn path_context_enforces_filename_and_warns_on_placement() -> Result<(), Box<dyn std::error::Error>>
{
    let report = validate_markdown_path(EXAMPLE, Path::new("elsewhere/not-a-plan.txt"))?;

    assert!(!report.is_valid());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::InvalidFilename
            && diagnostic.severity == DiagnosticSeverity::Error
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::InvalidFilename
            && diagnostic.severity == DiagnosticSeverity::Warning
    }));

    let valid_path = validate_markdown_path(
        EXAMPLE,
        Path::new("docs/plans/2026-06-15-replace-auth-middleware.md"),
    )?;
    assert!(valid_path.is_valid());
    Ok(())
}

#[test]
fn extra_task_detail_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = EXAMPLE.replace(
        "# Execution Protocol",
        "## T999 — Extra detail\n\n## Notes\n\nIgnored detail prose.\n\n# Execution Protocol",
    );
    let report = validate_markdown(&source)?;

    assert!(!report.is_valid());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::TaskDetailMismatch)
    );
    Ok(())
}

#[test]
fn missing_task_detail_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = EXAMPLE.replace(
        "## T001 — Add regression test for expired token handling",
        "## Notes",
    );
    let report = validate_markdown(&source)?;

    assert!(!report.is_valid());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::TaskDetailMismatch && diagnostic.message.contains("T001")
    }));
    Ok(())
}

#[test]
fn public_model_validation_reports_semantic_failures() -> Result<(), Box<dyn std::error::Error>> {
    let mut plan = parse_markdown(VALID_HIDDEN)?;
    plan.metadata.title.clear();
    plan.metadata.created_at = "bad".to_owned();
    plan.metadata.format_version = FormatVersion::V1 {
        mode: PlanMode::Multi,
    };
    plan.operator_guidance_log = None;
    plan.decision_log = None;
    plan.execution_protocol = None;
    plan.adr.context.clear();

    let task = plan.tasks.first_mut().ok_or("fixture has no task")?;
    task.title.clear();
    task.description.clear();
    task.invariants.clear();
    task.acceptance_checks.clear();
    task.work_items.clear();
    task.files = None;
    task.status = TaskStatus::Done;
    task.completion_evidence = None;
    task.depends_on = vec![
        task.id.clone(),
        TaskId::parse("T002")?,
        TaskId::parse("T999")?,
    ];
    let base = task
        .hidden_criteria
        .first()
        .cloned()
        .ok_or("fixture has no hidden criterion")?;
    let mut neither = base.clone();
    neither.claim.clear();
    neither.why_hidden.clear();
    neither.counterfactual.clear();
    neither.check = None;
    neither.ask = None;
    let mut automated_ask = base.clone();
    automated_ask.evaluator = Evaluator::Automated;
    automated_ask.check = None;
    automated_ask.ask = Some("Question?".to_owned());
    let mut human_check = base;
    human_check.evaluator = Evaluator::HumanJudgment;
    human_check.check = Some("true".to_owned());
    human_check.ask = None;
    task.hidden_criteria = vec![neither, automated_ask, human_check];
    let mut dependency = task.clone();
    dependency.id = TaskId::parse("T002")?;
    dependency.status = TaskStatus::NotStarted;
    dependency.depends_on.clear();
    dependency.hidden_criteria.clear();
    plan.tasks.push(dependency);

    let report = validate_plan(&plan);
    assert!(!report.is_valid());
    assert!(report.diagnostics.len() > 10);

    plan.tasks.clear();
    let empty = validate_plan(&plan);
    assert!(
        empty
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::EmptyTaskGraph)
    );
    Ok(())
}
