//! Pure lifecycle transitions over planning-domain models.

use thiserror::Error;

use crate::model::{
    HiddenEvidenceRecord, HiddenVerdictJudgment, OperatorPlan, PlanStatus, TaskGraphStatus, TaskId,
    TaskStatus,
};

/// A lifecycle operation for one task.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TaskAction {
    /// Make a not-started task ready after its dependencies complete.
    Ready,
    /// Start a ready task.
    Start,
    /// Block an active task and record why.
    Block(String),
    /// Complete an in-progress task and append evidence.
    Complete(String),
    /// Reopen a terminal or blocked task and record why.
    Reopen(String),
    /// Abandon a nonterminal task and record why.
    Abandon(String),
}

/// A lifecycle operation for the plan as a whole.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PlanAction {
    /// Approve a draft plan.
    Approve,
    /// Mark an approved plan implemented after every task is terminal.
    Implement,
    /// Abandon a draft or approved plan and record why.
    Abandon(String),
}

/// A requested mutation over an operator plan.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Mutation {
    /// Apply a task lifecycle operation.
    Task {
        /// Target task.
        task_id: TaskId,
        /// Requested lifecycle operation.
        action: TaskAction,
    },
    /// Apply a plan lifecycle operation.
    Plan(PlanAction),
    /// Append an operator-guidance entry.
    AddGuidance(String),
    /// Append a decision entry.
    AddDecision(String),
    /// Append text to a task's completion evidence, without any lifecycle
    /// transition. A supervising ledger uses it to record visible-criterion
    /// verdicts, automated check results, git facts, and residual summaries,
    /// so that record lives in the plan through the same validated mutation
    /// path as every other change.
    AppendCompletionEvidence {
        /// The task to append to.
        task_id: TaskId,
        /// The text to append.
        text: String,
    },
    /// Record a verdict against one of a task's hidden criteria, addressed
    /// by its position in the task's `hidden_criteria` list (0-indexed).
    /// Hidden-criteria verdicts, rationale, and evidence never reach the
    /// worker-safe projection (`WorkerTask` carries no `hidden_criteria`
    /// field at all).
    RecordHiddenVerdict {
        /// The task whose hidden criterion this verdict judges.
        task_id: TaskId,
        /// The hidden criterion's position in `hidden_criteria`.
        index: usize,
        /// The judgment.
        verdict: HiddenVerdictJudgment,
        /// The rationale behind the judgment.
        rationale: String,
        /// What would resolve the judgment; required (and only permitted)
        /// when `verdict` is [`HiddenVerdictJudgment::Undetermined`].
        evidence_needed: Option<String>,
        /// Evidence gathered in service of this verdict. `Some` replaces
        /// any evidence previously recorded for this criterion wholesale;
        /// `None` preserves whatever evidence is already recorded
        /// (re-judging a task must be able to update a hidden criterion's
        /// verdict without silently erasing judge-provenance evidence -- a
        /// run-level rationale, or a prior panel disagreement -- that a
        /// *different* mutation (`AppendHiddenEvidence`) already attached
        /// to it). A caller that
        /// wants to attach fresh evidence alongside a verdict update still
        /// does so through a separate `AppendHiddenEvidence` mutation,
        /// after this one, rather than through `Some` here, unless it
        /// specifically means to discard everything recorded before.
        evidence: Option<Vec<HiddenEvidenceRecord>>,
    },
    /// Append evidence to one of a task's hidden criteria, addressed by its
    /// position in the task's `hidden_criteria` list (0-indexed), without
    /// touching its `verdict`, `rationale`, or `evidence_needed`. A
    /// supervising ledger uses it to attach judge-provenance evidence (a run-level
    /// rationale, or a disagreement between two judge providers) to a
    /// hidden criterion whose verdict was set by another mutation, so the
    /// same information never has to travel through a worker-safe or
    /// operator-guidance surface that hidden-criteria material must not
    /// reach.
    AppendHiddenEvidence {
        /// The task whose hidden criterion this evidence belongs to.
        task_id: TaskId,
        /// The hidden criterion's position in `hidden_criteria`.
        index: usize,
        /// Evidence to append, in order, after whatever is already
        /// recorded for this criterion.
        evidence: Vec<HiddenEvidenceRecord>,
    },
}

