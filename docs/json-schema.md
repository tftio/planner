# Planner JSON diagnostic and inspection schemas

Planner exposes JSON only as an external compatibility interface. JSON is not the
canonical planning-document representation, an internal serialization format, or a
persistence protocol. Schema changes are additive within a schema version; removing or
renaming a documented field requires a new `schema_version`.

## Validation diagnostics

`planner validate --format json` returns an array with one object per input path:

- `path`: input path string.
- `valid`: boolean; warnings do not make it false.
- `diagnostics`: ordered array of diagnostic objects.

Each diagnostic contains `code`, `severity`, `message`, and nullable `location`.
Locations contain one-based `line` and `column` plus a half-open byte `span` with
`start` and `end`.

## Inspection schema version 1

`planner inspect --format json` returns an object with:

- `schema_version`: integer `1`.
- `role`: `operator` or `worker`.
- `plan`: the complete role-appropriate plan representation.

Both roles contain `metadata`, `adr`, `tasks`, and `execution_protocol`. The operator
role additionally contains `operator_guidance_log` and `decision_log`; its tasks contain
`hidden_criteria`. Worker JSON cannot contain `hidden_criteria`, `why_hidden`,
`counterfactual`, or hidden evaluator/check/ask data because it is rendered from
`WorkerPlan`, whose task type has no hidden-criterion field.

`metadata.project` is an object `{slug, remote}` naming the fleet project (from
`tftio_lib::project`) the plan belongs to. It is present, in both roles, only when the
plan carries one — today only a v2 plan can — and absent otherwise; a consumer must not
mistake absence for `null`. `remote` is `null` when the plan records a slug but no
remote.

## Mutation change-report schema version 1

Every lifecycle or append command accepts `--format json`. Its change report contains:

- `schema_version`: integer `1`.
- `path`: the exact operator-supplied plan path.
- `status`: `dry_run` when no write occurred or `applied` after atomic replacement.
- `summary`: a stable description of the accepted domain transition.
- `changed`: whether the prepared replacement differs from the captured source.
- `diff`: a unified whole-document diff whose added side is the exact canonical
  replacement.

The JSON report is diagnostic output rather than plan storage. A dry run and a later
application prepared from identical source bytes produce the same replacement diff. An
application rechecks the captured source immediately before same-directory atomic
replacement and returns a conflict instead of writing when those bytes changed.

## Metadata reports

`planner version --format json` returns `schema_version`, `name`, and `version`.
`planner doctor --format json` uses the shared `tftio-lib` doctor envelope with `ok`,
`header`, `checks`, `errors`, `warnings`, `info`, and `version`. Doctor checks exercise
the embedded format authority, compatibility example validation, and Markdown semantic
round trip without reading or changing a plan file.
