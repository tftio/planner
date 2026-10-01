//! Deterministic Planning Document Format validation, for both v1 and v2.
//!
//! A plan's `plan_format_version` — and, under v1, its `mode` — selects one rule
//! set, and it is applied in full. The versions differ here on two axes, each
//! named by a predicate on [`FormatVersion`]: whether the handoff record is
//! conditional — v1 gates the two logs, the Execution Protocol and `work_items`
//! on `mode: multi`, while v2 requires them of every plan — and whether an
//! authored `files` mapping is required at all, which only v1 `mode: multi`
//! asks for. Everything a rule set does not select is identical between the
//! versions, which is why nothing below branches on the version twice.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
use crate::markdown::{ParseError, parse_markdown};
use crate::model::{Evaluator, FormatVersion, OperatorPlan, OperatorTask, TaskId, TaskStatus};

/// The diagnostics produced by one validation operation.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ValidationReport {
    /// Errors and warnings in deterministic discovery order.
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// Return whether the document has no error diagnostics.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }

    fn error(&mut self, code: DiagnosticCode, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(code, message, None));
    }

    fn warning(&mut self, code: DiagnosticCode, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::warning(code, message, None));
    }
}

/// Validate a representation-independent operator plan.
#[must_use]
pub fn validate_plan(plan: &OperatorPlan) -> ValidationReport {
    let mut report = ValidationReport::default();
    validate_metadata(plan, &mut report);
    validate_adr(plan, &mut report);
    validate_tasks(plan, &mut report);
    report
}

/// Parse and validate a Markdown planning document without filesystem I/O.
///
/// # Errors
///
/// Returns adapter diagnostics when the document cannot be parsed into the domain model.
pub fn validate_markdown(source: &str) -> Result<ValidationReport, ParseError> {
    let plan = parse_markdown(source)?;
    let mut report = validate_plan(&plan);
    validate_markdown_structure(source, &plan, &mut report);
    Ok(report)
}

/// Parse and validate Markdown with path-dependent filename and placement checks.
///
/// This function is pure: the path supplies naming context and is never read.
///
/// # Errors
///
/// Returns adapter diagnostics when the document cannot be parsed into the domain model.
pub fn validate_markdown_path(source: &str, path: &Path) -> Result<ValidationReport, ParseError> {
    let mut report = validate_markdown(source)?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if !valid_plan_filename(filename) {
        report.error(
            DiagnosticCode::InvalidFilename,
            format!("filename '{filename}' must match YYYY-MM-DD-<slug>.md"),
        );
    }
    if !path
        .components()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| {
            pair.first().is_some_and(|part| part.as_os_str() == "docs")
                && pair.get(1).is_some_and(|part| part.as_os_str() == "plans")
        })
    {
        report.warning(
            DiagnosticCode::InvalidFilename,
            "plan is not under docs/plans/",
        );
    }
    Ok(report)
}

fn validate_metadata(plan: &OperatorPlan, report: &mut ValidationReport) {
    required_text(report, "frontmatter plan_id", plan.metadata.id.as_str());
    required_text(report, "frontmatter title", &plan.metadata.title);
    required_text(report, "frontmatter owner", &plan.metadata.owner);
    required_text(
        report,
        "frontmatter bug.summary",
        &plan.metadata.bug.summary,
    );
    for (name, value) in [
        ("created_at", plan.metadata.created_at.as_str()),
        ("updated_at", plan.metadata.updated_at.as_str()),
    ] {
        if !date_shaped(value) {
            report.error(
                DiagnosticCode::InvalidDate,
                format!("frontmatter {name} must be a YYYY-MM-DD date"),
            );
        }
    }
    if plan.metadata.format_version.requires_handoff_record() {
        required_option_text(
            report,
            "Operator Guidance Log",
            plan.operator_guidance_log.as_deref(),
        );
        required_option_text(report, "Decision Log", plan.decision_log.as_deref());
        required_option_text(
            report,
            "Execution Protocol",
            plan.execution_protocol.as_deref(),
        );
    }
}