/// Mutation request with the date used for synchronized metadata and records.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MutationRequest {
    /// Mutation date in `YYYY-MM-DD` form.
    pub date: String,
    /// Requested domain operation.
    pub mutation: Mutation,
}

/// Successful pure mutation result.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MutationOutcome {
    plan: OperatorPlan,
    summary: String,
}

impl MutationOutcome {
    /// Borrow the changed plan.
    #[must_use]
    pub const fn plan(&self) -> &OperatorPlan {
        &self.plan
    }

    /// Borrow the human-readable operation summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }
}

/// Rejected lifecycle mutation.
#[derive(Debug, Clone, Error, Eq, PartialEq)]
pub enum StateError {
    /// The supplied mutation date does not match the format contract.
    #[error("mutation date must use YYYY-MM-DD form")]
    InvalidDate,
    /// No task has the requested identifier.
    #[error("task {0} does not exist")]
    UnknownTask(TaskId),
    /// The requested transition is not in the state machine.
    #[error("cannot {action} {subject} from {status}")]
    InvalidTransition {
        /// Target description.
        subject: String,
        /// Requested action.
        action: &'static str,
        /// Current status.
        status: &'static str,
    },
    /// A required reason, evidence body, or log entry is empty.
    #[error("{0} must not be empty")]
    EmptyText(&'static str),
    /// One or more dependencies are not complete.
    #[error("task dependencies are not done: {0}")]
    DependenciesNotDone(String),
    /// The plan cannot be implemented while tasks remain nonterminal.
    #[error("plan has nonterminal tasks: {0}")]
    NonterminalTasks(String),
    /// A terminal plan cannot accept task or log mutations.
    #[error("plan status {0} does not permit this mutation")]
    TerminalPlan(&'static str),
    /// A hidden-criterion verdict named an index outside the task's
    /// `hidden_criteria` list.
    #[error("task {task_id} has no hidden_criteria[{index}] (has {len})")]
    HiddenCriterionOutOfRange {
        /// The task the verdict targeted.
        task_id: TaskId,
        /// The out-of-range index.
        index: usize,
        /// How many hidden criteria the task actually has.
        len: usize,
    },
    /// `evidence_needed` was supplied inconsistently with the verdict
    /// (required with `undetermined`, forbidden otherwise), or the
    /// rationale was empty.
    #[error("{0}")]
    InvalidHiddenVerdict(String),
}

/// Apply a lifecycle transition without performing representation or filesystem I/O.
///
/// # Errors
///
/// Returns a typed rejection when the date, transition, dependencies, or required text
/// violates the lifecycle contract.
pub fn apply_mutation(
    source: &OperatorPlan,
    request: &MutationRequest,
) -> Result<MutationOutcome, StateError> {
    if !date_shaped(&request.date) {
        return Err(StateError::InvalidDate);
    }
    let mut plan = source.clone();
    let summary = match &request.mutation {
        Mutation::Task { task_id, action } => {
            ensure_plan_mutable(&plan)?;
            apply_task_action(&mut plan, task_id, action, &request.date)?
        }
        Mutation::Plan(action) => apply_plan_action(&mut plan, action, &request.date)?,
        Mutation::AddGuidance(entry) => {
            ensure_plan_mutable(&plan)?;
            require_text(entry, "operator guidance")?;
            append_log(&mut plan.operator_guidance_log, &request.date, entry);
            "appended operator guidance".to_owned()
        }
        Mutation::AddDecision(entry) => {
            ensure_plan_mutable(&plan)?;
            require_text(entry, "decision")?;
            append_log(&mut plan.decision_log, &request.date, entry);
            "appended decision".to_owned()
        }
        Mutation::AppendCompletionEvidence { task_id, text } => {
            ensure_plan_mutable(&plan)?;
            require_text(text, "completion evidence")?;
            let task = plan
                .tasks
                .iter_mut()
                .find(|task| &task.id == task_id)
                .ok_or_else(|| StateError::UnknownTask(task_id.clone()))?;
            append_task_record(
                &mut task.completion_evidence,
                &request.date,
                "evidence",
                text,
            );
            format!("appended completion evidence to task {task_id}")
        }
        Mutation::RecordHiddenVerdict {
            task_id,
            index,
            verdict,
            rationale,
            evidence_needed,
            evidence,
        } => apply_record_hidden_verdict(
            &mut plan,
            task_id,
            *index,
            *verdict,
            rationale,
            evidence_needed.as_ref(),
            evidence.as_deref(),
        )?,
        Mutation::AppendHiddenEvidence {
            task_id,
            index,
            evidence,
        } => apply_append_hidden_evidence(&mut plan, task_id, *index, evidence)?,
    };
    plan.metadata.updated_at.clone_from(&request.date);
    Ok(MutationOutcome { plan, summary })
}

fn apply_record_hidden_verdict(
    plan: &mut OperatorPlan,
    task_id: &TaskId,
    index: usize,
    verdict: HiddenVerdictJudgment,
    rationale: &str,
    evidence_needed: Option<&String>,
    evidence: Option<&[HiddenEvidenceRecord]>,
) -> Result<String, StateError> {
    ensure_plan_mutable(plan)?;
    require_text(rationale, "hidden criterion rationale")?;
    let evidence_needed_ok = match verdict {
        HiddenVerdictJudgment::Undetermined => {
            evidence_needed.is_some_and(|value| !value.trim().is_empty())
        }
        HiddenVerdictJudgment::Pass | HiddenVerdictJudgment::Fail => {
            evidence_needed.is_none_or(|value| value.trim().is_empty())
        }
    };
    if !evidence_needed_ok {
        return Err(StateError::InvalidHiddenVerdict(format!(
            "task {task_id} hidden_criteria[{index}]: evidence_needed must be set \
             if and only if the verdict is undetermined"
        )));
    }
    let criterion = hidden_criterion_mut(plan, task_id, index)?;
    criterion.verdict = Some(verdict);
    criterion.rationale = Some(rationale.to_owned());
    criterion.evidence_needed = evidence_needed.cloned();
    // `None` preserves whatever evidence this criterion already carries, so
    // re-judging a task does not erase evidence another mutation attached;
    // `Some` replaces it wholesale.
    if let Some(evidence) = evidence {
        criterion.evidence = evidence.to_vec();
    }
    Ok(format!(
        "recorded hidden verdict for task {task_id} hidden_criteria[{index}]"
    ))
}

fn apply_append_hidden_evidence(
    plan: &mut OperatorPlan,
    task_id: &TaskId,
    index: usize,
    evidence: &[HiddenEvidenceRecord],
) -> Result<String, StateError> {
    ensure_plan_mutable(plan)?;
    let criterion = hidden_criterion_mut(plan, task_id, index)?;
    criterion.evidence.extend(evidence.iter().cloned());
    Ok(format!(
        "appended hidden evidence for task {task_id} hidden_criteria[{index}]"
    ))
}

fn hidden_criterion_mut<'a>(
    plan: &'a mut OperatorPlan,
    task_id: &TaskId,
    index: usize,
) -> Result<&'a mut crate::model::HiddenCriterion, StateError> {
    let task = plan
        .tasks
        .iter_mut()
        .find(|task| &task.id == task_id)
        .ok_or_else(|| StateError::UnknownTask(task_id.clone()))?;
    let len = task.hidden_criteria.len();
    task.hidden_criteria
        .get_mut(index)
        .ok_or_else(|| StateError::HiddenCriterionOutOfRange {
            task_id: task_id.clone(),
            index,
            len,
        })
}

