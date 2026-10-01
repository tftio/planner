//! Lifecycle mutation, synchronization, preview, and append-only contract tests.

use std::path::Path;

use tftio_planner::model::{
    HiddenEvidenceRecord, HiddenVerdictJudgment, PlanStatus, TaskGraphStatus, TaskId, TaskStatus,
    WorkerPlan,
};
use tftio_planner::{
    Mutation, MutationError, MutationRequest, PlanAction, StateError, TaskAction, apply_mutation,
    parse_markdown, prepare_markdown_mutation, project_worker_markdown, validate_markdown,
};

const VALID_SINGLE: &str = include_str!("fixtures/legacy/2026-06-22-hidden-criteria-valid.md");
const VALID_MULTI: &str =
    include_str!("../resources/examples/2026-06-15-replace-auth-middleware.md");

fn task_id(value: &str) -> Result<TaskId, Box<dyn std::error::Error>> {
    Ok(TaskId::parse(value)?)
}

fn request(mutation: Mutation) -> MutationRequest {
    MutationRequest {
        date: "2026-08-28".to_owned(),
        mutation,
    }
}

#[test]
fn evidence_mutations_reject_unknown_tasks() -> Result<(), Box<dyn std::error::Error>> {
    let plan = parse_markdown(VALID_SINGLE)?;
    let unknown = task_id("T999")?;
    for mutation in [
        Mutation::AppendCompletionEvidence {
            task_id: unknown.clone(),
            text: "verified".to_owned(),
        },
        Mutation::RecordHiddenVerdict {
            task_id: unknown.clone(),
            index: 0,
            verdict: HiddenVerdictJudgment::Pass,
            rationale: "verified".to_owned(),
            evidence_needed: None,
            evidence: None,
        },
        Mutation::AppendHiddenEvidence {
            task_id: unknown.clone(),
            index: 0,
            evidence: vec![HiddenEvidenceRecord {
                summary: "verified".to_owned(),
                provenance: "judge".to_owned(),
            }],
        },
    ] {
        assert_eq!(
            apply_mutation(&plan, &request(mutation)),
            Err(StateError::UnknownTask(unknown.clone()))
        );
    }
    Ok(())
}

#[test]
fn task_transition_table_covers_every_status_and_action() -> Result<(), Box<dyn std::error::Error>>
{
    let actions = [
        TaskAction::Ready,
        TaskAction::Start,
        TaskAction::Block("waiting".to_owned()),
        TaskAction::Complete("verified".to_owned()),
        TaskAction::Reopen("regression".to_owned()),
        TaskAction::Abandon("obsolete".to_owned()),
    ];
    for status in [
        TaskStatus::NotStarted,
        TaskStatus::Ready,
        TaskStatus::InProgress,
        TaskStatus::Blocked,
        TaskStatus::Done,
        TaskStatus::Abandoned,
    ] {
        for action in &actions {
            let mut plan = parse_markdown(VALID_SINGLE)?;
            plan.metadata.status = PlanStatus::Approved;
            let task = plan.tasks.first_mut().ok_or("fixture has no task")?;
            task.status = status;
            if status == TaskStatus::Done {
                task.completion_evidence = Some("prior evidence".to_owned());
            }
            let result = apply_mutation(
                &plan,
                &request(Mutation::Task {
                    task_id: task_id("T001")?,
                    action: action.clone(),
                }),
            );
            match expected_status(status, action) {
                Some(expected) => {
                    let outcome = result?;
                    assert_eq!(
                        outcome
                            .plan()
                            .tasks
                            .first()
                            .ok_or("outcome has no task")?
                            .status,
                        expected
                    );
                }
                None => assert!(matches!(result, Err(StateError::InvalidTransition { .. }))),
            }
        }
    }
    Ok(())
}

