//! Stable human and JSON inspection adapters for planning-domain models.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::model::{
    Criticality, Evaluator, FormatVersion, HiddenVerdictJudgment, OperatorPlan, OperatorTask,
    PlanMode, PlanStatus, ProjectIdentity, Severity, SourceType, TaskGraphStatus, TaskId,
    TaskStatus, WorkerPlan, WorkerTask,
};

/// Render a concise human operator summary.
#[must_use]
pub fn operator_summary(plan: &OperatorPlan) -> String {
    format!(
        "{} ({})\nstatus: {}\n{}tasks: {}\nhidden criteria: {}\n",
        plan.metadata.title,
        plan.metadata.id,
        plan_status(plan.metadata.status),
        version_line(plan.metadata.format_version),
        plan.tasks.len(),
        plan.tasks
            .iter()
            .map(|task| task.hidden_criteria.len())
            .sum::<usize>()
    )
}

/// Render a concise human worker summary without concealed metadata.
#[must_use]
pub fn worker_summary(plan: &WorkerPlan) -> String {
    format!(
        "{} ({})\nstatus: {}\n{}tasks: {}\n",
        plan.metadata.title,
        plan.metadata.id,
        plan_status(plan.metadata.status),
        version_line(plan.metadata.format_version),
        plan.tasks.len()
    )
}

/// Render the stable version-1 operator JSON diagnostic representation.
///
/// # Errors
///
/// Returns a JSON serialization error if the compatibility adapter cannot encode the value.
pub fn operator_json(plan: &OperatorPlan) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "role": "operator",
        "plan": operator_plan_value(plan),
    }))
}

/// Render the stable version-1 worker JSON diagnostic representation.
///
/// # Errors
///
/// Returns a JSON serialization error if the compatibility adapter cannot encode the value.
pub fn worker_json(plan: &WorkerPlan) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "role": "worker",
        "plan": worker_plan_value(plan),
    }))
}

fn operator_plan_value(plan: &OperatorPlan) -> Value {
    let derived = plan.derived_blocks();
    json!({
        "metadata": metadata_value(plan),
        "adr": adr_value(plan),
        "operator_guidance_log": plan.operator_guidance_log,
        "decision_log": plan.decision_log,
        "tasks": plan
            .tasks
            .iter()
            .map(|task| {
                let blocks = blocks_value(
                    plan.metadata.format_version,
                    &task.id,
                    &task.blocks,
                    &derived,
                );
                operator_task_value(task, &blocks)
            })
            .collect::<Vec<_>>(),
        "execution_protocol": plan.execution_protocol,
    })
}

/// A task's downstream set as the plan's version defines it: authored in v1,
/// and in v2 derived from `depends_on` because the document never carries it.
fn blocks_value(
    version: FormatVersion,
    id: &TaskId,
    authored: &[TaskId],
    derived: &BTreeMap<TaskId, Vec<TaskId>>,
) -> Vec<String> {
    let ids = match version {
        FormatVersion::V1 { .. } => authored,
        FormatVersion::V2 => derived.get(id).map_or(&[][..], Vec::as_slice),
    };
    ids.iter().map(ToString::to_string).collect()
}

/// The version-dependent line the human summaries carry between `status` and
/// `tasks`: v1 reports its `mode`, and v2, which has none, reports its version.
fn version_line(version: FormatVersion) -> String {
    match version {
        FormatVersion::V1 { mode } => format!("mode: {}\n", plan_mode(mode)),
        FormatVersion::V2 => format!("plan_format_version: {}\n", version.number()),
    }
}

