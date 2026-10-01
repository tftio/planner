//! Explicit Markdown/YAML adapter for the pure planning-domain model.

use std::collections::BTreeSet;
use std::fmt::{self, Write as _};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_yaml_ng::Value as YamlValue;
use tftio_lib::project::{NormalizedRemote, Slug, normalize_remote};

use crate::diagnostic::{Diagnostic, DiagnosticCode, SourceLocation, SourceSpan};
use crate::model::{
    Adr, BugMetadata, Criticality, Evaluator, ExecutionMetadata, FormatVersion, HiddenCriterion,
    HiddenEvidenceRecord, HiddenVerdictJudgment, IdentifierError, OperatorPlan, OperatorTask,
    PlanId, PlanMetadata, PlanMode, PlanStatus, ProjectIdentity, Severity, SourceMetadata,
    SourceType, TaskFiles, TaskGraphStatus, TaskId, TaskStatus,
};

const FRONTMATTER_DELIMITER: &str = "---\n";
const TASK_GRAPH_BEGIN: &str = "<!-- TASK_GRAPH:BEGIN -->";
const TASK_GRAPH_END: &str = "<!-- TASK_GRAPH:END -->";

/// A field v2 removed is rejected rather than ignored, and the diagnostic names
/// the version and says the field was removed, so an author who wrote it out of
/// v1 habit is told what happened rather than that an unknown key appeared.
/// These mirror `MODE_REMOVED`, `BLOCKS_REMOVED`, `FILES_REMOVED` and
/// `_worker_projection_removed` in the `check-plan` validator.
const MODE_REMOVED: &str = "frontmatter declares 'mode', which Planning Document Format v2 removed. \
Concurrency is a property of how a plan is executed, not of the document; v2 has one shape. \
Delete the key.";
const BLOCKS_REMOVED: &str = "declares 'blocks', which Planning Document Format v2 removed. \
It is the exact inverse of depends_on and is derived on projection, never authored. Delete it.";
const FILES_REMOVED: &str = "declares 'files', which Planning Document Format v2 removed. \
It asked for a forecast of the files the task would touch; v2 requires the completion evidence \
to name the files it actually touched instead. Delete it.";

/// The worker-projection keys, in the order `check-plan` rejects them. v1
/// authors all four; v2 has none of them.
const WORKER_PROJECTION_FIELDS: [&str; 4] = ["seed", "allow", "guest_class", "seed_includes_plans"];

/// The `check-plan` diagnostic for one rejected worker-projection field, whose
/// wording the v2 spec mandates. One message per field rather than one for the
/// set, so the diagnostic names the key the author actually wrote.
fn worker_projection_removed(field: &str) -> String {
    format!(
        "declares '{field}', which Planning Document Format v2 removed. v2 has no worker \
projection: the projection strips hidden_criteria unconditionally and no field widens what a \
worker may see. Delete it."
    )
}

/// One or more Markdown-adapter diagnostics.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ParseError {
    /// Stable diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(first) = self.diagnostics.first() else {
            return formatter.write_str("planning document parse failed without a diagnostic");
        };
        write!(formatter, "{}: {}", first.code.as_str(), first.message)
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    fn one(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }
}

/// Failure to render adapter-owned YAML DTOs.
pub type RenderError = serde_yaml_ng::Error;