const fn expected_status(status: TaskStatus, action: &TaskAction) -> Option<TaskStatus> {
    match (status, action) {
        (TaskStatus::NotStarted, TaskAction::Ready) => Some(TaskStatus::Ready),
        (TaskStatus::Ready, TaskAction::Start) => Some(TaskStatus::InProgress),
        (TaskStatus::Ready | TaskStatus::InProgress, TaskAction::Block(_)) => {
            Some(TaskStatus::Blocked)
        }
        (TaskStatus::InProgress, TaskAction::Complete(_)) => Some(TaskStatus::Done),
        (TaskStatus::Blocked | TaskStatus::Done | TaskStatus::Abandoned, TaskAction::Reopen(_)) => {
            Some(TaskStatus::Ready)
        }
        (
            TaskStatus::NotStarted
            | TaskStatus::Ready
            | TaskStatus::InProgress
            | TaskStatus::Blocked,
            TaskAction::Abandon(_),
        ) => Some(TaskStatus::Abandoned),
        (
            TaskStatus::NotStarted
            | TaskStatus::Ready
            | TaskStatus::InProgress
            | TaskStatus::Blocked
            | TaskStatus::Done
            | TaskStatus::Abandoned,
            TaskAction::Ready
            | TaskAction::Start
            | TaskAction::Block(_)
            | TaskAction::Complete(_)
            | TaskAction::Reopen(_)
            | TaskAction::Abandon(_),
        ) => None,
    }
}

#[test]
fn dependencies_required_text_and_terminal_plans_are_enforced()
-> Result<(), Box<dyn std::error::Error>> {
    let plan = parse_markdown(VALID_MULTI)?;
    assert!(matches!(
        apply_mutation(
            &plan,
            &request(Mutation::Task {
                task_id: task_id("T003")?,
                action: TaskAction::Ready,
            })
        ),
        Err(StateError::DependenciesNotDone(_))
    ));
    assert!(matches!(
        apply_mutation(
            &plan,
            &request(Mutation::Task {
                task_id: task_id("T999")?,
                action: TaskAction::Ready,
            })
        ),
        Err(StateError::UnknownTask(_))
    ));

    for mutation in [
        Mutation::Task {
            task_id: task_id("T002")?,
            action: TaskAction::Block(" ".to_owned()),
        },
        Mutation::AddGuidance(" ".to_owned()),
        Mutation::AddDecision(String::new()),
    ] {
        assert!(matches!(
            apply_mutation(&plan, &request(mutation)),
            Err(StateError::EmptyText(_))
        ));
    }

    let mut terminal = plan.clone();
    terminal.metadata.status = PlanStatus::Implemented;
    assert!(matches!(
        apply_mutation(
            &terminal,
            &request(Mutation::AddDecision("late".to_owned()))
        ),
        Err(StateError::TerminalPlan("implemented"))
    ));
    terminal.metadata.status = PlanStatus::Abandoned;
    assert!(matches!(
        apply_mutation(
            &terminal,
            &request(Mutation::AddGuidance("late".to_owned()))
        ),
        Err(StateError::TerminalPlan("abandoned"))
    ));
    assert!(matches!(
        apply_mutation(
            &plan,
            &MutationRequest {
                date: "28-08-2026".to_owned(),
                mutation: Mutation::AddDecision("entry".to_owned()),
            }
        ),
        Err(StateError::InvalidDate)
    ));
    Ok(())
}