fn worker_plan_value(plan: &WorkerPlan) -> Value {
    let derived = plan.derived_blocks();
    let mut metadata = json!({
            "plan_format_version": plan.metadata.format_version.number(),
            "plan_id": plan.metadata.id.to_string(),
            "title": plan.metadata.title,
            "status": plan_status(plan.metadata.status),
            "created_at": plan.metadata.created_at,
            "updated_at": plan.metadata.updated_at,
            "owner": plan.metadata.owner,
            "source": {
                "type": source_type(plan.metadata.source.kind),
                "url": plan.metadata.source.url,
                "external_id": plan.metadata.source.external_id,
                "imported_at": plan.metadata.source.imported_at,
            },
            "bug": {
                "summary": plan.metadata.bug.summary,
                "severity": severity(plan.metadata.bug.severity),
                "affected_area": plan.metadata.bug.affected_area,
                "user_impact": plan.metadata.bug.user_impact,
            },
            "execution": {
                "requires_operator_approval_before_implementation": plan.metadata.execution.requires_operator_approval_before_implementation,
                "requires_plan_updates_during_execution": plan.metadata.execution.requires_plan_updates_during_execution,
                "task_graph_status": graph_status(plan.metadata.execution.task_graph_status),
            },
    });
    insert_mode(&mut metadata, plan.metadata.format_version);
    insert_project(&mut metadata, plan.metadata.project.as_ref());
    json!({
        "metadata": metadata,
        "adr": {
            "title": plan.adr.title,
            "problem_statement": plan.adr.problem_statement,
            "ticket": plan.adr.ticket,
            "discussion_summary": plan.adr.discussion_summary,
            "context": plan.adr.context,
            "constraints": plan.adr.constraints,
            "non_goals": plan.adr.non_goals,
            "decision": plan.adr.decision,
            "alternatives_considered": plan.adr.alternatives_considered,
            "consequences": plan.adr.consequences,
        },
        "tasks": plan
            .tasks
            .iter()
            .map(|task| {
                let blocks = blocks_value(
                    plan.metadata.format_version,
                    &task.id,
                    &task.blocks,
                    &derived,
                );
                worker_task_value(task, &blocks)
            })
            .collect::<Vec<_>>(),
        "execution_protocol": plan.execution_protocol,
    })
}

/// Restore `mode` to a rendered metadata object for a v1 plan, in the position
/// v1's JSON has always carried it. A v2 plan has no mode, so the key is absent
/// rather than null: a consumer that reads it gets nothing to mistake for one.
fn insert_mode(metadata: &mut Value, version: FormatVersion) {
    let FormatVersion::V1 { mode } = version else {
        return;
    };
    if let Some(object) = metadata.as_object_mut() {
        object.insert("mode".to_owned(), Value::String(plan_mode(mode).to_owned()));
    }
}

/// Insert `project` into a rendered metadata object when the plan carries
/// one, following the same absent-when-inapplicable pattern [`insert_mode`]
/// uses: present only when the plan has a project (which, today, only a v2
/// plan can), absent — not null — otherwise, so a consumer that reads the key
/// gets nothing to mistake for a project.
fn insert_project(metadata: &mut Value, project: Option<&ProjectIdentity>) {
    let Some(project) = project else {
        return;
    };
    let value = json!({
        "slug": project.slug.as_str(),
        "remote": project.remote.as_ref().map(|remote| remote.as_str().to_owned()),
    });
    if let Some(object) = metadata.as_object_mut() {
        object.insert("project".to_owned(), value);
    }
}

fn metadata_value(plan: &OperatorPlan) -> Value {
    let mut metadata = json!({
        "plan_format_version": plan.metadata.format_version.number(),
        "plan_id": plan.metadata.id.to_string(),
        "title": plan.metadata.title,
        "status": plan_status(plan.metadata.status),
        "created_at": plan.metadata.created_at,
        "updated_at": plan.metadata.updated_at,
        "owner": plan.metadata.owner,
        "source": {
            "type": source_type(plan.metadata.source.kind),
            "url": plan.metadata.source.url,
            "external_id": plan.metadata.source.external_id,
            "imported_at": plan.metadata.source.imported_at,
        },
        "bug": {
            "summary": plan.metadata.bug.summary,
            "severity": severity(plan.metadata.bug.severity),
            "affected_area": plan.metadata.bug.affected_area,
            "user_impact": plan.metadata.bug.user_impact,
        },
        "execution": {
            "requires_operator_approval_before_implementation": plan.metadata.execution.requires_operator_approval_before_implementation,
            "requires_plan_updates_during_execution": plan.metadata.execution.requires_plan_updates_during_execution,
            "task_graph_status": graph_status(plan.metadata.execution.task_graph_status),
        },
    });
    insert_mode(&mut metadata, plan.metadata.format_version);
    insert_project(&mut metadata, plan.metadata.project.as_ref());
    metadata
}