const fn ensure_plan_mutable(plan: &OperatorPlan) -> Result<(), StateError> {
    match plan.metadata.status {
        PlanStatus::Draft | PlanStatus::Approved => Ok(()),
        PlanStatus::Implemented => Err(StateError::TerminalPlan("implemented")),
        PlanStatus::Abandoned => Err(StateError::TerminalPlan("abandoned")),
    }
}

fn apply_task_action(
    plan: &mut OperatorPlan,
    task_id: &TaskId,
    action: &TaskAction,
    date: &str,
) -> Result<String, StateError> {
    let current = plan
        .tasks
        .iter()
        .find(|task| &task.id == task_id)
        .ok_or_else(|| StateError::UnknownTask(task_id.clone()))?;
    let current_status = current.status;
    let dependencies = current.depends_on.clone();
    if matches!(
        action,
        TaskAction::Ready | TaskAction::Start | TaskAction::Reopen(_)
    ) {
        require_dependencies_done(plan, &dependencies)?;
    }
    validate_task_transition(current_status, action, task_id)?;
    let unknown_task = StateError::UnknownTask(task_id.clone());
    let task = plan
        .tasks
        .iter_mut()
        .find(|task| &task.id == task_id)
        .ok_or(unknown_task)?;
    let action_name = match action {
        TaskAction::Ready => {
            task.status = TaskStatus::Ready;
            "marked ready"
        }
        TaskAction::Start => {
            task.status = TaskStatus::InProgress;
            "started"
        }
        TaskAction::Block(reason) => {
            require_text(reason, "block reason")?;
            task.status = TaskStatus::Blocked;
            append_task_record(&mut task.completion_evidence, date, "blocked", reason);
            "blocked"
        }
        TaskAction::Complete(evidence) => {
            require_text(evidence, "completion evidence")?;
            task.status = TaskStatus::Done;
            append_task_record(&mut task.completion_evidence, date, "completed", evidence);
            "completed"
        }
        TaskAction::Reopen(reason) => {
            require_text(reason, "reopen reason")?;
            task.status = TaskStatus::Ready;
            append_task_record(&mut task.completion_evidence, date, "reopened", reason);
            "reopened"
        }
        TaskAction::Abandon(reason) => {
            require_text(reason, "abandon reason")?;
            task.status = TaskStatus::Abandoned;
            append_task_record(&mut task.completion_evidence, date, "abandoned", reason);
            "abandoned"
        }
    };
    plan.metadata.execution.task_graph_status = aggregate_graph_status(plan);
    Ok(format!("{action_name} task {task_id}"))
}

