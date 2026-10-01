---
plan_format_version: 2
plan_id: PLAN-20260804-cache-session-lookups
title: Stop re-reading the session row on every authenticated request
status: approved
created_at: 2026-08-04
updated_at: 2026-08-11
owner: platform-team
source:
  type: github
  url: https://github.com/example/gateway/issues/812
  external_id: "812"
  imported_at: 2026-08-04
bug:
  summary: >
    Every authenticated request reads the session row from the primary database,
    so a burst of traffic from one tenant saturates the primary's connection pool
    and slows every other tenant's writes.
  severity: high
  affected_area: gateway/src/auth/session.rs
  user_impact: >
    p99 write latency rises from 40ms to over 2s during a burst, and the gateway
    sheds load for tenants that sent no extra traffic.
execution:
  requires_operator_approval_before_implementation: true
  requires_plan_updates_during_execution: true
  task_graph_status: executing
project:
  slug: gateway
  remote: github.com/example/gateway
---

# ADR: Stop re-reading the session row on every authenticated request

## Problem Statement

The gateway resolves a bearer token to a session by reading `sessions` on the primary
database, once per request, with no cache. A session row changes only when someone signs
in, signs out, or an administrator revokes a session, so the read is almost always a
repeat of the previous one.

Under a burst from a single tenant the repeated reads exhaust the primary's connection
pool. Writes from unrelated tenants then queue behind them, which is why the symptom
reported in issue 812 is slow writes for tenants that sent no extra traffic.

## Source Material

### Ticket

`https://github.com/example/gateway/issues/812`, "write latency spikes when one tenant
bursts", opened 2026-08-01 with three tenants attached as affected.

### Discussion Summary

The 2026-08-03 incident review established that the primary's connection pool, not disk
or CPU, is the saturated resource, and that session reads are 78% of pool acquisitions
by count. Two directions were raised: cache the session, or move the read to a replica.
The reviewers preferred caching because a replica read still costs a pool acquisition on
the replica and adds replication lag to revocation.

## Context

**Existing behaviour.** `resolve_session` in `gateway/src/auth/session.rs` executes
`SELECT ... FROM sessions WHERE token_hash = $1` against the primary on every request and
returns the row or a 401.

**Expected behaviour.** A session resolves from an in-process cache when it is present
and unexpired; the database is read only on a miss. Revocation invalidates the cached
entry across every gateway process before the next request is served.

**Observed failure.** During the 2026-08-01 burst the primary's pool sat at its limit of
200 for 14 minutes and p99 write latency reached 2.3s. `pg_stat_activity` sampled during
the window showed session lookups holding 156 of the 200 connections.

**Affected systems.** The gateway's auth path, the revocation endpoint, and the metrics
that report cache behaviour.

## Constraints

- Revocation must take effect within one second across every gateway process. A cache
  that serves a revoked session is a security regression, not a performance tradeoff.
- The cache is per-process and bounded. The gateway runs with a fixed memory budget and
  an unbounded cache converts a traffic burst into an out-of-memory kill.
- No new infrastructure dependency. Adding a shared cache tier is a larger change than
  this incident justifies and is recorded as an alternative rather than folded in.

## Non-Goals

- Caching anything other than the session row.
- Changing the session lifetime or the token format.
- Moving any read to a replica. The discussion rejected it; it is recorded under
  Alternatives Considered.

## Decision

Add a bounded, per-process cache in front of `resolve_session`, keyed by token hash, and
invalidate it on revocation through the existing pub/sub channel the gateway already uses
for configuration reloads.

Entries expire at the session's own expiry or after 60 seconds, whichever is sooner. The
60-second ceiling bounds the staleness of anything the invalidation path misses, so a
dropped pub/sub message degrades to a one-minute delay rather than to an unbounded one.

Revocation publishes the token hash; every process drops the entry on receipt. The
publisher writes to the database first and publishes second, so a process that misses the
message and falls back to a read gets the revoked state.

## Alternatives Considered

