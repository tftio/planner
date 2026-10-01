---
plan_format_version: 2
plan_id: PLAN-20260910-format-v2-baseline
title: Format v2 cross-validator baseline
status: approved
created_at: 2026-09-10
updated_at: 2026-09-10
owner: test
source:
  type: manual
  url: null
  external_id: null
  imported_at: null
bug:
  summary: The v2 parser needs a portable representative fixture.
  severity: low
  affected_area: parser
  user_impact: Malformed v2 plans would otherwise reach consumers.
execution:
  requires_operator_approval_before_implementation: true
  requires_plan_updates_during_execution: true
  task_graph_status: ready
---

# ADR: Format v2 cross-validator baseline

## Problem Statement

Two independent validators decide whether a document is a valid v2 plan. This
document is the baseline every v2 rule fixture is a single edit away from, so that a
disagreement between the validators is attributable to one rule.

## Source Material

### Ticket

No ticket was provided.

### Discussion Summary

No external ticket exists; this is the cross-validator fixture baseline.

## Context

The fixture exercises the v2 rule set: no mode, no blocks, and the handoff record
required of every plan rather than gated on a mode. Every case in manifest.toml is
this document with one anchor replaced, so each case isolates exactly one rule.

## Constraints

Use LF line endings.

## Non-Goals

Exercising mutation, projection, or any v1 rule.

## Decision

Dispatch on plan_format_version and apply exactly one rule set.

## Alternatives Considered

Defaulting a v2 plan's mode to single was rejected because it would let v2 code
silently take a v1 branch.

## Consequences

A v2 plan carries the two logs and the Execution Protocol unconditionally.

## Operator Guidance Log

Append-only. Agents add a dated entry whenever they need operator guidance or
operator guidance changes the plan.

## Decision Log

Append-only. Decisions affecting architecture, behavior, sequencing, or
tradeoffs are recorded here.

# Task Graph

<!-- TASK_GRAPH:BEGIN -->
```yaml
tasks:
  - id: T001
    title: Parse the v2 fixture
    status: not_started
    owner: null
    depends_on: []
    description: Parse representative v2 input.
    work_items:
      - Read the frontmatter under the v2 rule set.
    invariants:
      - Source bytes remain unchanged.
    acceptance_checks:
      - The typed task is available.
    hidden_criteria:
      - claim: Worker output never contains this canary.
        criticality: must
        evaluator: human_judgment
        ask: Is the canary absent?
        why_hidden: Revealing the canary would invalidate the concealment test.
        counterfactual: A worker given the canary could optimize output around it.
    completion_evidence: null
  - id: T002
    title: Derive the downstream set
    status: not_started
    owner: test
    depends_on:
      - T001
    description: Compute blocks from depends_on rather than reading it.
    work_items:
      - Invert depends_on across the graph.
    invariants:
      - blocks is never read from the document.
    acceptance_checks:
      - T001 reports T002 as downstream.
    completion_evidence: null
```
<!-- TASK_GRAPH:END -->

# Task Details

## T001 — Parse the v2 fixture

Status: `not_started`

Depends on: none

### Description

Parse representative v2 input.

### Invariants

- Source bytes remain unchanged.

### Acceptance Checks

- The typed task is available.

### Completion Evidence

Pending.

## T002 — Derive the downstream set

Status: `not_started`

Depends on: T001

### Description

Compute blocks from depends_on rather than reading it.

### Invariants

- blocks is never read from the document.

### Acceptance Checks

- T001 reports T002 as downstream.

### Completion Evidence

Pending.

# Execution Protocol

Agents executing this plan must:

- Read the full plan and verify a task's dependencies are `done` before starting it.
- Set a task to `in_progress` and update `updated_at` when starting.
- Update the plan when they find it wrong, discover missing context, change the approach,
  need operator guidance, or alter the task graph.
- When operator guidance is needed: add an Operator Guidance Log entry, set affected tasks
  to `blocked`, and not guess when guessing risks rework, data loss, security, or
  product-behavior changes.
- When completing a task: verify invariants, run or describe acceptance checks, record
  `completion_evidence`, set status `done`, update `updated_at`, and update ADR sections
  only when the task changed context, decision, alternatives, or consequences.
- Treat the Operator Guidance Log, Decision Log, and completion evidence as append-only;
  supersede prior entries with a new dated entry rather than deleting them.
