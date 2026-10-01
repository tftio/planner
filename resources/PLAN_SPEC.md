# Planning Document Format

This file is the canonical contract for implementation planning documents. It defines
two versions of that contract. It is the human-readable companion to the deterministic
validator, `planner validate`. **The validator, not this prose, decides whether a plan
is valid.** When the two disagree, the validator wins and this document is the bug.

A plan is consumed by both humans and a suite of automated coding agents. It must
therefore be readable *and* machine-parseable.

## Versions

A plan declares an integer `plan_format_version` in its frontmatter. That number, and
nothing else, selects the rule set the plan is validated against.

| Version | Status | Shape |
|---------|--------|-------|
| `1` | Frozen. Existing v1 plans stay valid indefinitely; the rule set is never extended. | Declares a `mode` of `single` or `multi`. `multi` additionally requires the Operator Guidance Log, the Decision Log, the Execution Protocol, and four richer per-task fields. |
| `2` | Current. Every new plan is written in v2. | One shape and no `mode`. The two logs, the Execution Protocol, and the richer per-task fields are required in every plan. |

Version rules (enforced):

- `plan_format_version` is required, and is the integer `1` or `2`. Any other value —
  including the string `"1"`, a float, or an unreleased version number — is an error.
- A validator reads `plan_format_version` before anything else and applies that
  version's rule set in full. Rule sets are not mixed: a v1 plan is never judged against
  a v2 rule, and a v2 plan is never judged against a v1 rule.
- Version-specific rules live under `# Planning Document Format v1` and
  `# Planning Document Format v2` below. Where the v2 section adopts a v1 rule, it says
  so explicitly and by name; nothing is inherited silently.

## Migration between versions

**No plan is migrated to a later version as a side effect of being edited.** A tool that
reads, validates, mutates, projects, or re-renders a v1 plan writes it back as a v1
plan: it does not change `plan_format_version`, does not remove `mode`, does not remove
`blocks`, and does not add the sections v2 requires. A v1 plan edited for an unrelated
reason — a status transition, an appended log entry, completion evidence — must still
validate under the v1 rules afterwards.

Migrating a plan from v1 to v2 is a deliberate, separately-invoked operation. This
document defines no such operation. Until one exists, a plan stays at the version it was
written at for life.

# Planning Document Format v1

This is the canonical contract for implementation planning documents. It is the
human-readable companion to the deterministic validator, `planner validate`. **The
validator, not this prose, decides whether a plan is valid.** When the two disagree,
the validator wins and this document is the bug.

A plan is consumed by both humans and a suite of automated coding agents. It must
therefore be readable *and* machine-parseable.

## Purpose

Every plan serves four jobs:

1. Preserve the full problem context (ADR-style problem and decision record).
2. Record the implementation decision and the alternatives that were rejected.
3. Define a machine-readable task DAG that a suite of agents can execute.
4. Provide an update protocol so executing agents keep the plan current.

## Location and filename

- Plans live under `docs/plans/`.
- The filename **MUST** match `YYYY-MM-DD-<slug>.md`, where `<slug>` is
  lowercase, hyphen-separated, no spaces (e.g. `2026-06-15-replace-auth-middleware.md`).
- The validator treats a non-matching basename as an error and a location outside
  `docs/plans/` as a warning (so a plan can be validated in place before it is moved).

## Format modes

A plan declares a `mode` in its frontmatter. The mode scales how much structure is
required. The **core** is always required; `multi` mode additionally requires the
heavyweight execution machinery.

| Mode | Use for | Adds on top of core |
|------|---------|---------------------|
| `single` | One agent or a person executing a small, self-contained change | nothing |
| `multi` | A suite of agents executing the task DAG concurrently | Execution Protocol, Operator Guidance Log, Decision Log, and the richer per-task fields |

Everything in **Core requirements** below applies to every plan regardless of mode.

## Core requirements (all modes)

### Frontmatter

YAML frontmatter holds compact, machine-readable metadata only. Long-form prose,
ADR discussion, and ticket history belong in the body, never in frontmatter.