#[derive(Debug, Clone)]
struct Heading {
    level: u8,
    text: String,
    content_start: usize,
    section_start: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawFrontmatter {
    plan_format_version: u32,
    plan_id: String,
    title: String,
    status: String,
    /// Present in v1, where it is required; rejected in v2. `Option` here
    /// records key presence so the converter can enforce the version's rule
    /// rather than serde enforcing v1's on both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    created_at: String,
    updated_at: String,
    owner: String,
    source: RawSource,
    bug: RawBug,
    execution: RawExecution,
    /// The fleet project mapping, v2 only. Absent (rather than present and
    /// null) both when a v1 document carries no key and when a v2 document
    /// declares no project: `PlanMetadata::project` collapses "absent" and
    /// "null" to `None` regardless of version, so nothing here needs to keep
    /// them apart the way [`DeclaredText`] does for `owner`. `convert_metadata`
    /// reads this only under `FormatVersion::V2`; a v1 document that happens
    /// to carry the key has it parsed and then ignored, exactly as any other
    /// unmodeled v1 key is today. The static `PLAN_TEMPLATE.md` resource, not
    /// this renderer, is what spells an unset project `project: null` for an
    /// author to fill in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project: Option<RawProjectIdentity>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct RawProjectIdentity {
    slug: String,
    #[serde(default)]
    remote: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawSource {
    #[serde(rename = "type")]
    kind: String,
    url: Option<String>,
    external_id: Option<String>,
    imported_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawBug {
    summary: String,
    severity: String,
    affected_area: Option<String>,
    user_impact: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawExecution {
    requires_operator_approval_before_implementation: bool,
    requires_plan_updates_during_execution: bool,
    task_graph_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawTaskGraph {
    tasks: Vec<RawTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawTask {
    id: String,
    title: String,
    status: String,
    #[serde(default, skip_serializing_if = "DeclaredText::is_absent")]
    owner: DeclaredText,
    #[serde(default)]
    depends_on: Vec<String>,
    /// Authored in v1; rejected in v2, where a task's downstream set is derived
    /// from `depends_on`. `Option` distinguishes an authored empty list from an
    /// absent key so v2 rejects both spellings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    blocks: Option<Vec<String>>,
    description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    work_items: Vec<YamlValue>,
    invariants: Vec<YamlValue>,
    acceptance_checks: Vec<YamlValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hidden_criteria: Vec<RawHiddenCriterion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    files: Option<RawTaskFiles>,
    /// Worker projection, authored in v1 and rejected in v2, which has no such
    /// block. v1 never read these through this model and still does not: they
    /// are deserialized for their presence alone, and `skip_serializing` keeps
    /// them out of a rewritten document exactly as their absence from this
    /// struct did before v2 needed to see them.
    #[serde(default, skip_serializing)]
    seed: Option<YamlValue>,
    #[serde(default, skip_serializing)]
    allow: Option<YamlValue>,
    #[serde(default, skip_serializing)]
    guest_class: Option<YamlValue>,
    #[serde(default, skip_serializing)]
    seed_includes_plans: Option<YamlValue>,
    #[serde(default, skip_serializing_if = "DeclaredText::is_absent")]
    completion_evidence: DeclaredText,
}

/// A task key whose *presence* carries meaning independently of its value.
///
/// v2 requires `owner` and `completion_evidence` to be present and permits
/// either to be null, so the three states — absent, present-and-null,
/// present-with-a-value — are all distinct. `Option<String>` collapses the
/// first two, and a nested `Option` cannot tell them apart either: the outer
/// layer consumes the null. Naming the states is what keeps the v2 rule
/// enforceable and the v1 rule, which requires neither key, unchanged.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
enum DeclaredText {
    /// The key was absent from the document.
    #[default]
    Absent,
    /// The key was present, carrying either a string or null.
    Declared(Option<String>),
}

impl DeclaredText {
    const fn is_absent(&self) -> bool {
        matches!(*self, Self::Absent)
    }

    /// The declared value, flattening an absent key and a null value, which
    /// the domain model does not distinguish.
    fn into_value(self) -> Option<String> {
        match self {
            Self::Absent => None,
            Self::Declared(value) => value,
        }
    }
}

impl Serialize for DeclaredText {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *self {
            Self::Absent => serializer.serialize_none(),
            Self::Declared(ref value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for DeclaredText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer).map(Self::Declared)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawTaskFiles {
    /// Both keys are optional in v1, the only version that reads this mapping;
    /// v2 rejects the `files` key before either is looked at.
    #[serde(default)]
    likely_read: Option<Vec<String>>,
    #[serde(default)]
    likely_modify: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawHiddenCriterion {
    claim: String,
    criticality: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evaluator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    check: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ask: Option<String>,
    why_hidden: String,
    counterfactual: String,
    /// Optional verdict fields, written by a supervising ledger through
    /// `tftio_planner`'s mutation path and never authored by an agent, so a
    /// worker cannot grade its own hidden criteria.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verdict: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rationale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evidence_needed: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    evidence: Vec<RawHiddenEvidenceRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawHiddenEvidenceRecord {
    summary: String,
    provenance: String,
}

/// Parse Markdown/YAML into the pure operator-side model.
///
/// # Errors
///
/// Returns stable diagnostics when required regions, YAML, or typed values are invalid.
pub fn parse_markdown(source: &str) -> Result<OperatorPlan, ParseError> {
    if let Some(offset) = source.find('\r') {
        return Err(error_at(
            source,
            DiagnosticCode::NonCanonicalLineEnding,
            "planning documents must use LF line endings",
            SourceSpan::new(offset, offset.saturating_add(1)),
        ));
    }
    let Some(after_open) = source.strip_prefix(FRONTMATTER_DELIMITER) else {
        return Err(error_at(
            source,
            DiagnosticCode::MissingFrontmatter,
            "file must start with a YAML frontmatter delimiter",
            SourceSpan::new(0, 0),
        ));
    };
    let Some(frontmatter_end) = after_open.find("\n---\n") else {
        return Err(error_at(
            source,
            DiagnosticCode::UnterminatedFrontmatter,
            "frontmatter has no closing delimiter",
            SourceSpan::new(0, source.len()),
        ));
    };
    let yaml_start = FRONTMATTER_DELIMITER.len();
    let yaml_end = yaml_start.saturating_add(frontmatter_end);
    let body_start = yaml_end.saturating_add("\n---\n".len());
    let raw_metadata: RawFrontmatter = decode_yaml(
        source,
        source.get(yaml_start..yaml_end).unwrap_or_default(),
        yaml_start,
    )?;
    let metadata = convert_metadata(raw_metadata, source)?;
    let body = source.get(body_start..).unwrap_or_default();
    let headings = parse_headings(body);
    let adr = parse_adr(source, body, body_start, &headings)?;
    let tasks = parse_tasks(source, metadata.format_version)?;
    let operator_guidance_log = optional_section(body, &headings, 2, "operator guidance log");
    let decision_log = optional_section(body, &headings, 2, "decision log");
    let execution_protocol = optional_section(body, &headings, 1, "execution protocol");

    Ok(OperatorPlan {
        metadata,
        adr,
        operator_guidance_log,
        decision_log,
        tasks,
        execution_protocol,
    })
}

fn parse_adr(
    source: &str,
    body: &str,
    body_start: usize,
    headings: &[Heading],
) -> Result<Adr, ParseError> {
    let adr_heading = find_heading(headings, 1, "adr")
        .ok_or_else(|| structure_error(source, body_start, "missing required '# ADR:' heading"))?;
    let adr_title = adr_heading
        .text
        .split_once(':')
        .map_or(adr_heading.text.as_str(), |(_, title)| title)
        .trim()
        .to_owned();
    Ok(Adr {
        title: adr_title,
        problem_statement: required_section(
            source,
            body,
            body_start,
            headings,
            2,
            "problem statement",
        )?,
        ticket: required_section(source, body, body_start, headings, 3, "ticket")?,
        discussion_summary: required_section(
            source,
            body,
            body_start,
            headings,
            3,
            "discussion summary",
        )?,
        context: required_section(source, body, body_start, headings, 2, "context")?,
        constraints: required_section(source, body, body_start, headings, 2, "constraints")?,
        non_goals: required_section(source, body, body_start, headings, 2, "non-goals")?,
        decision: required_section(source, body, body_start, headings, 2, "decision")?,
        alternatives_considered: required_section(
            source,
            body,
            body_start,
            headings,
            2,
            "alternatives considered",
        )?,
        consequences: required_section(source, body, body_start, headings, 2, "consequences")?,
    })
}

fn parse_tasks(source: &str, version: FormatVersion) -> Result<Vec<OperatorTask>, ParseError> {
    let graph_yaml = extract_task_graph(source)?;
    let raw_graph: RawTaskGraph = decode_yaml(source, graph_yaml.text, graph_yaml.offset)?;
    let tasks = raw_graph
        .tasks
        .into_iter()
        .map(|task| convert_task(task, version, source, graph_yaml.offset))
        .collect::<Result<Vec<_>, _>>()?;
    let mut task_ids = BTreeSet::new();
    for task in &tasks {
        if !task_ids.insert(task.id.clone()) {
            return Err(structure_error(
                source,
                graph_yaml.offset,
                format!("duplicate task id: {}", task.id),
            ));
        }
    }
    Ok(tasks)
}

/// Canonically render a pure operator-side model as Planning Document Format
/// Markdown, in the shape its declared version defines.
///
/// # Errors
///
/// Returns [`RenderError`] if adapter-owned YAML DTO serialization fails.
pub fn render_markdown(plan: &OperatorPlan) -> Result<String, RenderError> {
    let frontmatter = serde_yaml_ng::to_string(&raw_frontmatter(plan))?;
    let version = plan.metadata.format_version;
    let raw_graph = RawTaskGraph {
        tasks: plan
            .tasks
            .iter()
            .map(|task| raw_task(task, version))
            .collect(),
    };
    let graph = serde_yaml_ng::to_string(&raw_graph)?;
    let mut output = String::new();
    output.push_str("---\n");
    output.push_str(frontmatter.trim_end());
    output.push_str("\n---\n\n# ADR: ");
    output.push_str(&plan.adr.title);
    output.push_str("\n\n");
    render_section(
        &mut output,
        2,
        "Problem Statement",
        &plan.adr.problem_statement,
    );
    output.push_str("## Source Material\n\n");
    render_section(&mut output, 3, "Ticket", &plan.adr.ticket);
    render_section(
        &mut output,
        3,
        "Discussion Summary",
        &plan.adr.discussion_summary,
    );
    render_section(&mut output, 2, "Context", &plan.adr.context);
    render_section(&mut output, 2, "Constraints", &plan.adr.constraints);
    render_section(&mut output, 2, "Non-Goals", &plan.adr.non_goals);
    render_section(&mut output, 2, "Decision", &plan.adr.decision);
    render_section(
        &mut output,
        2,
        "Alternatives Considered",
        &plan.adr.alternatives_considered,
    );
    render_section(&mut output, 2, "Consequences", &plan.adr.consequences);
    if let Some(log) = &plan.operator_guidance_log {
        render_section(&mut output, 2, "Operator Guidance Log", log);
    }
    if let Some(log) = &plan.decision_log {
        render_section(&mut output, 2, "Decision Log", log);
    }
    output.push_str("# Task Graph\n\n");
    output.push_str(TASK_GRAPH_BEGIN);
    output.push_str("\n```yaml\n");
    output.push_str(graph.trim_end());
    output.push_str("\n```\n");
    output.push_str(TASK_GRAPH_END);
    output.push_str("\n\n# Task Details\n\n");
    for task in &plan.tasks {
        render_task_details(&mut output, task);
    }
    if let Some(protocol) = &plan.execution_protocol {
        render_section(&mut output, 1, "Execution Protocol", protocol);
    }
    Ok(output)
}

struct YamlSlice<'a> {
    text: &'a str,
    offset: usize,
}

fn extract_task_graph(source: &str) -> Result<YamlSlice<'_>, ParseError> {
    let Some(begin) = source.find(TASK_GRAPH_BEGIN) else {
        return Err(structure_error(
            source,
            0,
            "missing TASK_GRAPH:BEGIN marker",
        ));
    };
    let after_begin = begin.saturating_add(TASK_GRAPH_BEGIN.len());
    let remaining = source.get(after_begin..).unwrap_or_default();
    let Some(end_relative) = remaining.find(TASK_GRAPH_END) else {
        return Err(structure_error(
            source,
            begin,
            "missing TASK_GRAPH:END marker",
        ));
    };
    let end = after_begin.saturating_add(end_relative);
    let block = source.get(after_begin..end).unwrap_or_default();
    let Some(fence_relative) = block.find("```") else {
        return Err(missing_fence(source, after_begin, end));
    };
    let fence = after_begin.saturating_add(fence_relative);
    let Some(line_end_relative) = source.get(fence..end).unwrap_or_default().find('\n') else {
        return Err(missing_fence(source, fence, end));
    };
    let yaml_start = fence.saturating_add(line_end_relative).saturating_add(1);
    let fenced = source.get(yaml_start..end).unwrap_or_default();
    let Some(close_relative) = fenced.find("\n```") else {
        return Err(missing_fence(source, yaml_start, end));
    };
    let yaml_end = yaml_start.saturating_add(close_relative);
    Ok(YamlSlice {
        text: source.get(yaml_start..yaml_end).unwrap_or_default(),
        offset: yaml_start,
    })
}

fn parse_headings(body: &str) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut in_fence = false;
    let mut offset = 0_usize;
    for line_with_newline in body.split_inclusive('\n') {
        let line = line_with_newline
            .strip_suffix('\n')
            .unwrap_or(line_with_newline);
        if line.trim().starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence && let Some((level, text)) = parse_heading(line) {
            headings.push(Heading {
                level,
                text: text.to_owned(),
                content_start: offset.saturating_add(line_with_newline.len()),
                section_start: offset,
            });
        }
        offset = offset.saturating_add(line_with_newline.len());
    }
    headings
}

fn parse_heading(line: &str) -> Option<(u8, &str)> {
    let count = line.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&count) {
        return None;
    }
    let text = line.get(count..)?.strip_prefix(' ')?.trim();
    if text.is_empty() {
        return None;
    }
    u8::try_from(count).ok().map(|level| (level, text))
}

fn heading_matches(heading: &Heading, level: u8, name: &str) -> bool {
    if heading.level != level {
        return false;
    }
    let text = heading.text.to_lowercase();
    if name == "adr" {
        text == name || text.starts_with("adr:")
    } else {
        text == name
            || text.starts_with(&format!("{name} "))
            || text.starts_with(&format!("{name} —"))
            || text.starts_with(&format!("{name} -"))
    }
}

fn find_heading<'a>(headings: &'a [Heading], level: u8, name: &str) -> Option<&'a Heading> {
    headings
        .iter()
        .find(|heading| heading_matches(heading, level, name))
}

fn required_section(
    source: &str,
    body: &str,
    body_start: usize,
    headings: &[Heading],
    level: u8,
    name: &str,
) -> Result<String, ParseError> {
    optional_section(body, headings, level, name).ok_or_else(|| {
        structure_error(
            source,
            body_start,
            format!("missing or empty required heading: {name}"),
        )
    })
}

fn optional_section(body: &str, headings: &[Heading], level: u8, name: &str) -> Option<String> {
    let index = headings
        .iter()
        .position(|heading| heading_matches(heading, level, name))?;
    let heading = headings.get(index)?;
    let end = headings
        .iter()
        .skip(index.saturating_add(1))
        .find(|candidate| candidate.level <= heading.level)
        .map_or(body.len(), |candidate| candidate.section_start);
    let content = body.get(heading.content_start..end)?.trim();
    if content.is_empty() {
        None
    } else {
        Some(content.to_owned())
    }
}

fn decode_yaml<'a, T>(source: &str, yaml: &'a str, base: usize) -> Result<T, ParseError>
where
    T: Deserialize<'a>,
{
    serde_yaml_ng::from_str(yaml).map_err(|error| {
        let relative = error.location().map_or(0, |location| location.index());
        error_at(
            source,
            DiagnosticCode::InvalidYaml,
            error.to_string(),
            SourceSpan::new(base.saturating_add(relative), base.saturating_add(relative)),
        )
    })
}

fn convert_metadata(raw: RawFrontmatter, source: &str) -> Result<PlanMetadata, ParseError> {
    let format_version =
        convert_format_version(raw.plan_format_version, raw.mode.as_deref(), source)?;
    // v1 never modeled `project`; a v1 document that happens to carry the key
    // has it parsed above (so an unrelated malformed shape elsewhere in the
    // document is still reported precisely) and then ignored here, exactly as
    // any other unmodeled v1 key is today.
    let project = match format_version {
        FormatVersion::V1 { .. } => None,
        FormatVersion::V2 => convert_project(raw.project, source)?,
    };
    Ok(PlanMetadata {
        format_version,
        id: PlanId::parse(raw.plan_id).map_err(|error| identifier_error(source, &error))?,
        title: raw.title,
        status: parse_plan_status(source, &raw.status)?,
        created_at: raw.created_at,
        updated_at: raw.updated_at,
        owner: raw.owner,
        source: SourceMetadata {
            kind: parse_source_type(source, &raw.source.kind)?,
            url: raw.source.url,
            external_id: raw.source.external_id,
            imported_at: raw.source.imported_at,
        },
        bug: BugMetadata {
            summary: raw.bug.summary,
            severity: parse_severity(source, &raw.bug.severity)?,
            affected_area: raw.bug.affected_area,
            user_impact: raw.bug.user_impact,
        },
        execution: ExecutionMetadata {
            requires_operator_approval_before_implementation: raw
                .execution
                .requires_operator_approval_before_implementation,
            requires_plan_updates_during_execution: raw
                .execution
                .requires_plan_updates_during_execution,
            task_graph_status: parse_graph_status(source, &raw.execution.task_graph_status)?,
        },
        project,
    })
}

/// Convert an authored v2 `project` mapping, validating `slug` and `remote`
/// against the fleet's shared grammar and remote-normalization rules from
/// `tftio_lib::project` rather than restating either. A malformed slug or an
/// unnormalized remote is reported under [`DiagnosticCode::InvalidProject`],
/// the one diagnostic this task adds.
fn convert_project(
    raw: Option<RawProjectIdentity>,
    source: &str,
) -> Result<Option<ProjectIdentity>, ParseError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let slug = Slug::new(&raw.slug).map_err(|error| project_slug_error(source, &error))?;
    let remote = raw
        .remote
        .map(|value| convert_project_remote(source, &value))
        .transpose()?;
    Ok(Some(ProjectIdentity { slug, remote }))
}

/// Validate an authored `project.remote` against
/// `tftio_lib::project::normalize_remote`, requiring it to already be
/// normalized (`normalize_remote(value) == value`) rather than silently
/// normalizing it: frontmatter is meant to be read as written, and a remote
/// that round-trips to something else was not copied in canonical form.
fn convert_project_remote(source: &str, value: &str) -> Result<NormalizedRemote, ParseError> {
    let normalized =
        normalize_remote(value).map_err(|error| project_remote_error(source, error))?;
    if normalized.as_str() == value {
        return Ok(normalized);
    }
    Err(project_remote_not_normalized_error(
        source,
        value,
        &normalized,
    ))
}

fn project_slug_error(source: &str, error: &tftio_lib::project::SlugError) -> ParseError {
    project_error(source, format!("project.slug is invalid: {error}"))
}

fn project_remote_error(source: &str, error: tftio_lib::project::RemoteError) -> ParseError {
    project_error(source, format!("project.remote is invalid: {error}"))
}

fn project_remote_not_normalized_error(
    source: &str,
    value: &str,
    normalized: &NormalizedRemote,
) -> ParseError {
    project_error(
        source,
        format!("project.remote {value:?} unnormalized: expected {normalized}"),
    )
}

fn project_error(source: &str, message: impl Into<String>) -> ParseError {
    error_at(
        source,
        DiagnosticCode::InvalidProject,
        message,
        SourceSpan::new(0, 0),
    )
}

/// Resolve the declared version, enforcing the rules that decide which shape a
/// document is read as: v1 requires `mode`, v2 rejects it, and no other version
/// is recognised.
fn convert_format_version(
    version: u32,
    mode: Option<&str>,
    source: &str,
) -> Result<FormatVersion, ParseError> {
    match version {
        1 => {
            let Some(mode) = mode else {
                return Err(structure_error(
                    source,
                    0,
                    "frontmatter missing required key: mode",
                ));
            };
            Ok(FormatVersion::V1 {
                mode: parse_mode(source, mode)?,
            })
        }
        2 => {
            if mode.is_some() {
                return Err(structure_error(source, 0, MODE_REMOVED));
            }
            Ok(FormatVersion::V2)
        }
        _ => Err(structure_error(
            source,
            0,
            "plan_format_version must be 1 or 2",
        )),
    }
}

fn convert_task(
    raw: RawTask,
    version: FormatVersion,
    source: &str,
    offset: usize,
) -> Result<OperatorTask, ParseError> {
    let task_id =
        TaskId::parse(raw.id).map_err(|error| identifier_error_at(source, &error, offset))?;
    let depends_on = raw
        .depends_on
        .into_iter()
        .map(TaskId::parse)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| identifier_error_at(source, &error, offset))?;
    let blocks = convert_blocks(raw.blocks, version, &task_id, source, offset)?;
    let owner = version_scoped_key(raw.owner, version, &task_id, "owner", source, offset)?;
    let completion_evidence = version_scoped_key(
        raw.completion_evidence,
        version,
        &task_id,
        "completion_evidence",
        source,
        offset,
    )?;
    let files = convert_files(raw.files, version, &task_id, source, offset)?;
    reject_worker_projection(
        [
            &raw.seed,
            &raw.allow,
            &raw.guest_class,
            &raw.seed_includes_plans,
        ],
        version,
        &task_id,
        source,
        offset,
    )?;
    let hidden_criteria = raw
        .hidden_criteria
        .into_iter()
        .map(|criterion| convert_hidden(criterion, source))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OperatorTask {
        id: task_id,
        title: raw.title,
        status: parse_task_status(source, &raw.status)?,
        owner,
        depends_on,
        blocks,
        description: raw.description,
        work_items: raw.work_items.into_iter().map(yaml_text).collect(),
        invariants: raw.invariants.into_iter().map(yaml_text).collect(),
        acceptance_checks: raw.acceptance_checks.into_iter().map(yaml_text).collect(),
        hidden_criteria,
        files,
        completion_evidence,
    })
}

