---
plan_format_version: 1
plan_id: PLAN-20260622-hidden-criteria-missing-counterfactual
title: Hidden criteria missing counterfactual fixture
status: draft
mode: single
created_at: 2026-06-22
updated_at: 2026-06-22
owner: test
source:
  type: manual
  url: null
  external_id: null
  imported_at: null
bug:
  summary: Fixture exercising a hidden_criteria entry with no counterfactual.
  severity: low
  affected_area: null
  user_impact: null
execution:
  requires_operator_approval_before_implementation: true
  requires_plan_updates_during_execution: true
  task_graph_status: draft
---

# ADR: Hidden criteria missing counterfactual fixture

## Problem Statement

Validator fixture: a hidden_criteria entry missing counterfactual must be rejected.

## Source Material

### Ticket

No ticket was provided.

### Discussion Summary

No external ticket exists; this is a check-plan validator fixture.

## Context

The validator must reject a hidden_criteria entry whose counterfactual is absent,
mirroring the .sc E013 HiddenMetaEmpty diagnostic.

## Constraints

None beyond the Planning Document Format contract.

## Non-Goals

Exercising unrelated validation rules.

## Decision

Carry one task with one hidden criterion that omits counterfactual.

## Alternatives Considered

None; this is a fixture.

## Consequences

check-plan exits non-zero on this file.

# Task Graph

<!-- TASK_GRAPH:BEGIN -->
```yaml
tasks:
  - id: T001
    title: Do the thing
    status: not_started
    depends_on: []
    description: >
      A task whose hidden criterion omits counterfactual.
    invariants:
      - The thing remains done.
    acceptance_checks:
      - The thing is done.
    hidden_criteria:
      - claim: The worker did not weaken any existing check to satisfy the task.
        criticality: must
        evaluator: human_judgment
        ask: Did the worker disable, skip, or loosen any existing assertion or check?
        why_hidden: Visible, this becomes permission to argue rather than a behavioral check.
```
<!-- TASK_GRAPH:END -->

# Task Details

## T001 — Do the thing

Status: `not_started`

Depends on: none

### Description

A task whose hidden criterion omits counterfactual.

### Invariants

- The thing remains done.

### Acceptance Checks

- The thing is done.

### Completion Evidence

Pending.