fn validate_adr(plan: &OperatorPlan, report: &mut ValidationReport) {
    for (name, value) in [
        ("ADR title", plan.adr.title.as_str()),
        ("Problem Statement", plan.adr.problem_statement.as_str()),
        ("Ticket", plan.adr.ticket.as_str()),
        ("Discussion Summary", plan.adr.discussion_summary.as_str()),
        ("Context", plan.adr.context.as_str()),
        ("Constraints", plan.adr.constraints.as_str()),
        ("Non-Goals", plan.adr.non_goals.as_str()),
        ("Decision", plan.adr.decision.as_str()),
        (
            "Alternatives Considered",
            plan.adr.alternatives_considered.as_str(),
        ),
        ("Consequences", plan.adr.consequences.as_str()),
    ] {
        required_text(report, name, value);
    }
}

fn validate_tasks(plan: &OperatorPlan, report: &mut ValidationReport) {
    if plan.tasks.is_empty() {
        report.error(
            DiagnosticCode::EmptyTaskGraph,
            "task graph 'tasks' list is empty",
        );
        return;
    }
    let by_id = plan
        .tasks
        .iter()
        .map(|task| (task.id.clone(), task))
        .collect::<BTreeMap<_, _>>();
    for task in &plan.tasks {
        validate_task(plan.metadata.format_version, task, &by_id, report);
    }
    validate_cycles(&by_id, report);
}

fn validate_task(
    version: FormatVersion,
    task: &OperatorTask,
    by_id: &BTreeMap<TaskId, &OperatorTask>,
    report: &mut ValidationReport,
) {
    let label = format!("task {}", task.id);
    required_text(report, &format!("{label} title"), &task.title);
    required_text(report, &format!("{label} description"), &task.description);
    required_non_empty_list(report, &format!("{label} invariants"), &task.invariants);
    required_non_empty_list(
        report,
        &format!("{label} acceptance_checks"),
        &task.acceptance_checks,
    );
    if task.status == TaskStatus::Done {
        required_option_text(
            report,
            &format!("{label} completion_evidence"),
            task.completion_evidence.as_deref(),
        );
    }
    if version.requires_handoff_record() {
        required_non_empty_list(report, &format!("{label} work_items"), &task.work_items);
    }
    if version.requires_authored_files() && task.files.is_none() {
        report.error(
            DiagnosticCode::RequiredValue,
            format!("{label} files must contain likely_read and likely_modify lists"),
        );
    }
    for dependency in &task.depends_on {
        if dependency == &task.id {
            report.error(
                DiagnosticCode::InvalidDependency,
                format!("{label} depends on itself"),
            );
        } else if !by_id.contains_key(dependency) {
            report.error(
                DiagnosticCode::InvalidDependency,
                format!("{label} depends on unknown task: {dependency}"),
            );
        } else if matches!(
            task.status,
            TaskStatus::Ready | TaskStatus::InProgress | TaskStatus::Done
        ) && by_id
            .get(dependency)
            .is_some_and(|dependency_task| dependency_task.status != TaskStatus::Done)
        {
            report.error(
                DiagnosticCode::DependencyStatus,
                format!("{label} is active or done but dependency {dependency} is not done"),
            );
        }
    }
    for (index, criterion) in task.hidden_criteria.iter().enumerate() {
        let criterion_label = format!("{label} hidden_criteria[{}]", index + 1);
        required_hidden_text(report, &criterion_label, "claim", &criterion.claim);
        required_hidden_text(
            report,
            &criterion_label,
            "why_hidden",
            &criterion.why_hidden,
        );
        required_hidden_text(
            report,
            &criterion_label,
            "counterfactual",
            &criterion.counterfactual,
        );
        let has_check = criterion.check.as_deref().is_some_and(substantive);
        let has_ask = criterion.ask.as_deref().is_some_and(substantive);
        if has_check == has_ask {
            report.error(
                DiagnosticCode::InvalidHiddenCriterion,
                format!("{criterion_label} must set exactly one of check/ask"),
            );
        } else if criterion.evaluator == Evaluator::Automated && has_ask {
            report.error(
                DiagnosticCode::InvalidHiddenCriterion,
                format!("{criterion_label} evaluator 'automated' requires check, not ask"),
            );
        } else if criterion.evaluator == Evaluator::HumanJudgment && has_check {
            report.error(
                DiagnosticCode::InvalidHiddenCriterion,
                format!("{criterion_label} evaluator 'human_judgment' requires ask, not check"),
            );
        }
        validate_hidden_verdict(report, &criterion_label, criterion);
    }
}