**Read from a replica instead of caching.** Rejected during the incident review: it still
costs a connection per request, on the replica's pool rather than the primary's, and it
adds replication lag to the revocation path, which the first constraint forbids.

**A shared cache tier.** Correct at a larger scale and rejected here as disproportionate:
it adds an operational dependency to the request path, and the failure mode of that
dependency is the same saturation this plan is fixing.

**Cache without invalidation, relying on a short TTL alone.** Simpler, and rejected
because the acceptable revocation delay is one second and a TTL that short removes most
of the benefit.

## Consequences

**Expected.** Session reads fall to roughly one per session per minute per process. The
primary's pool stops being the constraint during a single-tenant burst.

**Risks.** A cache in the auth path is a correctness surface: the failure mode of a bug
here is serving a revoked session, which is why T003 exists and why its acceptance checks
are about revocation rather than about hit rate.

**Follow-up.** If the per-process hit rate proves low because traffic is spread thinly
across processes, the shared cache tier rejected above becomes worth reopening.

## Operator Guidance Log

Append-only. Agents add a dated entry whenever they need operator guidance or operator
guidance changes the plan.

### 2026-08-07 — platform-team (operator)

Question or decision needed: whether a missed invalidation message should fail the
request closed or serve the cached entry until it expires.

Decision or guidance received: serve the cached entry, and bound the staleness with the
60-second ceiling. Failing closed on a dropped message makes the pub/sub channel a
dependency of every authenticated request, which is the coupling this plan avoids.

Impact on plan: T002's second invariant states the ceiling, and T003 checks the
one-minute worst case rather than only the sub-second normal case.

## Decision Log

Append-only. Decisions affecting architecture, behavior, sequencing, or tradeoffs.

### 2026-08-05 — D001: the cache is keyed by token hash, not by session id

Recorded while executing T001.

The revocation endpoint is given a session id, but the request path has only a token. A
session-id key would force a second lookup to invalidate, which is a database read on the
path this plan exists to relieve. The revocation publisher resolves the id to the hash
once, at revocation time.

### 2026-08-11 — D002: write to the database before publishing, not after

Recorded while executing T002.

Publishing first leaves a window in which a process drops the entry, re-reads, and caches
the pre-revocation row. Writing first makes the fallback read return the revoked state,
so a dropped message costs latency rather than correctness.

# Task Graph

