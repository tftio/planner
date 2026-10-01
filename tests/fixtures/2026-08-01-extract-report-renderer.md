---
plan_format_version: 1
plan_id: PLAN-20260801-extract-report-renderer
title: Extract the report renderer into a shared library
status: implemented
mode: multi
created_at: 2026-08-01
updated_at: 2026-08-20
owner: reporting-team
source:
  type: manual
  url: null
  external_id: null
  imported_at: null
bug:
  summary: >
    Report rendering is duplicated across the web service, the batch exporter, and a
    command-line tool, and the three copies have drifted in how they format totals.
  severity: medium
  affected_area: >
    web/src/reports, exporter/src/render.rs, cli/src/print.rs, and future report
    consumers
  user_impact: >
    The same report prints different totals depending on which tool produced it, so
    users cannot trust an exported report to match the one they saw in the browser.
execution:
  requires_operator_approval_before_implementation: true
  requires_plan_updates_during_execution: true
  task_graph_status: complete
---

# ADR: Extract report rendering into one shared library

## Problem Statement

Three tools render the same reports, each with its own copy of the formatting code.
The copies have drifted: the batch exporter rounds totals per row, while the web
service rounds only the grand total, so an exported report and the browser view of the
same data disagree in the last digit.

## Source Material

### Ticket

No ticket was filed. The source is a support escalation and the discussion below.

### Discussion Summary

A customer reported that a monthly export did not match the dashboard. Investigation
found the rounding difference and two further divergences in date formatting. The
operator asked for one renderer that every tool calls, rather than a fix to each copy.

## Context

Existing behavior: each tool formats reports independently. Expected behavior: every
tool produces byte-identical output for the same input. Observed failure: totals and
dates differ between tools.

## Constraints

- Output for the web service must not change, since it is the reference users see.
- The shared library must not depend on any one tool's runtime.
- No new third-party dependencies without operator approval.

## Non-Goals

- Redesigning the report layout.
- Adding new report types.

## Decision

Extract the web service's renderer into a library crate, prove its output against the
existing web fixtures, and migrate the exporter and the command-line tool to it one at
a time. Retire each tool's private copy only after its migration passes.

## Alternatives Considered

- Fix the rounding in the exporter only. Considered because it is the smallest change;
  rejected because the date divergences would remain and the copies would drift again.
- Generate reports in one service and have the others call it over HTTP. Considered for
  its single source of truth; rejected because the exporter runs offline.

## Consequences

Report output becomes consistent across tools. Exporter output changes in the last
digit of some totals; this is the intended fix. Follow-up: publish the library's
fixtures so a future consumer can check itself against them.

## Operator Guidance Log

### 2026-08-01 09:00 UTC — planning agent

Question or decision needed: May the exporter's output change where it currently
disagrees with the web service?

Decision or guidance received: Pending.

Impact on plan: T003 is blocked until answered.

### 2026-08-01 11:30 UTC — operator

Question or decision needed: Exporter output changes.

Decision or guidance received: Yes. The web service is the reference; the exporter
must match it.

Impact on plan: T003 is unblocked.

### 2026-08-12 — operator

Question or decision needed: The command-line tool has two users and is scheduled for
removal. Should it still be migrated?

Decision or guidance received: No. Drop the migration; the tool is removed next
quarter and its private renderer goes with it.

Impact on plan: T004 is abandoned. T005 no longer depends on it.

## Decision Log

- 2026-08-01 — Extract the web service's renderer rather than writing a new one, so
  the reference output is preserved by construction.
- 2026-08-05 — Represent money as integer minor units inside the library and round only
  at presentation, which removes the per-row rounding difference.
- 2026-08-12 — Abandon the command-line migration (see the 2026-08-12 guidance entry)
  and record its private renderer as a known exception.

# Task Graph

