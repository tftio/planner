//! Representation-independent Planning Document Format domain model, covering
//! both v1 and v2.

use std::collections::BTreeMap;
use std::fmt;

use tftio_lib::project::{NormalizedRemote, Slug};
use thiserror::Error;

/// The format version a plan declares, carrying the metadata that exists only
/// in that version.
///
/// `mode` lives inside [`FormatVersion::V1`] rather than beside it because v2
/// retired the field: concurrency is a property of how a plan is executed, not
/// of the document. Tagging it this way makes a modeless v2 plan the only
/// representable v2 plan, so no code path can read a mode from one, and no
/// caller can construct a v2 plan that carries one.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FormatVersion {
    /// Planning Document Format v1, which declares an execution mode.
    V1 {
        /// Execution mode, v1-only.
        mode: PlanMode,
    },
    /// Planning Document Format v2, which has no execution mode.
    V2,
}

impl FormatVersion {
    /// Numeric format version used by external adapters.
    #[must_use]
    pub const fn number(self) -> u32 {
        match self {
            Self::V1 { .. } => 1,
            Self::V2 => 2,
        }
    }

    /// Whether this version requires the two logs, the Execution Protocol, and
    /// the per-task fields a cold session needs in order to resume work.
    ///
    /// v1 gates them on `mode: multi`; v2 requires them unconditionally.
    #[must_use]
    pub const fn requires_handoff_record(self) -> bool {
        match self {
            Self::V1 { mode } => matches!(mode, PlanMode::Multi),
            Self::V2 => true,
        }
    }

    /// Whether this version requires an authored `files` mapping on every task.
    ///
    /// v1 `mode: multi` does. v2 removed the field, so nothing requires it, and
    /// a v2 task that declares one is rejected in the adapter.
    #[must_use]
    pub const fn requires_authored_files(self) -> bool {
        match self {
            Self::V1 { mode } => matches!(mode, PlanMode::Multi),
            Self::V2 => false,
        }
    }
}

/// Invalid domain identifier.
#[derive(Debug, Clone, Eq, Error, PartialEq)]
pub enum IdentifierError {
    /// A plan id was empty.
    #[error("plan id must not be empty")]
    EmptyPlanId,
    /// A task id did not match `T` followed by at least three digits.
    #[error("task id `{0}` must match T followed by at least three ASCII digits")]
    InvalidTaskId(String),
}

/// Stable plan identifier.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlanId(String);

impl PlanId {
    /// Construct a non-empty plan id.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError::EmptyPlanId`] for blank input.
    pub fn parse(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        if value.trim().is_empty() {
            Err(IdentifierError::EmptyPlanId)
        } else {
            Ok(Self(value))
        }
    }

    /// Borrow the identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PlanId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Stable task identifier.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskId(String);

impl TaskId {
    /// Construct a task id matching `T` followed by at least three ASCII digits.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError::InvalidTaskId`] for malformed input.
    pub fn parse(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        let valid = value.strip_prefix('T').is_some_and(|digits| {
            digits.len() >= 3 && digits.bytes().all(|byte| byte.is_ascii_digit())
        });
        if valid {
            Ok(Self(value))
        } else {
            Err(IdentifierError::InvalidTaskId(value))
        }
    }

    /// Borrow the identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Plan lifecycle status.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PlanStatus {
    /// The plan is being authored.
    Draft,
    /// The operator approved implementation.
    Approved,
    /// Implementation is complete.
    Implemented,
    /// The plan will not be implemented.
    Abandoned,
}

/// Execution mode. Declared by a v1 plan only; retired in v2.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PlanMode {
    /// One worker executes the plan.
    Single,
    /// Multiple workers may execute the task graph.
    Multi,
}

/// External source kind.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SourceType {
    /// Asana.
    Asana,
    /// GitHub.
    Github,
    /// GitLab.
    Gitlab,
    /// Linear.
    Linear,
    /// Manually authored.
    Manual,
    /// Another source.
    Other,
}

/// Problem severity.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Severity {
    /// Unspecified severity.
    Unknown,
    /// Low severity.
    Low,
    /// Medium severity.
    Medium,
    /// High severity.
    High,
    /// Critical severity.
    Critical,
}