#[test]
fn plan_transition_table_and_aggregate_graph_status_are_enforced()
-> Result<(), Box<dyn std::error::Error>> {
    let mut draft = parse_markdown(VALID_SINGLE)?;
    draft.metadata.status = PlanStatus::Draft;
    let approved = apply_mutation(&draft, &request(Mutation::Plan(PlanAction::Approve)))?;
    assert_eq!(approved.plan().metadata.status, PlanStatus::Approved);

    assert!(matches!(
        apply_mutation(
            approved.plan(),
            &request(Mutation::Plan(PlanAction::Implement))
        ),
        Err(StateError::NonterminalTasks(_))
    ));
    let mut complete = approved.plan().clone();
    let task = complete.tasks.first_mut().ok_or("fixture has no task")?;
    task.status = TaskStatus::Done;
    task.completion_evidence = Some("verified".to_owned());
    let implemented = apply_mutation(&complete, &request(Mutation::Plan(PlanAction::Implement)))?;
    assert_eq!(implemented.plan().metadata.status, PlanStatus::Implemented);
    assert_eq!(
        implemented.plan().metadata.execution.task_graph_status,
        TaskGraphStatus::Complete
    );

    let abandoned = apply_mutation(
        &draft,
        &request(Mutation::Plan(PlanAction::Abandon("superseded".to_owned()))),
    )?;
    assert_eq!(abandoned.plan().metadata.status, PlanStatus::Abandoned);
    assert!(
        abandoned
            .plan()
            .decision_log
            .as_deref()
            .is_some_and(|log| log.contains("Plan abandoned: superseded"))
    );

    for status in [
        PlanStatus::Draft,
        PlanStatus::Approved,
        PlanStatus::Implemented,
        PlanStatus::Abandoned,
    ] {
        let mut plan = draft.clone();
        plan.metadata.status = status;
        for action in [
            PlanAction::Approve,
            PlanAction::Implement,
            PlanAction::Abandon("reason".to_owned()),
        ] {
            let allowed = matches!(
                (status, &action),
                (
                    PlanStatus::Draft,
                    PlanAction::Approve | PlanAction::Abandon(_)
                ) | (PlanStatus::Approved, PlanAction::Abandon(_))
            );
            let result = apply_mutation(&plan, &request(Mutation::Plan(action)));
            if allowed {
                assert!(result.is_ok());
            } else {
                assert!(matches!(
                    result,
                    Err(StateError::InvalidTransition { .. } | StateError::NonterminalTasks(_))
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn completion_and_logs_append_without_rewriting_prior_content()
-> Result<(), Box<dyn std::error::Error>> {
    let mut plan = parse_markdown(VALID_SINGLE)?;
    plan.metadata.status = PlanStatus::Approved;
    let task = plan.tasks.first_mut().ok_or("fixture has no task")?;
    task.status = TaskStatus::InProgress;
    task.completion_evidence = Some("prior evidence".to_owned());
    let completed = apply_mutation(
        &plan,
        &request(Mutation::Task {
            task_id: task_id("T001")?,
            action: TaskAction::Complete("new evidence".to_owned()),
        }),
    )?;
    let evidence = completed
        .plan()
        .tasks
        .first()
        .and_then(|task| task.completion_evidence.as_deref())
        .ok_or("completion evidence is absent")?;
    assert!(evidence.starts_with("prior evidence\n\n"));
    assert!(evidence.ends_with("new evidence"));

    let guided = apply_mutation(
        completed.plan(),
        &request(Mutation::AddGuidance("new guidance".to_owned())),
    )?;
    assert!(
        guided
            .plan()
            .operator_guidance_log
            .as_deref()
            .is_some_and(|log| log.contains("new guidance"))
    );
    let decided = apply_mutation(
        guided.plan(),
        &request(Mutation::AddDecision("new decision".to_owned())),
    )?;
    assert!(
        decided
            .plan()
            .decision_log
            .as_deref()
            .is_some_and(|log| log.contains("new decision"))
    );

    let mut blank_log = plan;
    blank_log.operator_guidance_log = Some(" ".to_owned());
    let appended = apply_mutation(
        &blank_log,
        &request(Mutation::AddGuidance("first entry".to_owned())),
    )?;
    assert!(
        appended
            .plan()
            .operator_guidance_log
            .as_deref()
            .is_some_and(|log| log.starts_with(" ### 2026-08-28") && log.ends_with("first entry"))
    );
    Ok(())
}

#[test]
fn markdown_mutation_synchronizes_graph_details_and_preview()
-> Result<(), Box<dyn std::error::Error>> {
    let prepared = prepare_markdown_mutation(
        VALID_SINGLE,
        &request(Mutation::Task {
            task_id: task_id("T001")?,
            action: TaskAction::Ready,
        }),
    )?;
    assert!(prepared.replacement().contains("status: ready"));
    assert!(prepared.replacement().contains("Status: `ready`"));
    assert!(validate_markdown(prepared.replacement())?.is_valid());
    assert_eq!(
        parse_markdown(prepared.replacement())?
            .tasks
            .first()
            .ok_or("replacement has no task")?
            .status,
        TaskStatus::Ready
    );
    let diff = prepared.unified_diff(Path::new("docs/plans/example.md"));
    assert!(diff.starts_with("--- docs/plans/example.md\n+++ docs/plans/example.md\n@@ "));
    assert!(diff.contains("+Status: `ready`"));

    let invalid = VALID_SINGLE.replace("depends_on: []", "depends_on: [T001]");
    assert!(matches!(
        prepare_markdown_mutation(
            &invalid,
            &request(Mutation::Task {
                task_id: task_id("T001")?,
                action: TaskAction::Ready,
            })
        ),
        Err(MutationError::InvalidSource { .. })
    ));
    Ok(())
}

#[test]
fn hidden_verdict_mutation_round_trips_and_stays_off_the_worker_projection()
-> Result<(), Box<dyn std::error::Error>> {
    let prepared = prepare_markdown_mutation(
        VALID_SINGLE,
        &request(Mutation::RecordHiddenVerdict {
            task_id: task_id("T001")?,
            index: 0,
            verdict: HiddenVerdictJudgment::Undetermined,
            rationale: "sentinel-rationale-xyz".to_owned(),
            evidence_needed: Some("a manual repro run".to_owned()),
            evidence: Some(vec![HiddenEvidenceRecord {
                summary: "sentinel-evidence-xyz".to_owned(),
                provenance: "judge".to_owned(),
            }]),
        }),
    )?;
    assert!(validate_markdown(prepared.replacement())?.is_valid());

    let target_id = task_id("T001")?;
    let reparsed = parse_markdown(prepared.replacement())?;
    let task = reparsed
        .tasks
        .iter()
        .find(|task| task.id == target_id)
        .ok_or("replacement has no T001")?;
    let criterion = task
        .hidden_criteria
        .first()
        .ok_or("replacement lost its hidden criterion")?;
    assert_eq!(Some(HiddenVerdictJudgment::Undetermined), criterion.verdict);
    assert_eq!(
        Some("sentinel-rationale-xyz"),
        criterion.rationale.as_deref()
    );
    assert_eq!(
        Some("a manual repro run"),
        criterion.evidence_needed.as_deref()
    );
    assert_eq!(1, criterion.evidence.len());
    let evidence_entry = criterion
        .evidence
        .first()
        .ok_or("verdict lost its evidence")?;
    assert_eq!("sentinel-evidence-xyz", evidence_entry.summary);
    assert_eq!("judge", evidence_entry.provenance);

    // Re-render/re-parse a second time: the verdict fields survive a full
    // Markdown round trip, not just the in-memory mutation.
    let rerendered = tftio_planner::render_markdown(&reparsed)?;
    let reparsed_again = parse_markdown(&rerendered)?;
    assert_eq!(reparsed, reparsed_again);

    // The sentinel never reaches the worker-safe projection, by any route.
    let worker = WorkerPlan::from(&reparsed);
    let worker_debug = format!("{worker:?}");
    assert!(!worker_debug.contains("sentinel-rationale-xyz"));
    assert!(!worker_debug.contains("sentinel-evidence-xyz"));
    let worker_markdown = project_worker_markdown(prepared.replacement())?;
    assert!(!worker_markdown.contains("sentinel-rationale-xyz"));
    assert!(!worker_markdown.contains("sentinel-evidence-xyz"));
    Ok(())
}

#[test]
fn hidden_verdict_requires_evidence_needed_iff_undetermined()
-> Result<(), Box<dyn std::error::Error>> {
    let missing = apply_mutation(
        &parse_markdown(VALID_SINGLE)?,
        &request(Mutation::RecordHiddenVerdict {
            task_id: task_id("T001")?,
            index: 0,
            verdict: HiddenVerdictJudgment::Undetermined,
            rationale: "needs more evidence".to_owned(),
            evidence_needed: None,
            evidence: Some(vec![]),
        }),
    );
    assert!(matches!(missing, Err(StateError::InvalidHiddenVerdict(_))));

    let spurious = apply_mutation(
        &parse_markdown(VALID_SINGLE)?,
        &request(Mutation::RecordHiddenVerdict {
            task_id: task_id("T001")?,
            index: 0,
            verdict: HiddenVerdictJudgment::Pass,
            rationale: "clearly met".to_owned(),
            evidence_needed: Some("should not be here".to_owned()),
            evidence: Some(vec![]),
        }),
    );
    assert!(matches!(spurious, Err(StateError::InvalidHiddenVerdict(_))));

    let out_of_range = apply_mutation(
        &parse_markdown(VALID_SINGLE)?,
        &request(Mutation::RecordHiddenVerdict {
            task_id: task_id("T001")?,
            index: 99,
            verdict: HiddenVerdictJudgment::Pass,
            rationale: "clearly met".to_owned(),
            evidence_needed: None,
            evidence: Some(vec![]),
        }),
    );
    assert!(matches!(
        out_of_range,
        Err(StateError::HiddenCriterionOutOfRange { .. })
    ));
    Ok(())
}

#[test]
fn append_completion_evidence_appends_without_a_status_transition()
-> Result<(), Box<dyn std::error::Error>> {
    let target_id = task_id("T001")?;
    let prepared = prepare_markdown_mutation(
        VALID_SINGLE,
        &request(Mutation::AppendCompletionEvidence {
            task_id: target_id.clone(),
            text: "automated check `cargo test` passed".to_owned(),
        }),
    )?;
    assert!(validate_markdown(prepared.replacement())?.is_valid());
    let reparsed = parse_markdown(prepared.replacement())?;
    let task = reparsed
        .tasks
        .iter()
        .find(|task| task.id == target_id)
        .ok_or("replacement has no T001")?;
    assert_eq!(TaskStatus::NotStarted, task.status);
    assert!(
        task.completion_evidence
            .as_deref()
            .is_some_and(|text| text.contains("automated check `cargo test` passed"))
    );
    Ok(())
}

#[test]
fn append_hidden_evidence_extends_without_touching_the_verdict()
-> Result<(), Box<dyn std::error::Error>> {
    // Record a verdict with one evidence record first, exactly as
    // a supervising ledger does when a judge routes a hidden-criterion
    // verdict.
    let with_verdict = prepare_markdown_mutation(
        VALID_SINGLE,
        &request(Mutation::RecordHiddenVerdict {
            task_id: task_id("T001")?,
            index: 0,
            verdict: HiddenVerdictJudgment::Pass,
            rationale: "clearly met".to_owned(),
            evidence_needed: None,
            evidence: Some(vec![HiddenEvidenceRecord {
                summary: "first-evidence".to_owned(),
                provenance: "tool_authored".to_owned(),
            }]),
        }),
    )?;

    // Then append a second evidence record — the run-level rationale or a
    // disagreement record — without re-supplying the verdict.
    let appended = prepare_markdown_mutation(
        with_verdict.replacement(),
        &request(Mutation::AppendHiddenEvidence {
            task_id: task_id("T001")?,
            index: 0,
            evidence: vec![HiddenEvidenceRecord {
                summary: "sentinel-appended-evidence".to_owned(),
                provenance: "judge".to_owned(),
            }],
        }),
    )?;
    assert!(validate_markdown(appended.replacement())?.is_valid());

    let target_id = task_id("T001")?;
    let reparsed = parse_markdown(appended.replacement())?;
    let task = reparsed
        .tasks
        .iter()
        .find(|task| task.id == target_id)
        .ok_or("replacement has no T001")?;
    let criterion = task
        .hidden_criteria
        .first()
        .ok_or("replacement lost its hidden criterion")?;

    // The verdict and rationale from the first mutation are untouched.
    assert_eq!(Some(HiddenVerdictJudgment::Pass), criterion.verdict);
    assert_eq!(Some("clearly met"), criterion.rationale.as_deref());
    // Both evidence records are present, in order.
    assert_eq!(2, criterion.evidence.len());
    let first_evidence = criterion.evidence.first().ok_or("missing first evidence")?;
    let second_evidence = criterion.evidence.get(1).ok_or("missing second evidence")?;
    assert_eq!("first-evidence", first_evidence.summary);
    assert_eq!("sentinel-appended-evidence", second_evidence.summary);
    assert_eq!("judge", second_evidence.provenance);

    // Never reaches the worker-safe projection.
    let worker_markdown = project_worker_markdown(appended.replacement())?;
    assert!(!worker_markdown.contains("sentinel-appended-evidence"));
    Ok(())
}

/// Re-judging a task must be able to update a hidden criterion's verdict
/// (e.g. `pass` -> `fail` on a second run) without silently erasing whatever judge-provenance evidence
/// (a run-level rationale, a prior panel disagreement) was already
/// attached to it by an earlier `AppendHiddenEvidence` mutation.
/// `RecordHiddenVerdict { evidence: None, .. }` is the typed way to say
/// "update the verdict only" -- unlike the pre-fix behavior, where the
/// only way to record a verdict at all also always replaced the evidence
/// list wholesale, even with an empty one.
#[test]
fn record_hidden_verdict_with_no_evidence_preserves_evidence_already_recorded()
-> Result<(), Box<dyn std::error::Error>> {
    // First run: pass, with one evidence record.
    let first_run = prepare_markdown_mutation(
        VALID_SINGLE,
        &request(Mutation::RecordHiddenVerdict {
            task_id: task_id("T001")?,
            index: 0,
            verdict: HiddenVerdictJudgment::Pass,
            rationale: "clearly met on the first run".to_owned(),
            evidence_needed: None,
            evidence: Some(vec![HiddenEvidenceRecord {
                summary: "first-run-evidence".to_owned(),
                provenance: "tool_authored".to_owned(),
            }]),
        }),
    )?;

    // Append a second evidence record, as a judge-panel disagreement or a
    // run-level rationale would be.
    let with_appended = prepare_markdown_mutation(
        first_run.replacement(),
        &request(Mutation::AppendHiddenEvidence {
            task_id: task_id("T001")?,
            index: 0,
            evidence: vec![HiddenEvidenceRecord {
                summary: "disagreement-evidence".to_owned(),
                provenance: "judge".to_owned(),
            }],
        }),
    )?;

    // Second run: the verdict flips to `fail`, but `evidence: None` -- the
    // shape a ledger uses to update only the verdict -- must not touch either
    // evidence record already recorded.
    let second_run = prepare_markdown_mutation(
        with_appended.replacement(),
        &request(Mutation::RecordHiddenVerdict {
            task_id: task_id("T001")?,
            index: 0,
            verdict: HiddenVerdictJudgment::Fail,
            rationale: "failed on the second run".to_owned(),
            evidence_needed: None,
            evidence: None,
        }),
    )?;
    assert!(validate_markdown(second_run.replacement())?.is_valid());

    let target_id = task_id("T001")?;
    let reparsed = parse_markdown(second_run.replacement())?;
    let task = reparsed
        .tasks
        .iter()
        .find(|task| task.id == target_id)
        .ok_or("replacement has no T001")?;
    let criterion = task
        .hidden_criteria
        .first()
        .ok_or("replacement lost its hidden criterion")?;

    // The verdict and rationale reflect the second run.
    assert_eq!(Some(HiddenVerdictJudgment::Fail), criterion.verdict);
    assert_eq!(
        Some("failed on the second run"),
        criterion.rationale.as_deref()
    );
    // Both evidence records from before this mutation survive it.
    assert_eq!(2, criterion.evidence.len());
    let first_evidence = criterion.evidence.first().ok_or("missing first evidence")?;
    let second_evidence = criterion.evidence.get(1).ok_or("missing second evidence")?;
    assert_eq!("first-run-evidence", first_evidence.summary);
    assert_eq!("disagreement-evidence", second_evidence.summary);
    Ok(())
}

#[test]
fn append_hidden_evidence_rejects_an_out_of_range_index() -> Result<(), Box<dyn std::error::Error>>
{
    let out_of_range = apply_mutation(
        &parse_markdown(VALID_SINGLE)?,
        &request(Mutation::AppendHiddenEvidence {
            task_id: task_id("T001")?,
            index: 99,
            evidence: vec![HiddenEvidenceRecord {
                summary: "irrelevant".to_owned(),
                provenance: "judge".to_owned(),
            }],
        }),
    );
    assert!(matches!(
        out_of_range,
        Err(StateError::HiddenCriterionOutOfRange { .. })
    ));
    Ok(())
}

#[test]
fn hidden_verdict_evidence_summary_and_provenance_are_validated()
-> Result<(), Box<dyn std::error::Error>> {
    let blank_summary = apply_mutation(
        &parse_markdown(VALID_SINGLE)?,
        &request(Mutation::RecordHiddenVerdict {
            task_id: task_id("T001")?,
            index: 0,
            verdict: HiddenVerdictJudgment::Pass,
            rationale: "clearly met".to_owned(),
            evidence_needed: None,
            evidence: Some(vec![HiddenEvidenceRecord {
                summary: "   ".to_owned(),
                provenance: "judge".to_owned(),
            }]),
        }),
    )?;
    let rendered = tftio_planner::render_markdown(blank_summary.plan())?;
    let report = validate_markdown(&rendered)?;
    assert!(!report.is_valid());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("summary must be a non-empty string")
    }));

    let bad_provenance = apply_mutation(
        &parse_markdown(VALID_SINGLE)?,
        &request(Mutation::RecordHiddenVerdict {
            task_id: task_id("T001")?,
            index: 0,
            verdict: HiddenVerdictJudgment::Pass,
            rationale: "clearly met".to_owned(),
            evidence_needed: None,
            evidence: Some(vec![HiddenEvidenceRecord {
                summary: "fine".to_owned(),
                provenance: "made_up".to_owned(),
            }]),
        }),
    )?;
    let rendered = tftio_planner::render_markdown(bad_provenance.plan())?;
    let report = validate_markdown(&rendered)?;
    assert!(!report.is_valid());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("provenance must be one of"))
    );
    Ok(())
}