/// v1 reads `blocks` as authored. v2 rejects the key outright — including an
/// authored empty list — and leaves the field empty, because a v2 task's
/// downstream set is derived from `depends_on` by
/// [`crate::model::OperatorPlan::derived_blocks`].
fn convert_blocks(
    raw: Option<Vec<String>>,
    version: FormatVersion,
    task_id: &TaskId,
    source: &str,
    offset: usize,
) -> Result<Vec<TaskId>, ParseError> {
    match version {
        FormatVersion::V1 { .. } => raw
            .unwrap_or_default()
            .into_iter()
            .map(TaskId::parse)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| identifier_error_at(source, &error, offset)),
        FormatVersion::V2 => {
            if raw.is_some() {
                return Err(structure_error(
                    source,
                    offset,
                    format!("task {task_id} {BLOCKS_REMOVED}"),
                ));
            }
            Ok(Vec::new())
        }
    }
}

/// Collapse a key whose presence v2 requires and whose value either version
/// permits to be null. v1 never required the key, so an absent one stays absent.
fn version_scoped_key(
    raw: DeclaredText,
    version: FormatVersion,
    task_id: &TaskId,
    field: &str,
    source: &str,
    offset: usize,
) -> Result<Option<String>, ParseError> {
    if version == FormatVersion::V2 && raw.is_absent() {
        return Err(missing_task_field(source, offset, task_id, field));
    }
    Ok(raw.into_value())
}