/// Aggregate task-graph state.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TaskGraphStatus {
    /// The graph is being authored.
    Draft,
    /// The graph is ready.
    Ready,
    /// The graph is executing.
    Executing,
    /// The graph is complete.
    Complete,
}

/// Task lifecycle status.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TaskStatus {
    /// Work has not started.
    NotStarted,
    /// Dependencies are complete.
    Ready,
    /// Work is underway.
    InProgress,
    /// Work is blocked.
    Blocked,
    /// Work is complete.
    Done,
    /// Work was abandoned.
    Abandoned,
}

/// Hidden-criterion criticality.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Criticality {
    /// Required for acceptance.
    Must,
    /// Strongly preferred.
    Should,
    /// Optional improvement.
    Nice,
}

/// Criterion evaluator kind.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Evaluator {
    /// A command evaluates the claim.
    Automated,
    /// An agent evaluates the claim.
    AgentEvaluated,
    /// A person evaluates the claim.
    HumanJudgment,
}

/// Metadata for the source from which a plan was created.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SourceMetadata {
    /// Source kind.
    pub kind: SourceType,
    /// Source URL.
    pub url: Option<String>,
    /// Source-system identifier.
    pub external_id: Option<String>,
    /// Import timestamp.
    pub imported_at: Option<String>,
}

/// Metadata describing the motivating problem.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BugMetadata {
    /// Problem summary.
    pub summary: String,
    /// Problem severity.
    pub severity: Severity,
    /// Affected area.
    pub affected_area: Option<String>,
    /// User impact.
    pub user_impact: Option<String>,
}

/// Execution policy.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ExecutionMetadata {
    /// Whether implementation requires explicit operator approval.
    pub requires_operator_approval_before_implementation: bool,
    /// Whether execution must update the plan.
    pub requires_plan_updates_during_execution: bool,
    /// Aggregate graph status.
    pub task_graph_status: TaskGraphStatus,
}

/// Plan metadata independent of any file representation.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PlanMetadata {
    /// Domain format version, carrying the v1-only execution mode.
    pub format_version: FormatVersion,
    /// Stable plan id.
    pub id: PlanId,
    /// Human-readable title.
    pub title: String,
    /// Lifecycle status.
    pub status: PlanStatus,
    /// Creation date.
    pub created_at: String,
    /// Last update date.
    pub updated_at: String,
    /// Owning person or team.
    pub owner: String,
    /// Source metadata.
    pub source: SourceMetadata,
    /// Problem metadata.
    pub bug: BugMetadata,
    /// Execution policy.
    pub execution: ExecutionMetadata,
    /// The fleet project this plan belongs to, v2 only. `None` for every v1
    /// plan and for a v2 plan that declares no project: v1 never modeled the
    /// key, so a v1 document that happens to carry one has it silently
    /// dropped by the first mutation, exactly as any other unmodeled v1 key
    /// is today.
    pub project: Option<ProjectIdentity>,
}

/// A plan's mapping onto the fleet's shared project identity
/// (`tftio_lib::project`), v2 only.
///
/// `slug` and `remote`, when present, are already validated against the
/// fleet's shared grammar and remote-normalization rules — this crate never
/// restates either rule, it only calls `tftio_lib::project`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProjectIdentity {
    /// The project's fleet-wide slug.
    pub slug: Slug,
    /// The project's normalized remote, when the plan records one.
    pub remote: Option<NormalizedRemote>,
}

/// ADR prose extracted from or rendered into a format adapter.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Adr {
    /// ADR heading title.
    pub title: String,
    /// Problem statement.
    pub problem_statement: String,
    /// Ticket section.
    pub ticket: String,
    /// Discussion summary.
    pub discussion_summary: String,
    /// Context.
    pub context: String,
    /// Constraints.
    pub constraints: String,
    /// Non-goals.
    pub non_goals: String,
    /// Decision.
    pub decision: String,
    /// Alternatives considered.
    pub alternatives_considered: String,
    /// Consequences.
    pub consequences: String,
}

/// Files associated with a task.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct TaskFiles {
    /// Files expected to be read.
    pub likely_read: Vec<String>,
    /// Files expected to be modified.
    pub likely_modify: Vec<String>,
}

