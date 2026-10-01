# planner

Authoritative planning-document library and CLI

## Getting started

Toolchain, task execution, and hook tools are managed by mise.
The Rust toolchain is declared in `mise.toml`. `rust-toolchain.toml` is derived
from that declaration by `mise run update` and checked against it by
`mise run check:locks`, so rustup, rust-analyzer, and your IDE resolve the same
pin mise does. Never edit it by hand.
Entering the directory does not prepare, install, or regenerate anything: setup is
explicit, so no lockfile ever changes because someone walked into the repository.

```sh
mise trust --quiet
mise install
mise run setup:idea     # optional; regenerates the gitignored .idea/
```

Tools come from `mise activate <shell>` in an interactive shell, and from mise shims
for non-interactive processes such as editors and coding agents.

## Installation

planner is distributed from its GitHub repository; it is not published to crates.io.
Install the CLI with cargo:

```sh
cargo install --locked --git https://github.com/tftio/planner
cargo install --locked --git https://github.com/tftio/planner --tag v0.5.3  # a specific release
```

To build from a checkout, run `cargo build --locked`.

## Architecture

`tftio_planner::model` is the representation-independent planning domain. It contains no
Markdown, YAML, JSON, paths, source locations, or filesystem state. Explicit adapters
parse Markdown into `OperatorPlan`, render canonical Markdown from that model, expose
diagnostic JSON, and derive a distinct `WorkerPlan` type that cannot contain hidden
criteria. Adapter idempotency is semantic: parsing canonical rendered Markdown returns
the same model, while input whitespace and layout need not be preserved.

Lifecycle transitions are pure functions in `state`; `mutate` prepares and revalidates
canonical Markdown without I/O; and `write` compares captured bytes twice before a
same-directory atomic replacement. A failed or stale mutation leaves the exact source
unchanged. See [JSON schemas](docs/json-schema.md) for the external diagnostic contract.

## CLI

```sh
planner validate --format human docs/plans/example.md
planner inspect --role operator --format json docs/plans/example.md
planner project docs/plans/example.md > /tmp/worker-plan.md
planner spec show
planner template show
planner init docs/plans/2026-08-28-example.md

planner plan approve docs/plans/example.md --dry-run
planner task ready docs/plans/example.md T001 --dry-run
planner task start docs/plans/example.md T001
planner task complete docs/plans/example.md T001 --evidence "mise run ci passed"
planner guidance add docs/plans/example.md --file guidance.md
planner decision add docs/plans/example.md --stdin

planner doctor --format json
planner completions zsh
planner version --format json
```

Every mutation supports `--dry-run`, `--format human|json`, and an optional deterministic
`--date YYYY-MM-DD`. Human dry runs emit a unified diff; JSON reports carry the exact
same diff. Without `--date`, planner records the current local date.

## Consumers

planner is the only validator and the only copy of the specification and template.
Consumers read them through `planner spec show` and `planner template show` rather than
carrying their own.

A Rust consumer uses the `tftio_planner` library as a git dependency pinned to a release
tag:

```toml
[dependencies]
tftio-planner = { git = "https://github.com/tftio/planner", tag = "v0.5.3" }
```

A consumer of the CLI pins it the same way, by tag or commit, through
`cargo install --git` or a tool manager's cargo backend. A plan document records no
planner version, so a consumer rolls back by reverting its pin, without editing any plan.
The fixture manifest under `tests/fixtures/format/` can be run against a pinned build to
confirm it agrees with this repository's validator.

## Tasks

```sh
mise run check  # check-only hooks, as CI runs them
mise run lint   # manual autofix hooks
mise run test   # test suite
mise run ci     # full CI gate
```

Dependencies move on one deliberate command, and never on their own:

```sh
mise run update       # mise tools, cargo crates, prek hooks
mise run check:locks  # read-only; fails if a lockfile is stale
```

The generated Rust gate includes formatting, TOML formatting, shell linting,
spelling, clippy, nextest, docs, unused-dependency detection, advisory audit,
license/source policy, packaging, and complete line coverage.