fn validate_task_transition(
    status: TaskStatus,
    action: &TaskAction,
    task_id: &TaskId,
) -> Result<(), StateError> {
    let allowed = matches!(
        (status, action),
        (TaskStatus::NotStarted, TaskAction::Ready)
            | (TaskStatus::Ready, TaskAction::Start)
            | (
                TaskStatus::Ready | TaskStatus::InProgress,
                TaskAction::Block(_)
            )
            | (TaskStatus::InProgress, TaskAction::Complete(_))
            | (
                TaskStatus::Blocked | TaskStatus::Done | TaskStatus::Abandoned,
                TaskAction::Reopen(_)
            )
            | (
                TaskStatus::NotStarted
                    | TaskStatus::Ready
                    | TaskStatus::InProgress
                    | TaskStatus::Blocked,
                TaskAction::Abandon(_)
            )
    );
    if allowed {
        Ok(())
    } else {
        Err(StateError::InvalidTransition {
            subject: format!("task {task_id}"),
            action: task_action_name(action),
            status: task_status_name(status),
        })
    }
}

fn apply_plan_action(
    plan: &mut OperatorPlan,
    action: &PlanAction,
    date: &str,
) -> Result<String, StateError> {
    match (plan.metadata.status, action) {
        (PlanStatus::Draft, PlanAction::Approve) => {
            plan.metadata.status = PlanStatus::Approved;
            plan.metadata.execution.task_graph_status = aggregate_graph_status(plan);
            Ok("approved plan".to_owned())
        }
        (PlanStatus::Approved, PlanAction::Implement) => {
            let nonterminal = plan
                .tasks
                .iter()
                .filter(|task| !matches!(task.status, TaskStatus::Done | TaskStatus::Abandoned))
                .map(|task| task.id.to_string())
                .collect::<Vec<_>>();
            if nonterminal.is_empty() {
                plan.metadata.status = PlanStatus::Implemented;
                plan.metadata.execution.task_graph_status = TaskGraphStatus::Complete;
                Ok("marked plan implemented".to_owned())
            } else {
                Err(StateError::NonterminalTasks(nonterminal.join(", ")))
            }
        }
        (PlanStatus::Draft | PlanStatus::Approved, PlanAction::Abandon(reason)) => {
            require_text(reason, "plan abandon reason")?;
            plan.metadata.status = PlanStatus::Abandoned;
            append_log(
                &mut plan.decision_log,
                date,
                &format!("Plan abandoned: {reason}"),
            );
            Ok("abandoned plan".to_owned())
        }
        (status, action) => Err(StateError::InvalidTransition {
            subject: "plan".to_owned(),
            action: plan_action_name(action),
            status: plan_status_name(status),
        }),
    }
}