fn validate_hidden_verdict(
    report: &mut ValidationReport,
    criterion_label: &str,
    criterion: &crate::model::HiddenCriterion,
) {
    let has_evidence_needed = criterion
        .evidence_needed
        .as_deref()
        .is_some_and(substantive);
    match criterion.verdict {
        Some(crate::model::HiddenVerdictJudgment::Undetermined) => {
            if !has_evidence_needed {
                report.error(
                    DiagnosticCode::InvalidHiddenCriterion,
                    format!(
                        "{criterion_label} verdict 'undetermined' requires a non-empty evidence_needed"
                    ),
                );
            }
            required_hidden_text(
                report,
                criterion_label,
                "rationale",
                rationale_str(criterion),
            );
        }
        Some(
            crate::model::HiddenVerdictJudgment::Pass | crate::model::HiddenVerdictJudgment::Fail,
        ) => {
            if has_evidence_needed {
                report.error(
                    DiagnosticCode::InvalidHiddenCriterion,
                    format!(
                        "{criterion_label} evidence_needed is only permitted when verdict is 'undetermined'"
                    ),
                );
            }
            required_hidden_text(
                report,
                criterion_label,
                "rationale",
                rationale_str(criterion),
            );
        }
        None => {
            if has_evidence_needed {
                report.error(
                    DiagnosticCode::InvalidHiddenCriterion,
                    format!("{criterion_label} evidence_needed requires a verdict"),
                );
            }
            if criterion.rationale.as_deref().is_some_and(substantive) {
                report.error(
                    DiagnosticCode::InvalidHiddenCriterion,
                    format!("{criterion_label} rationale requires a verdict"),
                );
            }
        }
    }
    for (index, record) in criterion.evidence.iter().enumerate() {
        let evidence_label = format!("{criterion_label} evidence[{}]", index + 1);
        if !substantive(&record.summary) {
            report.error(
                DiagnosticCode::InvalidHiddenCriterion,
                format!("{evidence_label} summary must be a non-empty string"),
            );
        }
        if !EVIDENCE_PROVENANCES.contains(&record.provenance.as_str()) {
            report.error(
                DiagnosticCode::InvalidHiddenCriterion,
                format!("{evidence_label} provenance must be one of {EVIDENCE_PROVENANCES:?}"),
            );
        }
    }
}

/// The closed set of evidence provenance stamps accepted on a hidden
/// criterion's `evidence` records, mirroring `EVIDENCE_PROVENANCES` in the
/// `check-plan` validator exactly.
const EVIDENCE_PROVENANCES: [&str; 4] = [
    "tool_authored",
    "judge",
    "worker_narrated",
    "operator_observed",
];

fn rationale_str(criterion: &crate::model::HiddenCriterion) -> &str {
    criterion.rationale.as_deref().unwrap_or("")
}

fn validate_cycles(by_id: &BTreeMap<TaskId, &OperatorTask>, report: &mut ValidationReport) {
    let mut visited = BTreeSet::new();
    let mut visiting = BTreeSet::new();
    for id in by_id.keys() {
        if visit(id, by_id, &mut visiting, &mut visited) {
            report.error(
                DiagnosticCode::DependencyCycle,
                format!("task graph has a dependency cycle involving {id}"),
            );
            return;
        }
    }
}

fn visit(
    id: &TaskId,
    by_id: &BTreeMap<TaskId, &OperatorTask>,
    visiting: &mut BTreeSet<TaskId>,
    visited: &mut BTreeSet<TaskId>,
) -> bool {
    if visited.contains(id) {
        return false;
    }
    if !visiting.insert(id.clone()) {
        return true;
    }
    let cyclic = by_id.get(id).is_some_and(|task| {
        task.depends_on
            .iter()
            .filter(|dependency| by_id.contains_key(*dependency))
            .any(|dependency| visit(dependency, by_id, visiting, visited))
    });
    visiting.remove(id);
    visited.insert(id.clone());
    cyclic
}

#[derive(Debug, Clone)]
struct Heading {
    level: u8,
    text: String,
    line: usize,
}