/// The judgment recorded against a hidden criterion's verdict.
///
/// Optional on [`HiddenCriterion`]: absent until a judge (or later, an
/// operator) records a verdict. Once present, `Undetermined` structurally
/// pairs with a non-empty [`HiddenCriterion::evidence_needed`] — enforced by
/// [`crate::validate::validate_markdown`], not by this type alone, so the
/// same check applies whether the value was just constructed or was parsed
/// back out of a plan document.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HiddenVerdictJudgment {
    /// The criterion was met.
    Pass,
    /// The criterion was not met.
    Fail,
    /// The available evidence could not settle the criterion.
    Undetermined,
}

/// A short evidence record attached to a hidden criterion's verdict, with a
/// provenance stamp naming who or what produced it.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HiddenEvidenceRecord {
    /// A human-readable summary of what this evidence shows.
    pub summary: String,
    /// The provenance stamp: `tool_authored`, `judge`, `worker_narrated`, or
    /// `operator_observed`. Kept as an opaque string here — this crate does
    /// not depend on any consumer's provenance type — and validated
    /// against that closed set by [`crate::validate::validate_markdown`].
    pub provenance: String,
}

/// Criterion concealed from a worker.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HiddenCriterion {
    /// Property asserted by the criterion.
    pub claim: String,
    /// Acceptance criticality.
    pub criticality: Criticality,
    /// Evaluator kind.
    pub evaluator: Evaluator,
    /// Automated command specification.
    pub check: Option<String>,
    /// Human or agent question.
    pub ask: Option<String>,
    /// Reason concealment improves signal.
    pub why_hidden: String,
    /// Predicted behavior if visible.
    pub counterfactual: String,
    /// The verdict recorded against this criterion, if any. Absent until a
    /// judge (or the operator) records one.
    pub verdict: Option<HiddenVerdictJudgment>,
    /// The rationale behind `verdict`. Required whenever `verdict` is
    /// present.
    pub rationale: Option<String>,
    /// What would resolve the judgment. Required when, and only permitted
    /// when, `verdict` is [`HiddenVerdictJudgment::Undetermined`].
    pub evidence_needed: Option<String>,
    /// Evidence gathered in service of this criterion's verdict, with
    /// provenance.
    pub evidence: Vec<HiddenEvidenceRecord>,
}

/// Operator-side task including concealed criteria.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OperatorTask {
    /// Stable task id.
    pub id: TaskId,
    /// Human-readable title.
    pub title: String,
    /// Lifecycle status.
    pub status: TaskStatus,
    /// Assigned owner.
    pub owner: Option<String>,
    /// Dependency task ids.
    pub depends_on: Vec<TaskId>,
    /// Tasks explicitly blocked by this task, as authored in a v1 document.
    /// Always empty for a v2 plan, which never reads the field: use
    /// [`OperatorPlan::derived_blocks`] instead.
    pub blocks: Vec<TaskId>,
    /// Task description.
    pub description: String,
    /// Concrete work items.
    pub work_items: Vec<String>,
    /// Preserved invariants.
    pub invariants: Vec<String>,
    /// Visible acceptance checks.
    pub acceptance_checks: Vec<String>,
    /// Concealed acceptance criteria.
    pub hidden_criteria: Vec<HiddenCriterion>,
    /// Expected file access.
    pub files: Option<TaskFiles>,
    /// Completion evidence.
    pub completion_evidence: Option<String>,
}

/// A complete operator-side planning document.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OperatorPlan {
    /// Plan metadata.
    pub metadata: PlanMetadata,
    /// ADR content.
    pub adr: Adr,
    /// Operator-guidance history content.
    pub operator_guidance_log: Option<String>,
    /// Decision history content.
    pub decision_log: Option<String>,
    /// Ordered task graph.
    pub tasks: Vec<OperatorTask>,
    /// Execution protocol content.
    pub execution_protocol: Option<String>,
}