```yaml
plan_format_version: 1
plan_id: PLAN-YYYYMMDD-short-slug      # stable id; PLAN- + date + slug
title: Short human-readable title
status: draft                          # draft | approved | implemented | abandoned
mode: single                           # single | multi
created_at: 2026-06-15
updated_at: 2026-06-15
owner: operator-or-team-name
source:
  type: manual                         # asana | github | gitlab | linear | manual | other
  url: null
  external_id: null
  imported_at: null
bug:
  summary: One-sentence description of the problem.
  severity: unknown                    # unknown | low | medium | high | critical
  affected_area: null
  user_impact: null
execution:
  requires_operator_approval_before_implementation: true
  requires_plan_updates_during_execution: true
  task_graph_status: draft             # draft | ready | executing | complete
```

Required frontmatter keys: `plan_format_version`, `plan_id`, `title`, `status`,
`mode`, `created_at`, `updated_at`, `owner`, `source`, `bug`, `execution`.
`created_at`/`updated_at` must be `YYYY-MM-DD`. `source`, `bug`, and `execution`
must be mappings containing the keys shown above.

### ADR body

The body opens with an ADR-style record. These headings are required, in this order:

1. `# ADR: ...` (H1, title may vary after the colon)
2. `## Problem Statement` — understandable to an agent with no prior conversation context.
3. `## Source Material` — with `### Ticket` and `### Discussion Summary` subsections.
4. `## Context` — existing behavior, expected behavior, observed failure, repro.
5. `## Constraints`
6. `## Non-Goals`
7. `## Decision` — enough detail that independent agents need not re-litigate the approach.
8. `## Alternatives Considered`
9. `## Consequences`

No required section may be empty (placeholder/boilerplate-only text counts as empty).

Source material rules:

- Do not invent source material. If a ticket is referenced but its discussion was
  unavailable, say so explicitly in `### Discussion Summary` and mark the plan as
  needing operator input.
- If there is no ticket, `### Discussion Summary` must say so plainly.

### Task Graph

A machine-readable task DAG is required in every mode. It lives between explicit
markers so the validator can extract it unambiguously:

```text
# Task Graph

<!-- TASK_GRAPH:BEGIN -->
```yaml
tasks:
  - id: T001
    title: Short imperative task title
    status: not_started
    depends_on: []
    description: >
      Complete description of the task.
    invariants:
      - Condition that must remain true after this task is complete.
    acceptance_checks:
      - Concrete check proving the task is complete.
\```
<!-- TASK_GRAPH:END -->
```

Required per-task fields (all modes): `id`, `title`, `status`, `depends_on`,
`description`, `invariants` (non-empty), `acceptance_checks` (non-empty).

Task id format: `T` followed by at least three digits (`T001`).

Task `status` is one of: `not_started`, `ready`, `in_progress`, `blocked`, `done`,
`abandoned`.

DAG rules (enforced):

- Every `id` is unique and matches the id format.
- Every entry in `depends_on` refers to an existing task id.
- No task depends on itself.
- The graph is acyclic.
- A task marked `ready`, `in_progress`, or `done` must have all dependencies `done`.
- A task marked `done` must have non-empty `completion_evidence`.

### Task Details

A `# Task Details` section follows the graph, with one `## <id> — <title>` subsection
per task. The validator enforces a bijection: every task in the YAML graph has a
matching detail subsection, and every detail subsection matches a task in the graph.

### Worker projection (optional, all modes)

A task **MAY** declare what a worker executing it is allowed to see and what it is
allowed to hand back. These fields exist so an isolated worker can be given a file tree
rather than a repository, and so a returned change set can be checked mechanically
instead of reviewed for scope creep.

```yaml
    seed:                       # paths projected into the worker's tree
      - src/api
      - tests/api
    allow:                      # paths a returned manifest may contain
      - src/api/tasks.rs
    guest_class: linux          # linux | macos
    seed_includes_plans: false  # optional; opt in to projecting docs/plans/
```

Worker-projection rules (enforced):

- All four fields are optional. A plan written before they existed stays valid.
- `guest_class`, if present, is one of `linux`, `macos`.
- `seed`, if present, is a non-empty list of repository-relative paths. A path may not
  be absolute and may not contain a `..` segment.
- `allow`, if present, is a non-empty list, and every entry is equal to or underneath
  some `seed` entry. A worker cannot return what it was never given.
- `allow` may not be declared without `seed`.
- No `seed` path may project `docs/plans/`, in either direction: naming `docs/plans`
  is rejected, and so is naming `docs` or `.`, which reach it. Set
  `seed_includes_plans: true` on a task whose deliverable is a plan document.