/// v1 reads `files` as authored, defaulting the mapping and both of its lists.
/// v2 rejects the key outright: the field asked for a forecast of the files a
/// task would touch, and v2 requires the completion evidence to name the files
/// it actually touched instead.
fn convert_files(
    raw: Option<RawTaskFiles>,
    version: FormatVersion,
    task_id: &TaskId,
    source: &str,
    offset: usize,
) -> Result<Option<TaskFiles>, ParseError> {
    match version {
        FormatVersion::V1 { .. } => Ok(raw.map(|files| TaskFiles {
            likely_read: files.likely_read.unwrap_or_default(),
            likely_modify: files.likely_modify.unwrap_or_default(),
        })),
        FormatVersion::V2 => {
            if raw.is_some() {
                return Err(structure_error(
                    source,
                    offset,
                    format!("task {task_id} {FILES_REMOVED}"),
                ));
            }
            Ok(None)
        }
    }
}

/// v1 carries worker projection and ignores it here; v2 rejects each of the
/// four keys outright, because v2 has no worker projection at all. The
/// hidden-criteria exclusion that `seed` once guarded is a property of the
/// projection step in v2, so no authored path can widen what a worker sees.
///
/// The declarations arrive in the order the keys are rejected, so a task that
/// writes several of them is told about the first one — the same one
/// `check-plan` names first.
fn reject_worker_projection(
    declared: [&Option<YamlValue>; WORKER_PROJECTION_FIELDS.len()],
    version: FormatVersion,
    task_id: &TaskId,
    source: &str,
    offset: usize,
) -> Result<(), ParseError> {
    if version != FormatVersion::V2 {
        return Ok(());
    }
    for (value, field) in declared.into_iter().zip(WORKER_PROJECTION_FIELDS) {
        if value.is_some() {
            return Err(structure_error(
                source,
                offset,
                format!("task {task_id} {}", worker_projection_removed(field)),
            ));
        }
    }
    Ok(())
}

