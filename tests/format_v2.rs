//! Planning Document Format v2 behaviour: the rules v2 states, exercised
//! through the public API a consumer actually calls.
//!
//! v1's behaviour is asserted by every other test in this suite; nothing here
//! touches it, because v2 was added beside v1 rather than over it.

use tftio_lib::project::Slug;
use tftio_planner::inspect::{operator_json, operator_summary, worker_json, worker_summary};
use tftio_planner::model::{FormatVersion, ProjectIdentity, TaskId, WorkerPlan};
use tftio_planner::{
    Mutation, MutationRequest, TaskAction, parse_markdown, prepare_markdown_mutation,
    project_worker_markdown, render_markdown, validate_markdown,
};

const V2: &str = include_str!("fixtures/valid-v2.md");
/// The worked v2 plan an author is pointed at. The v1 example beside it stays
/// v1 (D011 of the format-v2 plan): a worked document exists for each version.
const V2_EXAMPLE: &str = include_str!("../resources/examples/2026-08-04-cache-session-lookups.md");
const V1: &str = include_str!("fixtures/valid-single.md");

fn error_message(source: &str) -> String {
    parse_markdown(source).err().map_or_else(
        || "parsed successfully".to_owned(),
        |error| error.to_string(),
    )
}

#[test]
fn a_v2_plan_parses_validates_and_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let plan = parse_markdown(V2)?;
    assert_eq!(plan.metadata.format_version, FormatVersion::V2);
    assert_eq!(plan.metadata.format_version.number(), 2);
    assert!(validate_markdown(V2)?.is_valid());

    let rendered = render_markdown(&plan)?;
    assert_eq!(parse_markdown(&rendered)?, plan);
    // A v2 plan renders the keys v2 requires, spelling an absent value `null`
    // rather than omitting the key, and never renders the fields v2 removed.
    assert!(rendered.contains("owner: null"));
    assert!(rendered.contains("completion_evidence: null"));
    assert!(!rendered.contains("\nmode:"));
    assert!(!rendered.contains("blocks:"));
    assert!(!rendered.contains("files:"));
    assert!(validate_markdown(&rendered)?.is_valid());

    project_worker_markdown(V2)?;
    Ok(())
}

#[test]
fn the_worked_v2_example_validates_and_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let plan = parse_markdown(V2_EXAMPLE)?;
    assert_eq!(plan.metadata.format_version, FormatVersion::V2);
    assert!(validate_markdown(V2_EXAMPLE)?.is_valid());
    assert_eq!(parse_markdown(&render_markdown(&plan)?)?, plan);
    Ok(())
}

#[test]
fn v2_rejects_the_fields_it_removed_and_names_the_version() {
    let with_mode = V2.replace("status: approved\n", "status: approved\nmode: multi\n");
    let message = error_message(&with_mode);
    assert!(
        message.contains("Planning Document Format v2 removed"),
        "{message}"
    );
    assert!(message.contains("'mode'"), "{message}");

    for blocks in ["blocks: [T002]", "blocks: []"] {
        let with_blocks = V2.replace(
            "    depends_on: []\n",
            &format!("    depends_on: []\n    {blocks}\n"),
        );
        let message = error_message(&with_blocks);
        assert!(
            message.contains("Planning Document Format v2 removed"),
            "{message}"
        );
        assert!(message.contains("task T001"), "{message}");
    }

    // `files` is rejected however it is spelled, including the empty mapping a
    // v1 author would have written to say "no files".
    for files in [
        "    files:\n      likely_read: []\n      likely_modify: []\n",
        "    files:\n      likely_read:\n        - src/markdown.rs\n",
        "    files: {}\n",
    ] {
        let with_files = V2.replacen(
            "    depends_on: []\n",
            &format!("    depends_on: []\n{files}"),
            1,
        );
        let message = error_message(&with_files);
        assert!(
            message.contains("Planning Document Format v2 removed"),
            "{message}"
        );
        assert!(message.contains("task T001"), "{message}");
        assert!(message.contains("'files'"), "{message}");
    }
}

#[test]
fn v2_requires_every_field_a_cold_session_needs_to_resume() {
    for (removed, field) in [
        ("    owner: null\n", "owner"),
        ("    completion_evidence: null\n", "completion_evidence"),
    ] {
        let message = error_message(&V2.replacen(removed, "", 1));
        assert!(
            message.contains(&format!("task T001 missing required field: {field}")),
            "{message}"
        );
    }
}

#[test]
fn v2_requires_the_handoff_record_of_every_plan() -> Result<(), Box<dyn std::error::Error>> {
    // v1 gates these on `mode: multi`; v2 requires them unconditionally, so
    // removing one from a plan that declares no mode is an error.
    for heading in [
        "## Operator Guidance Log",
        "## Decision Log",
        "# Execution Protocol",
    ] {
        let without = V2.replacen(heading, "## Unrelated Aside", 1);
        assert!(
            !validate_markdown(&without)?.is_valid(),
            "{heading} was not required"
        );
    }
    let empty_work_items = V2.replacen(
        "    work_items:\n      - Read the frontmatter under the v2 rule set.\n",
        "    work_items: []\n",
        1,
    );
    assert!(!validate_markdown(&empty_work_items)?.is_valid());
    Ok(())
}