fn adr_value(plan: &OperatorPlan) -> Value {
    json!({
        "title": plan.adr.title,
        "problem_statement": plan.adr.problem_statement,
        "ticket": plan.adr.ticket,
        "discussion_summary": plan.adr.discussion_summary,
        "context": plan.adr.context,
        "constraints": plan.adr.constraints,
        "non_goals": plan.adr.non_goals,
        "decision": plan.adr.decision,
        "alternatives_considered": plan.adr.alternatives_considered,
        "consequences": plan.adr.consequences,
    })
}

fn operator_task_value(task: &OperatorTask, blocks: &[String]) -> Value {
    json!({
        "id": task.id.to_string(),
        "title": task.title,
        "status": task_status(task.status),
        "owner": task.owner,
        "depends_on": task.depends_on.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "blocks": blocks,
        "description": task.description,
        "work_items": task.work_items,
        "invariants": task.invariants,
        "acceptance_checks": task.acceptance_checks,
        "hidden_criteria": task.hidden_criteria.iter().map(|criterion| json!({
            "claim": criterion.claim,
            "criticality": criticality(criterion.criticality),
            "evaluator": evaluator(criterion.evaluator),
            "check": criterion.check,
            "ask": criterion.ask,
            "why_hidden": criterion.why_hidden,
            "counterfactual": criterion.counterfactual,
            "verdict": criterion.verdict.map(hidden_verdict_judgment),
            "rationale": criterion.rationale,
            "evidence_needed": criterion.evidence_needed,
            "evidence": criterion.evidence.iter().map(|record| json!({
                "summary": record.summary,
                "provenance": record.provenance,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "files": task.files.as_ref().map(|files| json!({
            "likely_read": files.likely_read,
            "likely_modify": files.likely_modify,
        })),
        "completion_evidence": task.completion_evidence,
    })
}

fn worker_task_value(task: &WorkerTask, blocks: &[String]) -> Value {
    json!({
        "id": task.id.to_string(),
        "title": task.title,
        "status": task_status(task.status),
        "owner": task.owner,
        "depends_on": task.depends_on.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "blocks": blocks,
        "description": task.description,
        "work_items": task.work_items,
        "invariants": task.invariants,
        "acceptance_checks": task.acceptance_checks,
        "files": task.files.as_ref().map(|files| json!({
            "likely_read": files.likely_read,
            "likely_modify": files.likely_modify,
        })),
        "completion_evidence": task.completion_evidence,
    })
}

const fn plan_status(value: PlanStatus) -> &'static str {
    match value {
        PlanStatus::Draft => "draft",
        PlanStatus::Approved => "approved",
        PlanStatus::Implemented => "implemented",
        PlanStatus::Abandoned => "abandoned",
    }
}

const fn plan_mode(value: PlanMode) -> &'static str {
    match value {
        PlanMode::Single => "single",
        PlanMode::Multi => "multi",
    }
}

const fn source_type(value: SourceType) -> &'static str {
    match value {
        SourceType::Asana => "asana",
        SourceType::Github => "github",
        SourceType::Gitlab => "gitlab",
        SourceType::Linear => "linear",
        SourceType::Manual => "manual",
        SourceType::Other => "other",
    }
}

const fn severity(value: Severity) -> &'static str {
    match value {
        Severity::Unknown => "unknown",
        Severity::Low => "low",
        Severity::Medium => "medium",
        Severity::High => "high",
        Severity::Critical => "critical",
    }
}

const fn graph_status(value: TaskGraphStatus) -> &'static str {
    match value {
        TaskGraphStatus::Draft => "draft",
        TaskGraphStatus::Ready => "ready",
        TaskGraphStatus::Executing => "executing",
        TaskGraphStatus::Complete => "complete",
    }
}

const fn task_status(value: TaskStatus) -> &'static str {
    match value {
        TaskStatus::NotStarted => "not_started",
        TaskStatus::Ready => "ready",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Done => "done",
        TaskStatus::Abandoned => "abandoned",
    }
}