fn missing_task_field(source: &str, offset: usize, task_id: &TaskId, field: &str) -> ParseError {
    structure_error(
        source,
        offset,
        format!("task {task_id} missing required field: {field}"),
    )
}

fn convert_hidden(raw: RawHiddenCriterion, source: &str) -> Result<HiddenCriterion, ParseError> {
    let verdict = raw
        .verdict
        .as_deref()
        .map(|value| parse_hidden_verdict(source, value))
        .transpose()?;
    Ok(HiddenCriterion {
        claim: raw.claim,
        criticality: parse_criticality(source, &raw.criticality)?,
        evaluator: parse_evaluator(source, raw.evaluator.as_deref().unwrap_or("human_judgment"))?,
        check: raw.check,
        ask: raw.ask,
        why_hidden: raw.why_hidden,
        counterfactual: raw.counterfactual,
        verdict,
        rationale: raw.rationale,
        evidence_needed: raw.evidence_needed,
        evidence: raw
            .evidence
            .into_iter()
            .map(|entry| HiddenEvidenceRecord {
                summary: entry.summary,
                provenance: entry.provenance,
            })
            .collect(),
    })
}

fn parse_hidden_verdict(source: &str, value: &str) -> Result<HiddenVerdictJudgment, ParseError> {
    match value {
        "pass" => Ok(HiddenVerdictJudgment::Pass),
        "fail" => Ok(HiddenVerdictJudgment::Fail),
        "undetermined" => Ok(HiddenVerdictJudgment::Undetermined),
        _ => Err(invalid_value(source, "hidden criterion verdict", value)),
    }
}

fn parse_plan_status(source: &str, value: &str) -> Result<PlanStatus, ParseError> {
    match value {
        "draft" => Ok(PlanStatus::Draft),
        "approved" => Ok(PlanStatus::Approved),
        "implemented" => Ok(PlanStatus::Implemented),
        "abandoned" => Ok(PlanStatus::Abandoned),
        _ => Err(invalid_value(source, "plan status", value)),
    }
}

fn parse_mode(source: &str, value: &str) -> Result<PlanMode, ParseError> {
    match value {
        "single" => Ok(PlanMode::Single),
        "multi" => Ok(PlanMode::Multi),
        _ => Err(invalid_value(source, "plan mode", value)),
    }
}

fn parse_source_type(source: &str, value: &str) -> Result<SourceType, ParseError> {
    match value {
        "asana" => Ok(SourceType::Asana),
        "github" => Ok(SourceType::Github),
        "gitlab" => Ok(SourceType::Gitlab),
        "linear" => Ok(SourceType::Linear),
        "manual" => Ok(SourceType::Manual),
        "other" => Ok(SourceType::Other),
        _ => Err(invalid_value(source, "source type", value)),
    }
}

fn parse_severity(source: &str, value: &str) -> Result<Severity, ParseError> {
    match value {
        "unknown" => Ok(Severity::Unknown),
        "low" => Ok(Severity::Low),
        "medium" => Ok(Severity::Medium),
        "high" => Ok(Severity::High),
        "critical" => Ok(Severity::Critical),
        _ => Err(invalid_value(source, "severity", value)),
    }
}

fn parse_graph_status(source: &str, value: &str) -> Result<TaskGraphStatus, ParseError> {
    match value {
        "draft" => Ok(TaskGraphStatus::Draft),
        "ready" => Ok(TaskGraphStatus::Ready),
        "executing" => Ok(TaskGraphStatus::Executing),
        "complete" => Ok(TaskGraphStatus::Complete),
        _ => Err(invalid_value(source, "task graph status", value)),
    }
}

fn parse_task_status(source: &str, value: &str) -> Result<TaskStatus, ParseError> {
    match value {
        "not_started" => Ok(TaskStatus::NotStarted),
        "ready" => Ok(TaskStatus::Ready),
        "in_progress" => Ok(TaskStatus::InProgress),
        "blocked" => Ok(TaskStatus::Blocked),
        "done" => Ok(TaskStatus::Done),
        "abandoned" => Ok(TaskStatus::Abandoned),
        _ => Err(invalid_value(source, "task status", value)),
    }
}

fn parse_criticality(source: &str, value: &str) -> Result<Criticality, ParseError> {
    match value {
        "must" => Ok(Criticality::Must),
        "should" => Ok(Criticality::Should),
        "nice" => Ok(Criticality::Nice),
        _ => Err(invalid_value(source, "criticality", value)),
    }
}

fn parse_evaluator(source: &str, value: &str) -> Result<Evaluator, ParseError> {
    match value {
        "automated" => Ok(Evaluator::Automated),
        "agent_evaluated" => Ok(Evaluator::AgentEvaluated),
        "human_judgment" => Ok(Evaluator::HumanJudgment),
        _ => Err(invalid_value(source, "evaluator", value)),
    }
}

fn invalid_value(source: &str, kind: &str, value: &str) -> ParseError {
    structure_error(source, 0, format!("invalid {kind}: {value}"))
}

fn identifier_error(source: &str, error: &IdentifierError) -> ParseError {
    identifier_error_at(source, error, 0)
}

fn identifier_error_at(source: &str, error: &IdentifierError, offset: usize) -> ParseError {
    structure_error(source, offset, error.to_string())
}

fn raw_frontmatter(plan: &OperatorPlan) -> RawFrontmatter {
    RawFrontmatter {
        plan_format_version: plan.metadata.format_version.number(),
        plan_id: plan.metadata.id.to_string(),
        title: plan.metadata.title.clone(),
        status: plan_status_name(plan.metadata.status).to_owned(),
        mode: match plan.metadata.format_version {
            FormatVersion::V1 { mode } => Some(mode_name(mode).to_owned()),
            FormatVersion::V2 => None,
        },
        created_at: plan.metadata.created_at.clone(),
        updated_at: plan.metadata.updated_at.clone(),
        owner: plan.metadata.owner.clone(),
        source: RawSource {
            kind: source_type_name(plan.metadata.source.kind).to_owned(),
            url: plan.metadata.source.url.clone(),
            external_id: plan.metadata.source.external_id.clone(),
            imported_at: plan.metadata.source.imported_at.clone(),
        },
        bug: RawBug {
            summary: plan.metadata.bug.summary.clone(),
            severity: severity_name(plan.metadata.bug.severity).to_owned(),
            affected_area: plan.metadata.bug.affected_area.clone(),
            user_impact: plan.metadata.bug.user_impact.clone(),
        },
        execution: RawExecution {
            requires_operator_approval_before_implementation: plan
                .metadata
                .execution
                .requires_operator_approval_before_implementation,
            requires_plan_updates_during_execution: plan
                .metadata
                .execution
                .requires_plan_updates_during_execution,
            task_graph_status: graph_status_name(plan.metadata.execution.task_graph_status)
                .to_owned(),
        },
        project: match plan.metadata.format_version {
            FormatVersion::V1 { .. } => None,
            FormatVersion::V2 => plan
                .metadata
                .project
                .as_ref()
                .map(|project| RawProjectIdentity {
                    slug: project.slug.to_string(),
                    remote: project.remote.as_ref().map(ToString::to_string),
                }),
        },
    }
}