<!-- TASK_GRAPH:BEGIN -->
```yaml
tasks:
  - id: T001
    title: Add the bounded cache behind resolve_session
    status: done
    depends_on: []
    owner: platform-team
    description: >
      Introduce a bounded per-process cache keyed by token hash in front of
      resolve_session, with no invalidation path yet. The cache is inert until
      T002 wires revocation to it, so this task can land without changing
      revocation behaviour.
    work_items:
      - Add the cache type with a fixed capacity and a per-entry expiry.
      - Call it from resolve_session on the read path, falling back to the query.
      - Emit hit, miss, and eviction counters.
    invariants:
      - The cache never grows beyond its configured capacity.
      - A miss returns exactly what the uncached path returned.
    acceptance_checks:
      - A test fills the cache past capacity and asserts the bound holds.
      - A test asserts a miss and a hit return the same session.
    completion_evidence: >
      Done 2026-08-06 in commit 4f2a1c8, "feat(auth): cache resolved sessions".
      Files actually touched: gateway/src/auth/cache.rs (new, the bounded map),
      gateway/src/auth/session.rs:88 (the resolve_session read path),
      gateway/src/metrics.rs:41 (the three counters), and
      gateway/tests/session_cache.rs (new). gateway/src/auth/mod.rs was read but
      not changed. Capacity is 50,000 entries, measured at 6.1MB full. cargo test
      passes, 214 tests. The bound test fills to 60,000 and asserts len() ==
      50,000; the equivalence test asserts the cached and uncached paths return
      the same row for 100 generated sessions. Keyed by token hash per D001.
  - id: T002
    title: Invalidate on revocation through the configuration pub/sub channel
    status: in_progress
    depends_on:
      - T001
    owner: platform-team
    description: >
      Publish the revoked token hash on the existing pub/sub channel and drop the
      matching cache entry in every process on receipt. The database write happens
      before the publish, so a dropped message degrades to staleness bounded by the
      60-second ceiling rather than to a revoked session being served indefinitely.
    work_items:
      - Resolve the session id to its token hash in the revocation handler.
      - Write the revocation to the database, then publish the hash.
      - Drop the entry on receipt in every subscriber.
    invariants:
      - The database write precedes the publish, never the reverse.
      - No cached entry outlives 60 seconds, whatever the pub/sub channel does.
    acceptance_checks:
      - A test revokes a session and asserts a second process stops serving it.
      - A test drops the message and asserts the entry expires within 60 seconds.
    completion_evidence: null
  - id: T003
    title: Prove revocation under a partitioned pub/sub channel
    status: not_started
    depends_on:
      - T002
    owner: null
    description: >
      Exercise the failure the constraints are written against: a process that
      never receives the revocation message. The check is that the staleness
      ceiling holds, not that the message arrives.
    work_items:
      - Partition one subscriber and revoke a session it has cached.
      - Assert the partitioned process stops serving the session within 60 seconds.
      - Record the observed worst-case staleness.
    invariants:
      - The test asserts an observed outcome, not a mocked expiry.
    acceptance_checks:
      - The partitioned subscriber stops serving the revoked session within 60s.
      - The recorded worst-case staleness is in the completion evidence.
    hidden_criteria:
      - claim: The test fails when the expiry ceiling is removed.
        criticality: must
        evaluator: agent_evaluated
        check: Remove the 60-second ceiling and confirm the test fails.
        why_hidden: >
          A worker told the mutation check exists can special-case the ceiling
          rather than write a test that depends on it.
        counterfactual: >
          A worker given this claim could assert on the constant directly and
          pass without exercising expiry at all.
    completion_evidence: null
```
<!-- TASK_GRAPH:END -->

# Task Details

## T001 — Add the bounded cache behind resolve_session

Status: `done`

Depends on: none

### Description

Introduce a bounded per-process cache keyed by token hash in front of `resolve_session`,
with no invalidation path yet.

### Invariants

- The cache never grows beyond its configured capacity.
- A miss returns exactly what the uncached path returned.

### Acceptance Checks

- A test fills the cache past capacity and asserts the bound holds.
- A test asserts a miss and a hit return the same session.

### Completion Evidence

Done 2026-08-06 in commit 4f2a1c8. See the task's `completion_evidence` in the graph for
the capacity, the measured footprint, and the two tests.

## T002 — Invalidate on revocation through the configuration pub/sub channel

Status: `in_progress`

Depends on: T001

### Description

Publish the revoked token hash on the existing pub/sub channel and drop the matching
cache entry in every process on receipt.

### Invariants

- The database write precedes the publish, never the reverse.
- No cached entry outlives 60 seconds, whatever the pub/sub channel does.

### Acceptance Checks

- A test revokes a session and asserts a second process stops serving it.
- A test drops the message and asserts the entry expires within 60 seconds.

### Completion Evidence

Agents update this when the task is complete.

## T003 — Prove revocation under a partitioned pub/sub channel

Status: `not_started`

Depends on: T002

### Description

Exercise the failure the constraints are written against: a process that never receives
the revocation message.

### Invariants

- The test asserts an observed outcome, not a mocked expiry.

### Acceptance Checks

- The partitioned subscriber stops serving the revoked session within 60s.
- The recorded worst-case staleness is in the completion evidence.

### Completion Evidence

Agents update this when the task is complete.

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
- Name, in the completion evidence, the files the task actually touched. A path and a
  line number beats a filename.
- Treat the Operator Guidance Log, Decision Log, and completion evidence as append-only;
  supersede prior entries with a new dated entry rather than deleting them.
