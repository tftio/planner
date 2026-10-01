---
plan_format_version: 2
plan_id: PLAN-YYYYMMDD-short-slug
title: <short human-readable title>
status: draft
created_at: YYYY-MM-DD
updated_at: YYYY-MM-DD
owner: <operator-or-team>
source:
  type: manual              # asana | github | gitlab | linear | manual | other
  url: null
  external_id: null
  imported_at: null
bug:
  summary: <one-sentence description of the problem>
  severity: unknown         # unknown | low | medium | high | critical
  affected_area: null
  user_impact: null
execution:
  requires_operator_approval_before_implementation: true
  requires_plan_updates_during_execution: true
  task_graph_status: draft  # draft | ready | executing | complete
# Optional fleet project mapping; { slug, remote } — see PLAN_SPEC.md. Filled by
# `planner init --project <slug>`.
project: null
---

# ADR: <problem and decision record title>

## Problem Statement

<Describe the bug or problem. Must be understandable to an agent with no prior
conversation context.>

## Source Material

### Ticket

<URL / id / imported-at, or "No ticket was provided.">

### Discussion Summary

<Summarize all relevant ticket discussion. If a ticket was referenced but its
discussion was unavailable, say so and mark the plan as needing operator input. Do
not invent source material.>

## Context

<Existing behavior, expected behavior, observed failure, affected users/systems,
reproduction steps.>

## Constraints

<Technical, product, operational, security, compatibility, migration constraints.>

## Non-Goals

<What this plan explicitly does not attempt to solve.>

## Decision

<The intended implementation direction, in enough detail that independent agents do
not need to re-litigate the approach.>

## Alternatives Considered

<For each alternative: what it was, why it was considered, why it was rejected or
deferred.>

## Consequences

<Expected consequences, tradeoffs, risks, migration concerns, follow-up work.>

## Operator Guidance Log

Append-only. Add a dated entry whenever an agent needs operator guidance or operator
guidance changes the plan. A new plan legitimately has no entries yet; this note is
what satisfies the requirement until the first one is written.

### YYYY-MM-DD HH:MM UTC — <agent-or-operator-id>

Question or decision needed:

Decision or guidance received:

Impact on plan:

## Decision Log

Append-only. Decisions affecting architecture, behavior, sequencing, or tradeoffs, as
distinct from operational chatter. A new plan legitimately has no entries yet; this
note is what satisfies the requirement until the first one is written.

# Task Graph

<!-- TASK_GRAPH:BEGIN -->
```yaml
tasks:
  - id: T001
    title: <short imperative task title>
    status: not_started
    depends_on: []
    owner: null
    description: >
      <Complete description: what to do, why it is needed, what boundaries apply.>
    work_items:
      - <Specific unit of work.>
    invariants:
      - <Condition that must remain true after this task is complete.>
    acceptance_checks:
      - <Concrete check proving the task is complete.>
    completion_evidence: null
    # --- optional: criteria withheld from the worker during execution ---
    # hidden_criteria:
    #   - claim: <property the hidden criterion asserts>
    #     criticality: must            # must | should | nice
    #     evaluator: human_judgment    # automated | agent_evaluated | human_judgment
    #     ask: <question for the evaluator>          # or check: <shell command>
    #     why_hidden: <why concealment improves the signal — non-generic>
    #     counterfactual: <specific worker behavior predicted if it were visible>
```
<!-- TASK_GRAPH:END -->

# Task Details

## T001 — <short imperative task title>

Status: `not_started`

Depends on: none

### Description

<Repeat and expand the task description in prose.>

### Invariants

- <Required invariant.>

### Acceptance Checks

- <Concrete check.>

### Completion Evidence

<Agents update this when the task is complete.>

# Execution Protocol

Agents executing this plan must:

- Read the full plan and verify a task's dependencies are `done` before starting it.
- Set a task to `in_progress` and update `updated_at` when starting.
- Update the plan when they find it wrong, discover missing context, change the
  approach, need operator guidance, or alter the task graph.
- When operator guidance is needed: add an Operator Guidance Log entry, set affected
  tasks to `blocked`, and not guess when guessing risks rework, data loss, security,
  or product-behavior changes.
- When completing a task: verify invariants, run or describe acceptance checks, record
  `completion_evidence`, set status `done`, update `updated_at`, and update ADR
  sections only when the task changed context, decision, alternatives, or consequences.
- Name, in the completion evidence, the files the task actually touched. A path and a
  line number beats a filename.
- Treat the Operator Guidance Log, Decision Log, and completion evidence as
  append-only; supersede prior entries with a new dated entry rather than deleting them.