/// Render one task in its version's shape: v1 emits `blocks` and `files` when
/// authored and omits an absent `owner` or `completion_evidence`; v2 never emits
/// `blocks` or `files` and always emits the keys it requires, spelling an absent
/// value as `null`. Neither version emits worker projection: the four keys are
/// `skip_serializing`, so the `None`s below are what the struct requires rather
/// than a rendering choice.
fn raw_task(task: &OperatorTask, version: FormatVersion) -> RawTask {
    let v2 = version == FormatVersion::V2;
    RawTask {
        seed: None,
        allow: None,
        guest_class: None,
        seed_includes_plans: None,
        id: task.id.to_string(),
        title: task.title.clone(),
        status: task_status_name(task.status).to_owned(),
        owner: declared_when(v2, task.owner.clone()),
        depends_on: task.depends_on.iter().map(ToString::to_string).collect(),
        blocks: if v2 || task.blocks.is_empty() {
            None
        } else {
            Some(task.blocks.iter().map(ToString::to_string).collect())
        },
        description: task.description.clone(),
        work_items: task
            .work_items
            .iter()
            .cloned()
            .map(YamlValue::String)
            .collect(),
        invariants: task
            .invariants
            .iter()
            .cloned()
            .map(YamlValue::String)
            .collect(),
        acceptance_checks: task
            .acceptance_checks
            .iter()
            .cloned()
            .map(YamlValue::String)
            .collect(),
        hidden_criteria: task
            .hidden_criteria
            .iter()
            .map(|criterion| RawHiddenCriterion {
                claim: criterion.claim.clone(),
                criticality: criticality_name(criterion.criticality).to_owned(),
                evaluator: Some(evaluator_name(criterion.evaluator).to_owned()),
                check: criterion.check.clone(),
                ask: criterion.ask.clone(),
                why_hidden: criterion.why_hidden.clone(),
                counterfactual: criterion.counterfactual.clone(),
                verdict: criterion
                    .verdict
                    .map(hidden_verdict_name)
                    .map(str::to_owned),
                rationale: criterion.rationale.clone(),
                evidence_needed: criterion.evidence_needed.clone(),
                evidence: criterion
                    .evidence
                    .iter()
                    .map(|entry| RawHiddenEvidenceRecord {
                        summary: entry.summary.clone(),
                        provenance: entry.provenance.clone(),
                    })
                    .collect(),
            })
            .collect(),
        files: if v2 {
            None
        } else {
            task.files.as_ref().map(|files| RawTaskFiles {
                likely_read: Some(files.likely_read.clone()),
                likely_modify: Some(files.likely_modify.clone()),
            })
        },
        completion_evidence: declared_when(v2, task.completion_evidence.clone()),
    }
}

/// v2 renders the keys it requires even when their value is absent, spelling
/// that as `null`; v1, which requires neither, omits the key entirely.
fn declared_when(required: bool, value: Option<String>) -> DeclaredText {
    if required || value.is_some() {
        DeclaredText::Declared(value)
    } else {
        DeclaredText::Absent
    }
}

fn yaml_text(value: YamlValue) -> String {
    match value {
        YamlValue::Null => "null".to_owned(),
        YamlValue::Bool(value) => value.to_string(),
        YamlValue::Number(value) => value.to_string(),
        YamlValue::String(value) => value,
        YamlValue::Sequence(values) => format!(
            "[{}]",
            values
                .into_iter()
                .map(yaml_text)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        YamlValue::Mapping(values) => values
            .into_iter()
            .map(|(key, value)| format!("{}: {}", yaml_text(key), yaml_text(value)))
            .collect::<Vec<_>>()
            .join(", "),
        YamlValue::Tagged(value) => yaml_text(value.value),
    }
}

fn render_section(output: &mut String, level: u8, title: &str, body: &str) {
    let _ = writeln!(output, "{} {title}\n", "#".repeat(usize::from(level)));
    output.push_str(body.trim());
    output.push_str("\n\n");
}

fn render_task_details(output: &mut String, task: &OperatorTask) {
    let _ = writeln!(output, "## {} — {}\n", task.id, task.title);
    let _ = writeln!(output, "Status: `{}`\n", task_status_name(task.status));
    let dependencies = if task.depends_on.is_empty() {
        "none".to_owned()
    } else {
        task.depends_on
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let _ = writeln!(output, "Depends on: {dependencies}\n");
    if !task.blocks.is_empty() {
        let blocks = task
            .blocks
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(output, "Blocks: {blocks}\n");
    }
    render_section(output, 3, "Description", &task.description);
    render_list_section(output, "Invariants", &task.invariants);
    render_list_section(output, "Acceptance Checks", &task.acceptance_checks);
    render_section(
        output,
        3,
        "Completion Evidence",
        task.completion_evidence.as_deref().unwrap_or("Pending."),
    );
}

fn render_list_section(output: &mut String, title: &str, entries: &[String]) {
    let _ = writeln!(output, "### {title}\n");
    for entry in entries {
        let _ = writeln!(output, "- {}", entry.trim());
    }
    output.push('\n');
}

const fn plan_status_name(value: PlanStatus) -> &'static str {
    match value {
        PlanStatus::Draft => "draft",
        PlanStatus::Approved => "approved",
        PlanStatus::Implemented => "implemented",
        PlanStatus::Abandoned => "abandoned",
    }
}

const fn mode_name(value: PlanMode) -> &'static str {
    match value {
        PlanMode::Single => "single",
        PlanMode::Multi => "multi",
    }
}

const fn source_type_name(value: SourceType) -> &'static str {
    match value {
        SourceType::Asana => "asana",
        SourceType::Github => "github",
        SourceType::Gitlab => "gitlab",
        SourceType::Linear => "linear",
        SourceType::Manual => "manual",
        SourceType::Other => "other",
    }
}

const fn severity_name(value: Severity) -> &'static str {
    match value {
        Severity::Unknown => "unknown",
        Severity::Low => "low",
        Severity::Medium => "medium",
        Severity::High => "high",
        Severity::Critical => "critical",
    }
}

const fn graph_status_name(value: TaskGraphStatus) -> &'static str {
    match value {
        TaskGraphStatus::Draft => "draft",
        TaskGraphStatus::Ready => "ready",
        TaskGraphStatus::Executing => "executing",
        TaskGraphStatus::Complete => "complete",
    }
}

const fn task_status_name(value: TaskStatus) -> &'static str {
    match value {
        TaskStatus::NotStarted => "not_started",
        TaskStatus::Ready => "ready",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Done => "done",
        TaskStatus::Abandoned => "abandoned",
    }
}