fn validate_markdown_structure(source: &str, plan: &OperatorPlan, report: &mut ValidationReport) {
    let body = source
        .split_once("\n---\n")
        .map_or(source, |(_, body)| body);
    let headings = headings(body);
    let ordered = [
        (1, "adr"),
        (2, "problem statement"),
        (2, "source material"),
        (3, "ticket"),
        (3, "discussion summary"),
        (2, "context"),
        (2, "constraints"),
        (2, "non-goals"),
        (2, "decision"),
        (2, "alternatives considered"),
        (2, "consequences"),
    ];
    let mut previous = None;
    let mut out_of_order = false;
    for (level, name) in ordered {
        let position = find_heading(&headings, level, name);
        if position.is_none() {
            report.error(
                DiagnosticCode::InvalidDocumentStructure,
                format!(
                    "missing required heading: {} {name}",
                    "#".repeat(level.into())
                ),
            );
            continue;
        }
        if previous.is_some_and(|last| position.is_some_and(|current| current < last)) {
            out_of_order = true;
        }
        previous = position;
        if level >= 2 && position.is_some_and(|index| section_is_empty(body, &headings, index)) {
            report.error(
                DiagnosticCode::InvalidDocumentStructure,
                format!(
                    "required section is empty: {} {name}",
                    "#".repeat(level.into())
                ),
            );
        }
    }
    if out_of_order {
        report.error(
            DiagnosticCode::InvalidDocumentStructure,
            "ADR headings are present but out of order",
        );
    }
    let mut present = vec![(1, "task graph"), (1, "task details")];
    if plan.metadata.format_version.requires_handoff_record() {
        present.extend([
            (2, "operator guidance log"),
            (2, "decision log"),
            (1, "execution protocol"),
        ]);
    }
    for (level, name) in present {
        let position = find_heading(&headings, level, name);
        if position.is_none() {
            report.error(
                DiagnosticCode::InvalidDocumentStructure,
                format!(
                    "missing required heading: {} {name}",
                    "#".repeat(level.into())
                ),
            );
        } else if level >= 2
            && position.is_some_and(|index| section_is_empty(body, &headings, index))
        {
            report.error(
                DiagnosticCode::InvalidDocumentStructure,
                format!(
                    "required section is empty: {} {name}",
                    "#".repeat(level.into())
                ),
            );
        }
    }
    validate_task_details(&headings, plan, report);
}

fn headings(body: &str) -> Vec<Heading> {
    let mut result = Vec::new();
    let mut in_fence = false;
    for (line, text) in body.lines().enumerate() {
        let trimmed = text.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence {
            let hashes = text.bytes().take_while(|byte| *byte == b'#').count();
            if (1..=6).contains(&hashes)
                && text.get(hashes..).is_some_and(|rest| rest.starts_with(' '))
            {
                let heading_text = text.get(hashes..).unwrap_or_default().trim();
                if !heading_text.is_empty() {
                    result.push(Heading {
                        level: u8::try_from(hashes).unwrap_or_default(),
                        text: heading_text.to_owned(),
                        line,
                    });
                }
            }
        }
    }
    result
}

fn find_heading(headings: &[Heading], level: u8, name: &str) -> Option<usize> {
    headings.iter().position(|heading| {
        if heading.level != level {
            return false;
        }
        let text = heading.text.to_lowercase();
        if level == 1 && name == "adr" {
            text.starts_with("adr")
        } else {
            text == name
                || text.starts_with(&format!("{name} "))
                || text.starts_with(&format!("{name} —"))
                || text.starts_with(&format!("{name} -"))
        }
    })
}

fn section_is_empty(body: &str, headings: &[Heading], index: usize) -> bool {
    let Some(heading) = headings.get(index) else {
        return true;
    };
    let end = headings
        .iter()
        .skip(index.saturating_add(1))
        .find(|candidate| candidate.level <= heading.level)
        .map_or_else(|| body.lines().count(), |candidate| candidate.line);
    body.lines()
        .skip(heading.line.saturating_add(1))
        .take(end.saturating_sub(heading.line.saturating_add(1)))
        .all(|line| {
            let text = line.trim();
            text.is_empty()
                || text.starts_with("<!--")
                || text.starts_with('#')
                || (text.starts_with('<') && text.ends_with('>'))
        })
}