#[test]
fn v2_inspection_reports_the_version_and_derives_blocks() -> Result<(), Box<dyn std::error::Error>>
{
    let plan = parse_markdown(V2)?;
    let worker = WorkerPlan::from(&plan);

    // `blocks` is never read from a v2 document, so the model carries none...
    assert!(plan.tasks.iter().all(|task| task.blocks.is_empty()));
    // ...and every view that presents one computes it from `depends_on`.
    let derived = plan.derived_blocks();
    assert_eq!(
        derived.get(&TaskId::parse("T001")?),
        Some(&vec![TaskId::parse("T002")?])
    );
    assert_eq!(derived.get(&TaskId::parse("T002")?), Some(&Vec::new()));
    assert_eq!(derived, worker.derived_blocks());

    for rendered in [operator_json(&plan)?, worker_json(&worker)?] {
        assert!(
            rendered.contains("\"plan_format_version\": 2"),
            "{rendered}"
        );
        assert!(!rendered.contains("\"mode\""), "{rendered}");
        assert!(
            rendered.contains("\"blocks\": [\n          \"T002\"\n        ]"),
            "{rendered}"
        );
    }
    for summary in [operator_summary(&plan), worker_summary(&worker)] {
        assert!(summary.contains("plan_format_version: 2"), "{summary}");
        assert!(!summary.contains("mode:"), "{summary}");
    }
    Ok(())
}

#[test]
fn a_v2_plans_project_round_trips_through_task_start() -> Result<(), Box<dyn std::error::Error>> {
    let plan = parse_markdown(V2)?;
    assert_eq!(
        plan.metadata.project,
        Some(ProjectIdentity {
            slug: Slug::new("planner")?,
            remote: Some(tftio_lib::project::normalize_remote(
                "github.com/tftio/planner"
            )?),
        })
    );

    let ready = prepare_markdown_mutation(
        V2,
        &MutationRequest {
            date: "2026-09-23".to_owned(),
            mutation: Mutation::Task {
                task_id: TaskId::parse("T001")?,
                action: TaskAction::Ready,
            },
        },
    )?;
    let started = prepare_markdown_mutation(
        ready.replacement(),
        &MutationRequest {
            date: "2026-09-23".to_owned(),
            mutation: Mutation::Task {
                task_id: TaskId::parse("T001")?,
                action: TaskAction::Start,
            },
        },
    )?;

    let after = parse_markdown(started.replacement())?;
    assert_eq!(after.metadata.project, plan.metadata.project);
    assert!(started.replacement().contains("project:\n  slug: planner"));
    Ok(())
}

#[test]
fn v2_project_grammar_is_validated_against_the_shared_rules()
-> Result<(), Box<dyn std::error::Error>> {
    let with_bad_slug = V2.replacen(
        "project:\n  slug: planner\n",
        "project:\n  slug: BAD_SLUG\n",
        1,
    );
    let message = error_message(&with_bad_slug);
    assert!(
        message.contains("[V012]") || message.contains("project.slug is invalid"),
        "{message}"
    );

    let with_unnormalized_remote = V2.replacen(
        "  remote: github.com/tftio/planner\n",
        "  remote: git@github.com:tftio/planner.git\n",
        1,
    );
    let message = error_message(&with_unnormalized_remote);
    assert!(message.contains("project.remote"), "{message}");

    // A valid slug with no remote passes.
    let no_remote = V2.replacen("  remote: github.com/tftio/planner\n", "", 1);
    assert!(validate_markdown(&no_remote)?.is_valid());
    let plan = parse_markdown(&no_remote)?;
    assert_eq!(
        plan.metadata.project,
        Some(ProjectIdentity {
            slug: Slug::new("planner")?,
            remote: None,
        })
    );
    let json = operator_json(&plan)?;
    assert!(json.contains("\"slug\": \"planner\""), "{json}");
    assert!(json.contains("\"remote\": null"), "{json}");
    Ok(())
}

#[test]
fn v1_keeps_its_mode_and_no_other_version_is_recognised() -> Result<(), Box<dyn std::error::Error>>
{
    let plan = parse_markdown(V1)?;
    assert!(matches!(
        plan.metadata.format_version,
        FormatVersion::V1 { .. }
    ));
    assert!(operator_summary(&plan).contains("mode: single"));
    assert!(operator_json(&plan)?.contains("\"mode\": \"single\""));

    assert!(
        error_message(&V1.replacen("mode: single\n", "", 1))
            .contains("frontmatter missing required key: mode")
    );
    assert!(
        error_message(&V1.replacen("plan_format_version: 1", "plan_format_version: 3", 1))
            .contains("plan_format_version must be 1 or 2")
    );
    Ok(())
}