const fn criticality_name(value: Criticality) -> &'static str {
    match value {
        Criticality::Must => "must",
        Criticality::Should => "should",
        Criticality::Nice => "nice",
    }
}

const fn evaluator_name(value: Evaluator) -> &'static str {
    match value {
        Evaluator::Automated => "automated",
        Evaluator::AgentEvaluated => "agent_evaluated",
        Evaluator::HumanJudgment => "human_judgment",
    }
}

const fn hidden_verdict_name(value: HiddenVerdictJudgment) -> &'static str {
    match value {
        HiddenVerdictJudgment::Pass => "pass",
        HiddenVerdictJudgment::Fail => "fail",
        HiddenVerdictJudgment::Undetermined => "undetermined",
    }
}

fn missing_fence(source: &str, start: usize, end: usize) -> ParseError {
    error_at(
        source,
        DiagnosticCode::MissingTaskGraphFence,
        "task graph markers must contain a fenced YAML block",
        SourceSpan::new(start, end),
    )
}

fn structure_error(source: &str, offset: usize, message: impl Into<String>) -> ParseError {
    error_at(
        source,
        DiagnosticCode::InvalidTaskGraphMarkers,
        message,
        SourceSpan::new(offset, offset),
    )
}

fn error_at(
    source: &str,
    code: DiagnosticCode,
    message: impl Into<String>,
    span: SourceSpan,
) -> ParseError {
    ParseError::one(Diagnostic::new(code, message, Some(location(source, span))))
}

fn location(source: &str, span: SourceSpan) -> SourceLocation {
    let prefix = source.get(..span.start).unwrap_or(source);
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix
        .rfind('\n')
        .map_or(0, |offset| offset.saturating_add(1));
    let column = source
        .get(line_start..span.start)
        .map_or(1, |line_prefix| line_prefix.chars().count() + 1);
    SourceLocation { line, column, span }
}

#[cfg(test)]
mod tests {
    use super::{
        Heading, ParseError, TASK_GRAPH_BEGIN, TASK_GRAPH_END, extract_task_graph, heading_matches,
        parse_adr, parse_criticality, parse_evaluator, parse_graph_status, parse_heading,
        parse_markdown, parse_mode, parse_plan_status, parse_severity, parse_source_type,
        parse_task_status, render_markdown,
    };
    use crate::model::{
        Criticality, Evaluator, FormatVersion, PlanMode, PlanStatus, Severity, SourceType,
        TaskGraphStatus, TaskStatus,
    };

    const VALID: &str = include_str!("../tests/fixtures/valid-single.md");

    #[test]
    fn declared_text_keeps_absence_and_null_apart() {
        // The three states must survive a round trip through YAML, because the
        // v2 rule that `owner` and `completion_evidence` be *present* and may be
        // null is only enforceable while absence and null stay distinct.
        let absent = serde_yaml_ng::from_str::<super::DeclaredText>("~")
            .map(|_| ())
            .is_ok();
        assert!(absent);
        assert_eq!(
            serde_yaml_ng::from_str::<super::DeclaredText>("null").ok(),
            Some(super::DeclaredText::Declared(None))
        );
        assert_eq!(
            serde_yaml_ng::from_str::<super::DeclaredText>("jfb").ok(),
            Some(super::DeclaredText::Declared(Some("jfb".to_owned())))
        );
        assert!(super::DeclaredText::default().is_absent());
        assert_eq!(super::DeclaredText::default().into_value(), None);
        // Serialized outside a struct that skips it, an absent key is null:
        // the only spelling that cannot be mistaken for a value.
        assert_eq!(
            serde_yaml_ng::to_string(&super::DeclaredText::Absent).ok(),
            Some("null\n".to_owned())
        );
        assert_eq!(
            serde_yaml_ng::to_string(&super::DeclaredText::Declared(Some("jfb".to_owned()))).ok(),
            Some("jfb\n".to_owned())
        );
    }

    #[test]
    fn convert_project_reports_a_malformed_slug_and_an_unnormalizable_remote() {
        let bad_slug = super::RawProjectIdentity {
            slug: "BAD".to_owned(),
            remote: None,
        };
        let slug_error = super::convert_project(Some(bad_slug), VALID)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(
            slug_error.contains("project.slug is invalid"),
            "{slug_error}"
        );

        let unnormalizable_remote = super::RawProjectIdentity {
            slug: "kb".to_owned(),
            remote: Some(String::new()),
        };
        let remote_error = super::convert_project(Some(unnormalizable_remote), VALID)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(
            remote_error.contains("project.remote is invalid"),
            "{remote_error}"
        );
    }

    #[test]
    fn yaml_list_values_have_deterministic_text_projection() {
        let values = serde_yaml_ng::from_str::<Vec<super::YamlValue>>(
            "- null\n- true\n- 42\n- text\n- [one, two]\n- {key: value}\n- !tag tagged\n",
        );
        assert!(values.is_ok());
        let values = values.unwrap_or_default();
        let projected = values.into_iter().map(super::yaml_text).collect::<Vec<_>>();

        assert_eq!(
            projected,
            [
                "null",
                "true",
                "42",
                "text",
                "[one, two]",
                "key: value",
                "tagged",
            ]
        );
    }

    fn round_trip(plan: &crate::model::OperatorPlan) -> Result<(), Box<dyn std::error::Error>> {
        let rendered = render_markdown(plan)?;
        let reparsed = parse_markdown(&rendered)?;
        assert_eq!(&reparsed, plan);
        Ok(())
    }

    #[test]
    fn parse_error_display_handles_empty_and_populated_diagnostics() {
        let empty = ParseError {
            diagnostics: Vec::new(),
        };
        assert_eq!(
            empty.to_string(),
            "planning document parse failed without a diagnostic"
        );
        let populated = parse_markdown("").err().map(|error| error.to_string());
        assert_eq!(
            populated.as_deref(),
            Some("P001: file must start with a YAML frontmatter delimiter")
        );
    }