**Absence is not permission.** A task without `seed` is not scoped for isolated
execution, and a supervisor should refuse to dispatch it rather than project the whole
repository. A forgotten field must fail closed.

The `docs/plans/` rule exists because this project keeps its plans inside the
repository it plans against. A worker that can read the plan directory can read the
acceptance criteria it is being judged against — including the `hidden_criteria`
deliberately withheld from it — so a seed that reaches `docs/plans/` silently voids
the supervision guarantee that hidden criteria exist to provide.

### Failure dispositions

A worker reports the outcome of its attempt as one value from a closed set, so a
supervisor can route a retry without an LLM judgment about free text. Plans do not
carry these values; the vocabulary is defined here so the plan format and the
supervisor agree on one set.

| Disposition | Meaning |
|---|---|
| `succeeded` | The task was completed and a manifest is offered. |
| `blocked_by_brief` | The brief is underspecified or self-contradictory. Escalate. |
| `blocked_by_environment` | A tool, dependency, or fixture the task needs is unavailable. |
| `flaky_evidence` | A check failed non-deterministically. Retrying unchanged is reasonable. |
| `not_understood` | The code does something the worker could not account for. The brief was probably thin. |
| `out_of_scope` | The change required lies outside the task's `allow` paths. |
| `ceiling_exhausted` | An iteration or token ceiling was reached before completion. |

A failing disposition is accompanied by a structured report naming the exact error,
what was attempted, and what the worker believes blocked it. A worker that fails fast
without diagnosis discards the most valuable output of the run.

### Hidden criteria (optional, all modes)

A task **MAY** carry an optional `hidden_criteria` list. A hidden criterion is an
acceptance criterion deliberately withheld from the worker during execution, so the
worker cannot optimize against the full acceptance surface. They are the central
supervision artifact of the `silent-critic` tool and map 1:1 onto its `.sc` hidden
bindings; the bundled importer translates them mechanically.

Each entry is a mapping:

```yaml
hidden_criteria:
  - claim: "The property this hidden criterion asserts."
    criticality: must            # must | should | nice
    evaluator: human_judgment    # optional, default human_judgment;
                                 # automated | agent_evaluated | human_judgment
    ask: "Question for the evaluator."   # use with human_judgment or agent_evaluated
    # check: "shell command"             # use instead of ask with automated (or agent_evaluated)
    why_hidden: "Why concealing this criterion improves the signal (non-generic)."
    counterfactual: "The specific worker behavior predicted if it were visible."
```

Hidden-criteria rules (enforced):

- `claim` is a non-empty string.
- `criticality` is one of `must`, `should`, `nice`.
- `evaluator`, if present, is one of `automated`, `agent_evaluated`, `human_judgment`
  (default `human_judgment`).
- Exactly one of `check` / `ask` is present and non-empty, consistent with the
  evaluator: `automated` requires `check`; `human_judgment` requires `ask`;
  `agent_evaluated` accepts either.
- `why_hidden` and `counterfactual` are both present and non-empty.

The metadata-non-emptiness rule mirrors the `.sc` validator's E013 (HiddenMetaEmpty);
the evaluator/spec rule mirrors its E002. Together they guarantee a valid plan imports
to a compilable `.sc` contract. Do not auto-author `why_hidden`/`counterfactual` — the
operator writes them; the importer only translates them.

Hidden criteria are authored **only** in the task graph YAML, never echoed in Task
Details prose, so the worker-facing projection has a single place to strip them. An
executing worker is served a hidden-stripped projection of the plan, never the raw
plan; the committed plan with its hidden criteria is an operator artifact.

#### Verdict fields (optional)

