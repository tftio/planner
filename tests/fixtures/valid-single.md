---
plan_format_version: 1
plan_id: PLAN-20260828-parser-fixture
title: Parser fixture
status: approved
mode: single
created_at: 2026-08-28
updated_at: 2026-08-28
owner: test
source:
  type: manual
  url: null
  external_id: null
  imported_at: null
bug:
  summary: The parser needs a portable representative fixture.
  severity: low
  affected_area: parser
  user_impact: Malformed plans would otherwise reach consumers.
execution:
  requires_operator_approval_before_implementation: true
  requires_plan_updates_during_execution: true
  task_graph_status: ready
---

# ADR: Parser fixture

## Problem Statement

The parser must preserve this prose.

## Source Material

### Ticket

No ticket was provided.

### Discussion Summary

No external ticket exists; this is a parser fixture.

## Context

The fixture exercises typed YAML and exact Markdown retention.

## Constraints

Use LF line endings.

## Non-Goals

Exercising mutation.

## Decision

Parse owned spans and retain all source bytes.

## Alternatives Considered

Whole-document reserialization was rejected because it loses unrelated Markdown.

## Consequences

Consumers receive typed values and source identity.

# Task Graph

<!-- TASK_GRAPH:BEGIN -->
```yaml
tasks:
  - id: T001
    title: Parse the fixture
    status: not_started
    depends_on: []
    description: Parse representative input.
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
```
<!-- TASK_GRAPH:END -->

# Task Details

## T001 — Parse the fixture

Status: `not_started`

Depends on: none

### Description

Parse representative input.

### Invariants

- Source bytes remain unchanged.

### Acceptance Checks

- The typed task is available.

### Completion Evidence

Pending.