<!-- TASK_GRAPH:BEGIN -->
```yaml
tasks:
  - id: T001
    title: Extract the web renderer into a library crate
    status: done
    owner: null
    depends_on: []
    blocks: [T002]
    description: >
      Move the web service's report renderer into a new library crate with no
      dependency on the web runtime, keeping its public behaviour unchanged.
    work_items:
      - Create the library crate and move the renderer into it.
      - Replace web runtime types in the renderer's signature with plain data types.
      - Point the web service at the library.
    invariants:
      - Web service output is byte-identical before and after the move.
      - The library has no dependency on the web runtime.
    acceptance_checks:
      - The web service's report snapshot tests pass unchanged.
      - The library builds on its own.
    files:
      likely_read:
        - web/src/reports/render.rs
      likely_modify:
        - web/src/reports/render.rs
        - report-render/src/lib.rs
        - report-render/Cargo.toml
    completion_evidence: >
      Moved the renderer into report-render with plain input types; the web snapshot
      suite passed unchanged on 2026-08-04.
  - id: T002
    title: Prove the library against the reference fixtures
    status: done
    owner: null
    depends_on: [T001]
    blocks: [T003]
    description: >
      Build a fixture corpus from the web service's reference reports and run the
      library against it, so every later migration has one oracle to match.
    work_items:
      - Export a representative set of reference reports as fixtures.
      - Add a test that renders each fixture's input and compares it to the output.
    invariants:
      - Fixtures come from the reference renderer, never from a tool being migrated.
    acceptance_checks:
      - Every fixture renders byte-identically.
    files:
      likely_read:
        - web/tests/snapshots
      likely_modify:
        - report-render/tests/fixtures
        - report-render/tests/reference.rs
    completion_evidence: >
      Added 42 reference fixtures covering every report type; all render identically.
      Integer minor units replaced floating-point totals inside the library (Decision
      Log, 2026-08-05) with no fixture change.
  - id: T003
    title: Migrate the batch exporter
    status: done
    owner: null
    depends_on: [T002]
    blocks: [T005]
    description: >
      Replace the exporter's private renderer with calls to the library and delete the
      private copy.
    work_items:
      - Call the library from the exporter's render step.
      - Delete exporter/src/render.rs.
      - Record the expected output changes in the exporter's release notes.
    invariants:
      - The exporter still runs without network access.
    acceptance_checks:
      - The exporter's output for the reference inputs matches the fixtures.
      - The exporter's own test suite passes.
    files:
      likely_read:
        - exporter/src/render.rs
      likely_modify:
        - exporter/src/main.rs
        - exporter/src/render.rs
        - exporter/CHANGELOG.md
    completion_evidence: >
      The exporter now renders through the library; its private renderer is deleted.
      Output for the reference inputs matches the fixtures. Totals changed in the last
      digit for 3 of 42 fixtures, as expected, and the release notes say so.
  - id: T004
    title: Migrate the command-line tool
    status: abandoned
    owner: null
    depends_on: [T002]
    blocks: []
    description: >
      Replace the command-line tool's private renderer with calls to the library.
    work_items:
      - Call the library from the print command.
      - Delete cli/src/print.rs.
    invariants:
      - The tool's flags and exit codes are unchanged.
    acceptance_checks:
      - The tool's output for the reference inputs matches the fixtures.
    files:
      likely_read:
        - cli/src/print.rs
      likely_modify:
        - cli/src/print.rs
    completion_evidence: >
      Abandoned 2026-08-12 at the operator's direction. The tool is scheduled for
      removal and has two users, so migrating it would spend effort on code about to
      be deleted. Known exception: cli/src/print.rs keeps its own renderer until the
      tool is removed.
  - id: T005
    title: Prove the cutover and retire duplicate code
    status: done
    owner: null
    depends_on: [T003]
    blocks: []
    description: >
      Confirm every remaining consumer renders through the library and that no second
      renderer remains outside the recorded exception.
      Amended 2026-08-12. T004 was dropped, so the command-line tool is out of this
      cutover and its renderer is a recorded exception.
    work_items:
      - Search the repositories for remaining report-formatting code.
      - Run every consumer against the reference fixtures.
    invariants:
      - The only renderer outside the library is the recorded exception.
    acceptance_checks:
      - The search finds no report-formatting code outside the library and the exception.
      - Every consumer matches the fixtures.
    files:
      likely_read:
        - web/src/reports
        - exporter/src
      likely_modify: []
    completion_evidence: >
      The search found formatting code only in report-render and cli/src/print.rs.
      The web service and the exporter both match all 42 fixtures.
```
<!-- TASK_GRAPH:END -->