fn require_dependencies_done(
    plan: &OperatorPlan,
    dependencies: &[TaskId],
) -> Result<(), StateError> {
    let unfinished = dependencies
        .iter()
        .filter(|dependency| {
            plan.tasks
                .iter()
                .find(|task| &task.id == *dependency)
                .is_none_or(|task| task.status != TaskStatus::Done)
        })
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if unfinished.is_empty() {
        Ok(())
    } else {
        Err(StateError::DependenciesNotDone(unfinished.join(", ")))
    }
}

fn aggregate_graph_status(plan: &OperatorPlan) -> TaskGraphStatus {
    if plan
        .tasks
        .iter()
        .all(|task| matches!(task.status, TaskStatus::Done | TaskStatus::Abandoned))
    {
        return TaskGraphStatus::Complete;
    }
    if plan.tasks.iter().any(|task| {
        matches!(
            task.status,
            TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::Done | TaskStatus::Abandoned
        )
    }) {
        return TaskGraphStatus::Executing;
    }
    if plan
        .tasks
        .iter()
        .any(|task| task.status == TaskStatus::Ready)
    {
        TaskGraphStatus::Ready
    } else {
        TaskGraphStatus::Draft
    }
}

fn append_task_record(target: &mut Option<String>, date: &str, verb: &str, body: &str) {
    append_text(target, &format!("{date} — {verb}: {}", body.trim()));
}

fn append_log(target: &mut Option<String>, date: &str, entry: &str) {
    append_text(target, &format!("### {date} — planner\n\n{}", entry.trim()));
}

fn append_text(target: &mut Option<String>, entry: &str) {
    match target {
        Some(existing) if !existing.trim().is_empty() => {
            existing.push_str("\n\n");
            existing.push_str(entry);
        }
        Some(existing) => existing.push_str(entry),
        None => *target = Some(entry.to_owned()),
    }
}

fn require_text(value: &str, label: &'static str) -> Result<(), StateError> {
    if value.trim().is_empty() {
        Err(StateError::EmptyText(label))
    } else {
        Ok(())
    }
}

fn date_shaped(value: &str) -> bool {
    let mut parts = value.split('-');
    let year = parts.next();
    let month = parts.next();
    let day = parts.next();
    parts.next().is_none()
        && year
            .is_some_and(|part| part.len() == 4 && part.bytes().all(|byte| byte.is_ascii_digit()))
        && month
            .is_some_and(|part| part.len() == 2 && part.bytes().all(|byte| byte.is_ascii_digit()))
        && day.is_some_and(|part| part.len() == 2 && part.bytes().all(|byte| byte.is_ascii_digit()))
}

const fn task_action_name(action: &TaskAction) -> &'static str {
    match action {
        TaskAction::Ready => "mark ready",
        TaskAction::Start => "start",
        TaskAction::Block(_) => "block",
        TaskAction::Complete(_) => "complete",
        TaskAction::Reopen(_) => "reopen",
        TaskAction::Abandon(_) => "abandon",
    }
}

const fn plan_action_name(action: &PlanAction) -> &'static str {
    match action {
        PlanAction::Approve => "approve",
        PlanAction::Implement => "implement",
        PlanAction::Abandon(_) => "abandon",
    }
}

const fn task_status_name(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::NotStarted => "not_started",
        TaskStatus::Ready => "ready",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Done => "done",
        TaskStatus::Abandoned => "abandoned",
    }
}

const fn plan_status_name(status: PlanStatus) -> &'static str {
    match status {
        PlanStatus::Draft => "draft",
        PlanStatus::Approved => "approved",
        PlanStatus::Implemented => "implemented",
        PlanStatus::Abandoned => "abandoned",
    }
}