    #[test]
    fn every_domain_enum_variant_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let mut plan = parse_markdown(VALID)?;
        for value in [
            PlanStatus::Draft,
            PlanStatus::Approved,
            PlanStatus::Implemented,
            PlanStatus::Abandoned,
        ] {
            plan.metadata.status = value;
            round_trip(&plan)?;
        }
        for value in [PlanMode::Single, PlanMode::Multi] {
            plan.metadata.format_version = FormatVersion::V1 { mode: value };
            round_trip(&plan)?;
        }
        for value in [
            SourceType::Asana,
            SourceType::Github,
            SourceType::Gitlab,
            SourceType::Linear,
            SourceType::Manual,
            SourceType::Other,
        ] {
            plan.metadata.source.kind = value;
            round_trip(&plan)?;
        }
        for value in [
            Severity::Unknown,
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical,
        ] {
            plan.metadata.bug.severity = value;
            round_trip(&plan)?;
        }
        for value in [
            TaskGraphStatus::Draft,
            TaskGraphStatus::Ready,
            TaskGraphStatus::Executing,
            TaskGraphStatus::Complete,
        ] {
            plan.metadata.execution.task_graph_status = value;
            round_trip(&plan)?;
        }
        for value in [
            TaskStatus::NotStarted,
            TaskStatus::Ready,
            TaskStatus::InProgress,
            TaskStatus::Blocked,
            TaskStatus::Done,
            TaskStatus::Abandoned,
        ] {
            let task = plan.tasks.first_mut().ok_or("fixture has no task")?;
            task.status = value;
            round_trip(&plan)?;
        }
        for value in [Criticality::Must, Criticality::Should, Criticality::Nice] {
            let criterion = plan
                .tasks
                .first_mut()
                .and_then(|task| task.hidden_criteria.first_mut())
                .ok_or("fixture has no hidden criterion")?;
            criterion.criticality = value;
            round_trip(&plan)?;
        }
        for value in [
            Evaluator::Automated,
            Evaluator::AgentEvaluated,
            Evaluator::HumanJudgment,
        ] {
            let criterion = plan
                .tasks
                .first_mut()
                .and_then(|task| task.hidden_criteria.first_mut())
                .ok_or("fixture has no hidden criterion")?;
            criterion.evaluator = value;
            if value == Evaluator::Automated {
                criterion.check = Some("true".to_owned());
                criterion.ask = None;
            } else {
                criterion.check = None;
                criterion.ask = Some("Does it pass?".to_owned());
            }
            round_trip(&plan)?;
        }
        Ok(())
    }

    #[test]
    fn invalid_domain_enum_values_are_diagnostics() {
        let invalid = "invalid";
        let results = [
            parse_plan_status(VALID, invalid).map(|_| ()),
            parse_mode(VALID, invalid).map(|_| ()),
            parse_source_type(VALID, invalid).map(|_| ()),
            parse_severity(VALID, invalid).map(|_| ()),
            parse_graph_status(VALID, invalid).map(|_| ()),
            parse_task_status(VALID, invalid).map(|_| ()),
            parse_criticality(VALID, invalid).map(|_| ()),
            parse_evaluator(VALID, invalid).map(|_| ()),
        ];
        assert!(results.iter().all(Result::is_err));
    }

    #[test]
    fn malformed_document_boundaries_are_diagnostics() {
        // `convert_metadata` propagates `parse_graph_status`'s error through
        // `?`; `invalid_domain_enum_values_are_diagnostics` above exercises
        // `parse_graph_status` directly but never through a full parse, so
        // this is the only place that error-propagation path is exercised.
        assert!(
            parse_markdown(&VALID.replacen(
                "task_graph_status: ready",
                "task_graph_status: bogus",
                1
            ))
            .is_err()
        );
        assert!(parse_markdown(&VALID.replace('\n', "\r\n")).is_err());
        assert!(parse_markdown(&VALID.replacen("title: Parser fixture", "title: [", 1)).is_err());
        assert!(parse_markdown("---\nunterminated").is_err());
        assert!(
            parse_markdown(&VALID.replace("# ADR: Parser fixture", "# Other heading")).is_err()
        );
        assert!(
            parse_markdown(&VALID.replace("plan_format_version: 1", "plan_format_version: 3"))
                .is_err()
        );
        // v2 is accepted, where it once was not: the same fixture read under the
        // v2 rule set fails only on the fields v2 removed and requires, never on
        // the version itself.
        let as_v2 = VALID.replace("plan_format_version: 1", "plan_format_version: 2");
        assert!(
            parse_markdown(&as_v2)
                .is_err_and(|error| error.to_string().contains("Planning Document Format v2"))
        );
        assert!(
            parse_markdown(&as_v2.replace("mode: single\n", "")).is_err_and(|error| {
                error.to_string().contains("missing required field: owner")
            })
        );
        assert!(
            parse_markdown(&VALID.replace("plan_id: PLAN-20260828-parser-fixture", "plan_id: ''"))
                .is_err()
        );
    }

    #[test]
    fn every_required_adr_section_is_enforced() -> Result<(), Box<dyn std::error::Error>> {
        let (_, body) = VALID
            .split_once("\n---\n")
            .ok_or("fixture has no frontmatter close")?;
        let headings = super::parse_headings(body);
        for (level, name) in [
            (2, "problem statement"),
            (3, "ticket"),
            (3, "discussion summary"),
            (2, "context"),
            (2, "constraints"),
            (2, "non-goals"),
            (2, "decision"),
            (2, "alternatives considered"),
            (2, "consequences"),
        ] {
            let filtered = headings
                .iter()
                .filter(|heading| !heading_matches(heading, level, name))
                .cloned()
                .collect::<Vec<_>>();
            assert!(parse_adr(VALID, body, 0, &filtered).is_err());
        }
        let body_without_colon = body.replace("# ADR: Parser fixture", "# ADR");
        let headings_without_colon = super::parse_headings(&body_without_colon);
        let adr = parse_adr(VALID, &body_without_colon, 0, &headings_without_colon)?;
        assert_eq!(adr.title, "ADR");
        Ok(())
    }

    #[test]
    fn task_graph_region_failures_are_diagnostics() {
        assert!(extract_task_graph("no markers").is_err());
        assert!(extract_task_graph(TASK_GRAPH_BEGIN).is_err());
        assert!(
            extract_task_graph(&format!("{TASK_GRAPH_BEGIN}\nno fence\n{TASK_GRAPH_END}")).is_err()
        );
        assert!(extract_task_graph(&format!("{TASK_GRAPH_BEGIN}```{TASK_GRAPH_END}")).is_err());
        assert!(
            extract_task_graph(&format!(
                "{TASK_GRAPH_BEGIN}\n```yaml\ntasks: []{TASK_GRAPH_END}"
            ))
            .is_err()
        );
    }

    #[test]
    fn heading_parser_rejects_non_headings_and_empty_sections() {
        assert_eq!(parse_heading("####### too deep"), None);
        assert_eq!(parse_heading("# "), None);
        assert_eq!(parse_heading("not a heading"), None);
        let wrong_level = Heading {
            level: 2,
            text: "ADR".to_owned(),
            content_start: 0,
            section_start: 0,
        };
        assert!(!heading_matches(&wrong_level, 1, "adr"));
        let empty_body = "## ADR\n";
        let empty_heading = Heading {
            level: 2,
            text: "ADR".to_owned(),
            content_start: empty_body.len(),
            section_start: 0,
        };
        assert_eq!(
            super::optional_section(empty_body, &[empty_heading], 2, "adr"),
            None
        );
    }

    #[test]
    fn malformed_task_identifiers_and_default_evaluator_are_handled()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(parse_markdown(&VALID.replace("id: T001", "id: invalid")).is_err());
        assert!(parse_markdown(&VALID.replace("depends_on: []", "depends_on: [invalid]")).is_err());
        assert!(
            parse_markdown(
                &VALID.replace("depends_on: []", "depends_on: []\n    blocks: [invalid]")
            )
            .is_err()
        );
        let without_evaluator = VALID.replace("        evaluator: human_judgment\n", "");
        let plan = parse_markdown(&without_evaluator)?;
        let evaluator = plan
            .tasks
            .first()
            .and_then(|task| task.hidden_criteria.first())
            .map(|criterion| criterion.evaluator);
        assert_eq!(evaluator, Some(Evaluator::HumanJudgment));
        Ok(())
    }
}