# Task Details

## T001 — Extract the web renderer into a library crate

Status: `done`

Depends on: none

Blocks: T002

### Description

Move the web service's report renderer into a new library crate with no dependency on
the web runtime.

### Invariants

- Web service output is byte-identical before and after the move.
- The library has no dependency on the web runtime.

### Acceptance Checks

- The web service's report snapshot tests pass unchanged.
- The library builds on its own.

### Completion Evidence

Moved the renderer into `report-render`; the web snapshot suite passed unchanged.

## T002 — Prove the library against the reference fixtures

Status: `done`

Depends on: T001

Blocks: T003

### Description

Build a fixture corpus from the web service's reference reports and run the library
against it.

### Invariants

- Fixtures come from the reference renderer, never from a tool being migrated.

### Acceptance Checks

- Every fixture renders byte-identically.

### Completion Evidence

Added 42 reference fixtures; all render identically.

## T003 — Migrate the batch exporter

Status: `done`

Depends on: T002

Blocks: T005

### Description

Replace the exporter's private renderer with calls to the library and delete the
private copy.

### Invariants

- The exporter still runs without network access.

### Acceptance Checks

- The exporter's output for the reference inputs matches the fixtures.
- The exporter's own test suite passes.

### Completion Evidence

The exporter renders through the library and matches the fixtures.

## T004 — Migrate the command-line tool

Status: `abandoned`

Depends on: T002

Blocks: none

### Description

Replace the command-line tool's private renderer with calls to the library.

### Invariants

- The tool's flags and exit codes are unchanged.

### Acceptance Checks

- The tool's output for the reference inputs matches the fixtures.

### Completion Evidence

Abandoned 2026-08-12 at the operator's direction; the tool is scheduled for removal.

## T005 — Prove the cutover and retire duplicate code

Status: `done`

Depends on: T003

Blocks: none

### Description

Confirm every remaining consumer renders through the library and that no second
renderer remains outside the recorded exception.

### Invariants

- The only renderer outside the library is the recorded exception.

### Acceptance Checks

- The search finds no report-formatting code outside the library and the exception.
- Every consumer matches the fixtures.

### Completion Evidence

Only the library and the recorded exception contain report-formatting code.

# Execution Protocol

Agents executing this plan must:

- Read the full plan and verify a task's dependencies are `done` before starting it.
- Set a task to `in_progress` and update `updated_at` when starting.
- Update the plan when they find it wrong, discover missing context, change the
  approach, need operator guidance, or alter the task graph.
- When operator guidance is needed: add an Operator Guidance Log entry, set affected
  tasks to `blocked`, and not guess when guessing risks rework, data loss, security, or
  product-behavior changes.
- Before an external or irreversible operation, show the exact command and destination
  and wait for explicit operator approval; planning or implementation approval does not
  authorize repository creation, credential changes, commits, pushes, publication, or
  releases.
- When completing a task: verify invariants, run or describe acceptance checks, record
  `completion_evidence`, set status `done`, update `updated_at`, and update ADR sections
  only when the task changed context, decision, alternatives, or consequences.
- Treat the Operator Guidance Log, Decision Log, and completion evidence as append-only;
  supersede prior entries with a new dated entry rather than deleting them.