A hidden criterion **MAY** additionally carry the verdict a supervision loop has
rendered against it. These fields are never authored by an agent
(`why_hidden`/`counterfactual`'s own rule applies equally here); they are written by a
ledger through the same mutation path as every other plan change, addressed by the
criterion's position in the `hidden_criteria` list:

```yaml
hidden_criteria:
  - claim: "The property this hidden criterion asserts."
    criticality: must
    evaluator: agent_evaluated
    ask: "Question for the evaluator."
    why_hidden: "..."
    counterfactual: "..."
    verdict: undetermined              # optional; pass | fail | undetermined
    rationale: "Why the judge reached this verdict."   # required whenever verdict is present
    evidence_needed: "What would resolve it."          # required iff verdict is undetermined
    evidence:                                           # optional list, default empty
      - summary: "A short description of what this evidence shows."
        provenance: judge              # tool_authored | judge | worker_narrated | operator_observed
```

Verdict-field rules (enforced):

- `verdict`, if present, is one of `pass`, `fail`, `undetermined`.
- `rationale` is a non-empty string whenever `verdict` is present; absent otherwise.
- `evidence_needed` is a non-empty string if and only if `verdict` is `undetermined`.
- Each `evidence` entry has a non-empty `summary` and a `provenance` of `tool_authored`,
  `judge`, `worker_narrated`, or `operator_observed`.

Like every other hidden-criterion field, `verdict`/`rationale`/`evidence_needed`/
`evidence` are stripped wholesale from the worker-facing projection: the projection
removes the entire `hidden_criteria` entry, not a subset of its fields, so no
verdict-field addition here can leak by itself.

## Additional requirements for `mode: multi`

When `mode: multi`, the following are also required.

### Extra body sections

- `## Operator Guidance Log` — append-only; agents add a dated entry whenever they
  need operator guidance or operator guidance changes the plan.
- `## Decision Log` — append-only; records decisions affecting architecture,
  behavior, sequencing, or tradeoffs (distinct from operational chatter).
- `# Execution Protocol` — the rules executing agents follow (see below).

### Extra per-task fields

In addition to the core fields, each task must define: `owner` (may be `null`),
`blocks` (list, may be empty), `work_items` (non-empty), `files` (a mapping with
`likely_read` and `likely_modify` lists), and `completion_evidence` (may be `null`
until the task is `done`).

### Execution Protocol contract

The `# Execution Protocol` section must instruct executing agents to:

- Read the full plan and verify a task's dependencies are `done` before starting it.
- Set a task to `in_progress` and update `updated_at` when starting.
- Update the plan when they discover it is wrong, find missing context, change the
  approach, need operator guidance, or alter the task graph.
- When operator guidance is needed: add an Operator Guidance Log entry, set affected
  tasks to `blocked`, and not guess if guessing risks rework, data loss, security, or
  product-behavior changes.
- When completing a task: verify invariants, run/describe acceptance checks, record
  `completion_evidence`, set status `done`, update `updated_at`, and update ADR
  sections only if the task changed context, decision, alternatives, or consequences.
- Treat Operator Guidance Log, Decision Log, and completion evidence as append-only;
  supersede prior entries with a new dated entry rather than deleting them.

## Status transition rules (guidance)

These are not all mechanically enforced, but agents must follow them:

```text
not_started -> ready | blocked
ready       -> in_progress | blocked
in_progress -> blocked | done
blocked     -> ready | abandoned
done        -> in_progress   (only when reopened, with a stated reason)
any         -> abandoned     (only with a stated reason)
```

A task may be `ready` only when all dependencies are `done`. A task reopened from
`done` must record a reopening reason; a task `abandoned` must record an abandonment
reason; a `blocked` task must record a blocking reason.

## What the validator does not check

The validator enforces structure, not judgment. It does **not** decide whether a
plan is thoughtful, whether risks are adequately considered, or whether the design is
sound. Those are review criteria for a human or an LLM reviewer, not deterministic
enforcement.

# Planning Document Format v2

Everything in this section applies to a document whose `plan_format_version` is `2`, and
to no other document. v2 has one shape. There is no `mode`: every plan carries the
Operator Guidance Log, the Decision Log, and the Execution Protocol, and every task
carries the fields a session with no prior context needs in order to resume it.

`mode` was retired because concurrency is a property of how a plan is *executed*, not of
the document, while the fields `mode` gated — the two logs, and `owner`, `work_items`,
`completion_evidence` — are about record-keeping and cold-start affordance, neither of
which varies with how many agents walk the graph.

## What changed from v1

Orientation only; the normative statements are the numbered rule lists below.

| v1 | v2 |
|----|----|
| `mode: single \| multi` in frontmatter | No `mode`. The key is rejected. |
| Two logs and the Execution Protocol required only when `mode: multi` | Required in every plan. |
| `owner`, `work_items`, `completion_evidence` required only when `mode: multi` | Required on every task. |
| `files`, a mapping of `likely_read` and `likely_modify`, required when `mode: multi` | Removed. Rejected if present; completion evidence names the files actually touched. |
| `blocks` authored by hand, required when `mode: multi` | Removed as an authored field. Rejected if present; derived from `depends_on`. |
| Worker projection — `seed`, `allow`, `guest_class`, `seed_includes_plans` — optional on any task | Removed. All four are rejected if present. |

## Frontmatter (v2)

YAML frontmatter holds compact, machine-readable metadata only. Long-form prose, ADR
discussion, and ticket history belong in the body, never in frontmatter.

```yaml
plan_format_version: 2
plan_id: PLAN-YYYYMMDD-short-slug      # stable id; PLAN- + date + slug
title: Short human-readable title
status: draft                          # draft | approved | implemented | abandoned
created_at: 2026-09-10
updated_at: 2026-09-10
owner: operator-or-team-name
source:
  type: manual                         # asana | github | gitlab | linear | manual | other
  url: null
  external_id: null
  imported_at: null
bug:
  summary: One-sentence description of the problem.
  severity: unknown                    # unknown | low | medium | high | critical
  affected_area: null
  user_impact: null
execution:
  requires_operator_approval_before_implementation: true
  requires_plan_updates_during_execution: true
  task_graph_status: draft             # draft | ready | executing | complete
project: null                          # optional; { slug, remote } — see below
```

Frontmatter rules (enforced):

- Required keys, exactly these: `plan_format_version`, `plan_id`, `title`, `status`,
  `created_at`, `updated_at`, `owner`, `source`, `bug`, `execution`. This is the v1 list
  with `mode` removed. `project` is optional and not in this list.
- **A v2 plan that declares `mode` is invalid.** The key is rejected, not ignored. The
  diagnostic must name the version and say the field was removed in v2 — an author who
  wrote `mode` out of v1 habit is told what happened, not merely that an unknown key
  appeared.
- `plan_format_version` is the integer `2`.
- `plan_id`, `title`, and `owner` are non-empty strings.
- `status` is one of `draft`, `approved`, `implemented`, `abandoned`.
- `created_at` and `updated_at` match `YYYY-MM-DD`.
- `source`, `bug`, and `execution` are mappings containing the keys shown above;
  `source.type` is one of `asana`, `github`, `gitlab`, `linear`, `manual`, `other`;
  `bug.severity` is one of `unknown`, `low`, `medium`, `high`, `critical`;
  `execution.task_graph_status` is one of `draft`, `ready`, `executing`, `complete`.
- No other key is required. Unknown keys other than `mode` are not rejected.
- **`project`, when present, maps the plan onto the fleet's shared project identity.**
  It is a mapping `{ slug, remote }`. `slug` is required and must match the fleet
  project-slug grammar (`^[a-z0-9][a-z0-9-]*$`, at most 64 bytes) that
  `tftio_lib::project::Slug` enforces; this document does not restate the grammar, it
  names the one implementation that owns it. `remote`, if present and non-null, must
  already be a normalized remote: passing it to `tftio_lib::project::normalize_remote`
  must succeed and return the same text unchanged. A `project` with a malformed `slug`
  or an unnormalized `remote` is invalid. `project` is absent from every v1 document;
  v1 does not model the key, and an unmodeled key already tolerated by v1 stays
  tolerated (and silently dropped by the next rewrite), not rejected.

## Required body sections (v2)

The ADR block is order-enforced. Everything after it is presence-enforced only, because
the logs, the graph, the details, and the protocol legitimately interleave.

Required in this order:

1. `# ADR: ...` (H1; the title after the colon may vary)
2. `## Problem Statement` — understandable to an agent with no prior conversation context.
3. `## Source Material` — with `### Ticket` and `### Discussion Summary` subsections.
4. `## Context` — existing behavior, expected behavior, observed failure, repro.
5. `## Constraints`
6. `## Non-Goals`
7. `## Decision` — enough detail that independent agents need not re-litigate the approach.
8. `## Alternatives Considered`
9. `## Consequences`

Required, presence only, in any position after the ADR block:

- `## Operator Guidance Log` — append-only; agents add a dated entry whenever they need
  operator guidance or operator guidance changes the plan.
- `## Decision Log` — append-only; decisions affecting architecture, behavior,
  sequencing, or tradeoffs, as distinct from operational chatter.
- `# Task Graph` — the machine-readable DAG, between its markers.
- `# Task Details` — one subsection per task.
- `# Execution Protocol` — the rules executing agents follow.

No required section may be empty; placeholder or boilerplate-only text counts as empty.
The two logs are required from the moment a plan is authored, when both are legitimately
empty of entries — an author satisfies the rule with the standing "append-only" note the
template carries, not by inventing an entry.

Source material rules are unchanged from v1: do not invent source material; if a ticket
is referenced but its discussion was unavailable, say so explicitly in
`### Discussion Summary` and mark the plan as needing operator input; if there is no
ticket, `### Discussion Summary` must say so plainly.

## Task Graph (v2)

The DAG lives between explicit markers so a validator can extract it unambiguously:

```text
# Task Graph

<!-- TASK_GRAPH:BEGIN -->
```yaml
tasks:
  - id: T001
    title: Short imperative task title
    status: not_started
    depends_on: []
    owner: null
    description: >
      Complete description of the task.
    work_items:
      - Specific unit of work.
    invariants:
      - Condition that must remain true after this task is complete.
    acceptance_checks:
      - Concrete check proving the task is complete.
    completion_evidence: null
\```
<!-- TASK_GRAPH:END -->
```

Required per-task fields, on every task: `id`, `title`, `status`, `depends_on`,
`description`, `owner`, `work_items`, `invariants`, `acceptance_checks`,
`completion_evidence`.

Per-task field rules (enforced):

- `id` matches `T` followed by at least three digits (`T001`), and is unique in the plan.
- `title` and `description` are non-empty strings.
- `status` is one of `not_started`, `ready`, `in_progress`, `blocked`, `done`,
  `abandoned`.
- `depends_on` is a list of task ids, possibly empty.
- `owner` is present and may be `null`.
- `work_items` is a non-empty list.
- `invariants` is a non-empty list.
- `acceptance_checks` is a non-empty list.
- `completion_evidence` is present, may be `null`, and must be a non-empty string when
  `status` is `done`.
- **A v2 task that declares `blocks` is invalid.** As with `mode`, the field is rejected
  rather than ignored, and the diagnostic must name the version and say the field was
  removed in v2.
- **A v2 task that declares `files` is invalid.** As with `mode`, the field is rejected
  rather than ignored, and the diagnostic must name the version and say the field was
  removed in v2.
- **A v2 task that declares `seed`, `allow`, `guest_class` or `seed_includes_plans` is
  invalid.** v2 has no worker projection. Each of the four is rejected rather than
  ignored, and the diagnostic must name the version and say the field was removed in v2.

`blocks` is the exact inverse of `depends_on`. Authoring it stores derived data that can
drift from its source, and the validator would have to check the two against each other.
In v2 a task's downstream set is **computed from `depends_on`** wherever it is wanted: a
tool that presents `blocks` derives it on projection or inspection, and never reads it
from the document.

`files` asked an author to enumerate, before any of the work was done, the files a task
would read and modify. Nothing validated its content, and a structured field invites a
cold session to read it as a manifest. Measured against a completed eight-task plan it
was exact only on the three tasks whose title already named the file — where it restated
the title in a second syntax — and under-predicted by eight and by fifteen files on the
two tasks whose file set was the *output* of the work. v2 replaces it with a reporting
obligation rather than a field: **completion evidence names the files the task actually
touched**, written after the work by the session that did it. A downstream task's cold
session orients from its dependencies' evidence, which is observed rather than guessed.

Worker projection leaves v2 entirely. `seed` and `allow` are author-written path lists
of the same kind, describing a capability grant for a worker runtime that does not yet
exist; `seed` further conflated a hard security boundary with a soft scoping preference.
The security boundary survives without them, unconditionally — see **Worker-facing
projection (v2)** below. `guest_class` and `seed_includes_plans` go with them rather than
surviving as orphans of a deleted section: `seed_includes_plans` has no referent once
`seed` is gone, and a version whose worker-projection section has exactly one member is
harder to describe, teach and validate than one with no such section. If a worker runtime
later needs scoping or an operating-system selector, v2 adds fields for what that runtime
actually needs.

DAG rules (enforced), unchanged from v1:

- Every `id` is unique and matches the id format.
- Every entry in `depends_on` refers to an existing task id.
- No task depends on itself.
- The graph is acyclic.
- A task marked `ready`, `in_progress`, or `done` must have all dependencies `done`.
- A task marked `done` must have non-empty `completion_evidence`.

## Task Details (v2)

A `# Task Details` section follows the graph, with one `## <id> — <title>` subsection per
task. A validator enforces a bijection: every task in the YAML graph has a matching
detail subsection, and every detail subsection matches a task in the graph. Unchanged
from v1.

## Execution Protocol contract (v2)

`# Execution Protocol` is required in every v2 plan. It must instruct executing agents to:

- Read the full plan and verify a task's dependencies are `done` before starting it.
- Set a task to `in_progress` and update `updated_at` when starting.
- Update the plan when they discover it is wrong, find missing context, change the
  approach, need operator guidance, or alter the task graph.
- When operator guidance is needed: add an Operator Guidance Log entry, set affected
  tasks to `blocked`, and not guess if guessing risks rework, data loss, security, or
  product-behavior changes.
- When completing a task: verify invariants, run or describe acceptance checks, record
  `completion_evidence`, set status `done`, update `updated_at`, and update ADR sections
  only if the task changed context, decision, alternatives, or consequences.
- **Name, in the completion evidence, the files the task actually touched.** A path and
  a line number beats a filename. This is what v2 carries instead of an authored `files`
  field: a record written after the work by the session that did it, rather than a
  forecast written before it.
- Treat the Operator Guidance Log, Decision Log, and completion evidence as append-only;
  supersede prior entries with a new dated entry rather than deleting them.

A plan may add rules of its own to this section. It may not drop one of the above.

## Worker-facing projection (v2)

A v2 plan carries no worker-projection fields. There is no `seed`, no `allow`, no
`guest_class` and no `seed_includes_plans`, and so no author-written statement of what a
worker may see or hand back.

One property survives, and it is unconditional rather than configurable:

- **A worker is never handed the raw plan.** What a worker receives is a projection of
  the plan, and that projection strips `hidden_criteria` wholesale. There is no field by
  which an author can widen the worker's view of the criteria it is judged against, and
  therefore no field an author can forget.

In v1 this guarantee is guarded indirectly, by a rule that no `seed` path may reach
`docs/plans/`. That shape makes a security boundary look like a path allowlist an author
configures, and the two failure directions are not symmetric: an allowlist that is too
narrow fails loudly and cheaply, while one that is too broad fails silently and voids the
supervision guarantee with no artifact showing it. Iteration pressure therefore runs in
the unsafe direction. In v2 the guarantee is a property of projection itself, enforced by
the projection step rather than by a rule about a path an author wrote.

## Adopted from v1 without change

The following are defined once, in the v1 section above, and apply identically to a v2
plan. They are listed here by name so nothing is inherited silently:

- **Location and filename.** Plans live under `docs/plans/`; the basename must match
  `YYYY-MM-DD-<slug>.md`; a non-matching basename is an error and a location outside
  `docs/plans/` is a warning.
- **Failure dispositions** — the closed vocabulary a worker reports its outcome from.
  One entry reads differently under v2: `out_of_scope` is defined in the v1 section in
  terms of a task's `allow` paths, and a v2 task has none, so a v2 worker reports it when
  the change required lies outside the task as briefed.
- **Hidden criteria** — the optional `hidden_criteria` list, its enums, its
  exactly-one-of `check`/`ask` rule, the requirement that `why_hidden` and
  `counterfactual` be operator-written and never auto-generated, and the rule that
  hidden criteria are authored only in the graph YAML and stripped wholesale from the
  worker-facing projection.
- **Verdict fields** — the optional `verdict`, `rationale`, `evidence_needed`, and
  `evidence` on a hidden criterion, written by a ledger and never by an authoring agent.
- **Status transition rules** — the guidance transitions, and the requirement that a
  reopened, abandoned, or blocked task record its reason.
- **What the validator does not check** — structure, not judgment.

These are version-independent because none of them was ever gated on `mode`. Worker
projection was on this list and is not any more: v2 removes it outright rather than
inheriting it, which is the first place v2 differs from v1 by subtraction rather than by
replacement. v1 keeps all four fields and every rule the v1 section states about them,
unchanged.