impl OperatorPlan {
    /// Each task's downstream set, computed as the exact inverse of
    /// `depends_on` and never read from the document.
    ///
    /// Every task id in the graph is a key, mapping to the ids of the tasks
    /// that depend on it in document order.
    #[must_use]
    pub fn derived_blocks(&self) -> BTreeMap<TaskId, Vec<TaskId>> {
        derived_blocks(
            self.tasks
                .iter()
                .map(|task| (&task.id, task.depends_on.as_slice())),
        )
    }
}

/// Worker-side task with no hidden-criterion field.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkerTask {
    /// Stable task id.
    pub id: TaskId,
    /// Human-readable title.
    pub title: String,
    /// Lifecycle status.
    pub status: TaskStatus,
    /// Assigned owner.
    pub owner: Option<String>,
    /// Dependency task ids.
    pub depends_on: Vec<TaskId>,
    /// Tasks explicitly blocked by this task, as authored in a v1 document.
    /// Always empty for a v2 plan, which never reads the field: use
    /// [`WorkerPlan::derived_blocks`] instead.
    pub blocks: Vec<TaskId>,
    /// Task description.
    pub description: String,
    /// Concrete work items.
    pub work_items: Vec<String>,
    /// Preserved invariants.
    pub invariants: Vec<String>,
    /// Visible acceptance checks.
    pub acceptance_checks: Vec<String>,
    /// Expected file access.
    pub files: Option<TaskFiles>,
    /// Completion evidence.
    pub completion_evidence: Option<String>,
}

/// Complete worker view, with concealed criteria absent by construction.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkerPlan {
    /// Plan metadata.
    pub metadata: PlanMetadata,
    /// ADR content.
    pub adr: Adr,
    /// Ordered worker-safe tasks.
    pub tasks: Vec<WorkerTask>,
    /// Execution protocol content.
    pub execution_protocol: Option<String>,
}

impl WorkerPlan {
    /// Each task's downstream set, computed as the exact inverse of
    /// `depends_on`. See [`OperatorPlan::derived_blocks`].
    #[must_use]
    pub fn derived_blocks(&self) -> BTreeMap<TaskId, Vec<TaskId>> {
        derived_blocks(
            self.tasks
                .iter()
                .map(|task| (&task.id, task.depends_on.as_slice())),
        )
    }
}

fn derived_blocks<'a>(
    tasks: impl Iterator<Item = (&'a TaskId, &'a [TaskId])> + Clone,
) -> BTreeMap<TaskId, Vec<TaskId>> {
    let mut downstream = tasks
        .clone()
        .map(|(id, _)| (id.clone(), Vec::new()))
        .collect::<BTreeMap<_, Vec<TaskId>>>();
    for (id, depends_on) in tasks {
        for dependency in depends_on {
            if let Some(entry) = downstream.get_mut(dependency) {
                entry.push(id.clone());
            }
        }
    }
    downstream
}

impl From<&OperatorPlan> for WorkerPlan {
    fn from(plan: &OperatorPlan) -> Self {
        Self {
            metadata: plan.metadata.clone(),
            adr: plan.adr.clone(),
            tasks: plan
                .tasks
                .iter()
                .map(|task| WorkerTask {
                    id: task.id.clone(),
                    title: task.title.clone(),
                    status: task.status,
                    owner: task.owner.clone(),
                    depends_on: task.depends_on.clone(),
                    blocks: task.blocks.clone(),
                    description: task.description.clone(),
                    work_items: task.work_items.clone(),
                    invariants: task.invariants.clone(),
                    acceptance_checks: task.acceptance_checks.clone(),
                    files: task.files.clone(),
                    completion_evidence: task.completion_evidence.clone(),
                })
                .collect(),
            execution_protocol: plan.execution_protocol.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IdentifierError, PlanId, TaskId};

    #[test]
    fn identifiers_enforce_domain_shapes() {
        assert_eq!(PlanId::parse(" "), Err(IdentifierError::EmptyPlanId));
        assert_eq!(
            TaskId::parse("bad"),
            Err(IdentifierError::InvalidTaskId("bad".to_owned()))
        );
        assert_eq!(
            PlanId::parse("PLAN-1").as_ref().map(PlanId::as_str),
            Ok("PLAN-1")
        );
        assert_eq!(
            TaskId::parse("T001").as_ref().map(TaskId::as_str),
            Ok("T001")
        );
    }
}
