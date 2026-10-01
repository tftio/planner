//! Loss-aware parser integration tests.

use proptest::prelude::*;
use tftio_planner::{DiagnosticCode, ParseError, WorkerPlan, parse_markdown, render_markdown};

const VALID: &str = include_str!("fixtures/valid-single.md");
const IMPLEMENTED_MULTI: &str = include_str!("fixtures/2026-08-01-extract-report-renderer.md");
const V2: &str = include_str!("fixtures/valid-v2.md");

#[test]
fn parses_valid_plan() -> Result<(), Box<dyn std::error::Error>> {
    let plan = parse_markdown(VALID)?;

    assert_eq!(plan.metadata.title, "Parser fixture");
    assert_eq!(plan.tasks.len(), 1);
    Ok(())
}

#[test]
fn v2_project_round_trips_through_the_render_and_reparse_used_by_every_consumer()
-> Result<(), Box<dyn std::error::Error>> {
    let plan = parse_markdown(V2)?;
    let project = plan
        .metadata
        .project
        .clone()
        .ok_or("v2 fixture must carry a project for this test")?;
    let rendered = render_markdown(&plan)?;
    let round_tripped = parse_markdown(&rendered)?;
    assert_eq!(round_tripped.metadata.project, Some(project));

    let bad_slug = V2.replacen("slug: planner\n", "slug: BAD SLUG\n", 1);
    assert_eq!(
        parse_markdown(&bad_slug)
            .err()
            .and_then(|error| error.diagnostics.first().map(|diagnostic| diagnostic.code)),
        Some(DiagnosticCode::InvalidProject)
    );
    Ok(())
}

#[test]
fn worker_type_omits_hidden_criteria_by_construction() -> Result<(), Box<dyn std::error::Error>> {
    let plan = parse_markdown(VALID)?;
    let worker = WorkerPlan::from(&plan);

    assert_eq!(worker.tasks.len(), 1);
    assert_eq!(
        plan.tasks.first().map(|task| task.hidden_criteria.len()),
        Some(1)
    );
    Ok(())
}

#[test]
fn malformed_yaml_reports_a_located_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
    let malformed = VALID.replacen("title: Parser fixture", "title: [", 1);
    let error = parse_markdown(&malformed)
        .err()
        .ok_or("malformed YAML unexpectedly parsed")?;
    let diagnostic = error
        .diagnostics
        .first()
        .ok_or("parse failure contained no diagnostic")?;

    assert_eq!(diagnostic.code, DiagnosticCode::InvalidYaml);
    assert!(diagnostic.location.is_some());
    Ok(())
}

#[test]
fn rejects_non_lf_input() -> Result<(), Box<dyn std::error::Error>> {
    let error = parse_markdown(&VALID.replace('\n', "\r\n"))
        .err()
        .ok_or("CRLF planning document unexpectedly parsed")?;
    let diagnostic = error
        .diagnostics
        .first()
        .ok_or("parse failure contained no diagnostic")?;

    assert_eq!(diagnostic.code, DiagnosticCode::NonCanonicalLineEnding);
    Ok(())
}

#[test]
fn rejects_an_unknown_hidden_verdict() -> Result<(), Box<dyn std::error::Error>> {
    let source = VALID.replace(
        "        evaluator: human_judgment",
        "        evaluator: human_judgment\n        verdict: inconclusive",
    );
    let error = parse_markdown(&source)
        .err()
        .ok_or("unknown verdict parsed")?;
    let diagnostic = error.diagnostics.first().ok_or("missing diagnostic")?;
    assert_eq!(diagnostic.code, DiagnosticCode::InvalidTaskGraphMarkers);
    assert_eq!(
        diagnostic.message,
        "invalid hidden criterion verdict: inconclusive"
    );
    assert!(diagnostic.location.is_some());
    Ok(())
}

#[test]
fn rejects_an_empty_required_heading() {
    let malformed = VALID.replace("## Context", "## ");

    assert!(parse_markdown(&malformed).is_err());
}

#[test]
fn empty_parse_error_has_a_stable_fallback_message() {
    let error = ParseError {
        diagnostics: Vec::new(),
    };

    assert_eq!(
        error.to_string(),
        "planning document parse failed without a diagnostic"
    );
}

#[test]
fn empty_optional_section_is_absent_from_the_model() -> Result<(), Box<dyn std::error::Error>> {
    let source = VALID.replace("# Task Graph", "## Operator Guidance Log\n\n# Task Graph");
    let plan = parse_markdown(&source)?;

    assert_eq!(plan.operator_guidance_log, None);
    Ok(())
}

#[test]
fn rejects_duplicate_task_ids() -> Result<(), Box<dyn std::error::Error>> {
    let duplicate = VALID.replace(
        "```\n<!-- TASK_GRAPH:END -->",
        "  - id: T001\n    title: Duplicate\n    status: not_started\n    depends_on: []\n    description: Duplicate task.\n    invariants: [Still valid.]\n    acceptance_checks: [Rejected.]\n```\n<!-- TASK_GRAPH:END -->",
    );
    let error = parse_markdown(&duplicate)
        .err()
        .ok_or("duplicate task id unexpectedly parsed")?;
    let diagnostic = error
        .diagnostics
        .first()
        .ok_or("parse failure contained no diagnostic")?;

    assert!(diagnostic.message.contains("duplicate task id"));
    Ok(())
}

#[test]
fn implemented_multi_plan_semantically_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let plan = parse_markdown(IMPLEMENTED_MULTI)?;
    let rendered = render_markdown(&plan)?;
    let reparsed = parse_markdown(&rendered)?;

    assert_eq!(reparsed, plan);
    Ok(())
}

proptest! {
    #[test]
    fn markdown_semantic_round_trip(note in "[^\\r\\n]{0,128}") {
        let mut plan = parse_markdown(VALID)?;
        plan.adr.context = format!("{} {note}", plan.adr.context).trim().to_owned();
        let rendered = render_markdown(&plan)?;
        let reparsed = parse_markdown(&rendered)?;

        prop_assert_eq!(reparsed, plan);
    }
}