fn validate_task_details(headings: &[Heading], plan: &OperatorPlan, report: &mut ValidationReport) {
    let graph_ids = plan
        .tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<BTreeSet<_>>();
    let mut detail_ids = BTreeSet::new();
    let mut in_details = false;
    for heading in headings {
        if heading.level == 1 {
            in_details = heading.text.eq_ignore_ascii_case("task details");
        } else if in_details && heading.level == 2 {
            let candidate = heading.text.split_whitespace().next().unwrap_or_default();
            if let Ok(id) = TaskId::parse(candidate) {
                detail_ids.insert(id.clone());
                if let Some(task) = plan.tasks.iter().find(|task| task.id == id) {
                    let detail_title = heading
                        .text
                        .get(candidate.len()..)
                        .unwrap_or_default()
                        .trim_start_matches([' ', '—', '-'])
                        .trim();
                    if !detail_title.is_empty() && detail_title != task.title {
                        report.warning(
                            DiagnosticCode::DuplicatedStateMismatch,
                            format!(
                                "Task Details title for {} differs from Task Graph title",
                                task.id
                            ),
                        );
                    }
                }
            }
        }
    }
    for id in graph_ids.difference(&detail_ids) {
        report.error(
            DiagnosticCode::TaskDetailMismatch,
            format!("task {id} has no matching '## {id}' section in Task Details"),
        );
    }
    for id in detail_ids.difference(&graph_ids) {
        report.error(
            DiagnosticCode::TaskDetailMismatch,
            format!("Task Details section '## {id}' has no matching task in graph"),
        );
    }
}

fn required_text(report: &mut ValidationReport, name: &str, value: &str) {
    if !substantive(value) {
        report.error(
            DiagnosticCode::RequiredValue,
            format!("{name} must be non-empty"),
        );
    }
}

fn required_option_text(report: &mut ValidationReport, name: &str, value: Option<&str>) {
    if !value.is_some_and(substantive) {
        report.error(
            DiagnosticCode::RequiredValue,
            format!("{name} must be present and non-empty"),
        );
    }
}

fn required_non_empty_list(report: &mut ValidationReport, name: &str, values: &[String]) {
    if !values.iter().any(|value| substantive(value)) {
        report.error(
            DiagnosticCode::RequiredValue,
            format!("{name} must be a non-empty list"),
        );
    }
}

fn required_hidden_text(report: &mut ValidationReport, label: &str, name: &str, value: &str) {
    if !substantive(value) {
        report.error(
            DiagnosticCode::InvalidHiddenCriterion,
            format!("{label} {name} must be a non-empty string"),
        );
    }
}

fn substantive(value: &str) -> bool {
    !value.trim().is_empty()
}