const fn criticality(value: Criticality) -> &'static str {
    match value {
        Criticality::Must => "must",
        Criticality::Should => "should",
        Criticality::Nice => "nice",
    }
}

const fn evaluator(value: Evaluator) -> &'static str {
    match value {
        Evaluator::Automated => "automated",
        Evaluator::AgentEvaluated => "agent_evaluated",
        Evaluator::HumanJudgment => "human_judgment",
    }
}

/// Render a hidden criterion's recorded verdict, matching the exact
/// lowercase spelling `src/markdown.rs` reads and writes
/// (`crate::markdown`'s own `render`/`parse` pair for this field), so the
/// JSON view's `verdict` string round-trips against the Markdown a plan is
/// stored as.
const fn hidden_verdict_judgment(value: HiddenVerdictJudgment) -> &'static str {
    match value {
        HiddenVerdictJudgment::Pass => "pass",
        HiddenVerdictJudgment::Fail => "fail",
        HiddenVerdictJudgment::Undetermined => "undetermined",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_enum_names_cover_every_domain_variant() {
        assert_eq!(
            [
                PlanStatus::Draft,
                PlanStatus::Approved,
                PlanStatus::Implemented,
                PlanStatus::Abandoned,
            ]
            .map(plan_status),
            ["draft", "approved", "implemented", "abandoned"]
        );
        assert_eq!(
            [PlanMode::Single, PlanMode::Multi].map(plan_mode),
            ["single", "multi"]
        );
        assert_eq!(
            [
                SourceType::Asana,
                SourceType::Github,
                SourceType::Gitlab,
                SourceType::Linear,
                SourceType::Manual,
                SourceType::Other,
            ]
            .map(source_type),
            ["asana", "github", "gitlab", "linear", "manual", "other"]
        );
        assert_eq!(
            [
                Severity::Unknown,
                Severity::Low,
                Severity::Medium,
                Severity::High,
                Severity::Critical,
            ]
            .map(severity),
            ["unknown", "low", "medium", "high", "critical"]
        );
        assert_eq!(
            [
                TaskGraphStatus::Draft,
                TaskGraphStatus::Ready,
                TaskGraphStatus::Executing,
                TaskGraphStatus::Complete,
            ]
            .map(graph_status),
            ["draft", "ready", "executing", "complete"]
        );
        assert_eq!(
            [
                TaskStatus::NotStarted,
                TaskStatus::Ready,
                TaskStatus::InProgress,
                TaskStatus::Blocked,
                TaskStatus::Done,
                TaskStatus::Abandoned,
            ]
            .map(task_status),
            [
                "not_started",
                "ready",
                "in_progress",
                "blocked",
                "done",
                "abandoned",
            ]
        );
        assert_eq!(
            [Criticality::Must, Criticality::Should, Criticality::Nice].map(criticality),
            ["must", "should", "nice"]
        );
        assert_eq!(
            [
                Evaluator::Automated,
                Evaluator::AgentEvaluated,
                Evaluator::HumanJudgment,
            ]
            .map(evaluator),
            ["automated", "agent_evaluated", "human_judgment"]
        );
        assert_eq!(
            [
                HiddenVerdictJudgment::Pass,
                HiddenVerdictJudgment::Fail,
                HiddenVerdictJudgment::Undetermined,
            ]
            .map(hidden_verdict_judgment),
            ["pass", "fail", "undetermined"]
        );
    }
}