fn date_shaped(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

fn valid_plan_filename(filename: &str) -> bool {
    let Some(stem) = filename.strip_suffix(".md") else {
        return false;
    };
    let Some((date, slug)) = stem.split_at_checked(10) else {
        return false;
    };
    date_shaped(date)
        && slug.strip_prefix('-').is_some_and(|value| {
            !value.is_empty()
                && value.split('-').all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                })
        })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        Heading, ValidationReport, headings, section_is_empty, valid_plan_filename,
        validate_markdown_path, validate_markdown_structure, validate_plan, validate_task_details,
    };
    use crate::diagnostic::DiagnosticCode;
    use crate::markdown::parse_markdown;
    use crate::model::{Evaluator, FormatVersion, PlanMode, TaskId, TaskStatus};

    const VALID: &str = include_str!("../tests/fixtures/valid-single.md");

    #[test]
    fn semantic_validation_reports_model_inconsistencies() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut plan = parse_markdown(VALID)?;
        plan.metadata.title.clear();
        plan.metadata.owner.clear();
        plan.metadata.bug.summary.clear();
        plan.metadata.created_at = "bad".to_owned();
        plan.metadata.updated_at = "2026/08/28".to_owned();
        plan.metadata.format_version = FormatVersion::V1 {
            mode: PlanMode::Multi,
        };
        plan.operator_guidance_log = None;
        plan.decision_log = Some(" ".to_owned());
        plan.execution_protocol = None;
        plan.adr.title.clear();
        plan.adr.problem_statement.clear();
        plan.adr.ticket.clear();
        plan.adr.discussion_summary.clear();
        plan.adr.context.clear();
        plan.adr.constraints.clear();
        plan.adr.non_goals.clear();
        plan.adr.decision.clear();
        plan.adr.alternatives_considered.clear();
        plan.adr.consequences.clear();

        let mut dependency = plan.tasks.first().cloned().ok_or("fixture has no task")?;
        dependency.id = TaskId::parse("T002")?;
        dependency.status = TaskStatus::NotStarted;
        dependency.depends_on.clear();
        dependency.hidden_criteria.clear();

        let task = plan.tasks.first_mut().ok_or("fixture has no task")?;
        task.title.clear();
        task.description.clear();
        task.invariants = vec![" ".to_owned()];
        task.acceptance_checks.clear();
        task.status = TaskStatus::Done;
        task.completion_evidence = None;
        task.work_items.clear();
        task.files = None;
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
        plan.tasks.push(dependency);

        let report = validate_plan(&plan);
        let codes = report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        for expected in [
            DiagnosticCode::RequiredValue,
            DiagnosticCode::InvalidDate,
            DiagnosticCode::InvalidDependency,
            DiagnosticCode::DependencyStatus,
            DiagnosticCode::DependencyCycle,
            DiagnosticCode::InvalidHiddenCriterion,
        ] {
            assert!(codes.contains(&expected));
        }
        assert!(!report.is_valid());
        Ok(())
    }

    #[test]
    fn empty_graph_is_invalid() -> Result<(), Box<dyn std::error::Error>> {
        let mut plan = parse_markdown(VALID)?;
        plan.tasks.clear();

        let report = validate_plan(&plan);
        assert_eq!(
            report.diagnostics.first().map(|diagnostic| diagnostic.code),
            Some(DiagnosticCode::EmptyTaskGraph)
        );
        Ok(())
    }

    #[test]
    fn structural_validation_covers_missing_empty_and_reordered_sections()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut plan = parse_markdown(VALID)?;
        let mut missing = ValidationReport::default();
        validate_markdown_structure("", &plan, &mut missing);
        assert!(!missing.is_valid());

        let reordered = VALID
            .replace("## Context", "## TEMP")
            .replace("## Constraints", "## Context")
            .replace("## TEMP", "## Constraints");
        let mut reordered_report = ValidationReport::default();
        validate_markdown_structure(&reordered, &plan, &mut reordered_report);
        assert!(reordered_report.diagnostics.iter().any(|diagnostic| {
            diagnostic.message == "ADR headings are present but out of order"
        }));

        let empty = VALID.replace(
            "## Context\n\nThe fixture exercises typed YAML and exact Markdown retention.",
            "## Context\n\n<fill this in>",
        );
        let mut empty_report = ValidationReport::default();
        validate_markdown_structure(&empty, &plan, &mut empty_report);
        assert!(
            empty_report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("required section is empty"))
        );

        plan.metadata.format_version = FormatVersion::V1 {
            mode: PlanMode::Multi,
        };
        let multi = VALID.replace(
            "# Task Graph",
            "## Operator Guidance Log\n\n<fill this in>\n\n# Task Graph",
        );
        let mut multi_report = ValidationReport::default();
        validate_markdown_structure(&multi, &plan, &mut multi_report);
        assert!(multi_report.diagnostics.len() >= 3);
        Ok(())
    }

    #[test]
    fn heading_and_detail_helpers_cover_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        let plan = parse_markdown(VALID)?;
        let parsed = headings("```markdown\n# Ignored\n```\n# Task Details\n## T999 — Extra\n# \n");
        let mut report = ValidationReport::default();
        validate_task_details(&parsed, &plan, &mut report);
        assert!(
            report
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == DiagnosticCode::TaskDetailMismatch)
                .count()
                >= 2
        );

        assert!(section_is_empty("", &[], 0));
        let placeholder = [Heading {
            level: 2,
            text: "Context".to_owned(),
            line: 0,
        }];
        assert!(section_is_empty(
            "## Context\n\n<!-- comment -->\n### child\n<placeholder>",
            &placeholder,
            0
        ));
        Ok(())
    }

    #[test]
    fn filename_and_path_rules_cover_valid_and_invalid_shapes()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(valid_plan_filename("2026-08-28-valid-plan1.md"));
        assert!(!valid_plan_filename("2026-08-28-valid-plan.txt"));
        assert!(!valid_plan_filename("short.md"));
        assert!(!valid_plan_filename("2026-08-28-Bad--slug.md"));

        let report =
            validate_markdown_path(VALID, Path::new("docs/plans/2026-08-28-parser-fixture.md"))?;
        assert!(report.is_valid());
        Ok(())
    }
}
