//! Command-line interface for `planner`.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use tftio_planner::model::TaskId;
use tftio_planner::{
    Diagnostic, DiagnosticSeverity, Mutation, MutationRequest, PlanAction, TaskAction, WorkerPlan,
};

/// Command-line arguments for `planner`.
#[derive(Debug, Parser)]
#[command(name = "planner", author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Planner operations.
#[derive(Debug, Subcommand)]
enum Command {
    /// Validate one or more Planning Document Format files.
    Validate {
        /// Diagnostic output representation.
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
        /// Plan files to validate.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Inspect a validated plan without modifying it.
    Inspect {
        /// Inspection role and concealment policy.
        #[arg(long, value_enum, default_value_t = InspectRole::Operator)]
        role: InspectRole,
        /// Inspection output representation.
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
        /// Plan file to inspect.
        path: PathBuf,
    },
    /// Render worker-safe Markdown to standard output.
    Project {
        /// Plan file to project.
        path: PathBuf,
    },
    /// Access the embedded format specification.
    Spec {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Access the embedded authoring template.
    Template {
        #[command(subcommand)]
        command: ResourceCommand,
    },
    /// Create a new plan file from the embedded template without overwriting.
    Init {
        /// Destination path.
        path: PathBuf,
        /// Fleet project slug to pre-fill under `project.slug`, validated
        /// against `tftio_lib::project`'s shared slug grammar.
        #[arg(long)]
        project: Option<String>,
    },
    /// Apply a task lifecycle transition.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Apply a plan lifecycle transition.
    Plan {
        #[command(subcommand)]
        command: PlanCommand,
    },
    /// Append operator-guidance history.
    Guidance {
        #[command(subcommand)]
        command: EntryCommand,
    },
    /// Append decision history.
    Decision {
        #[command(subcommand)]
        command: EntryCommand,
    },
    /// Generate shell completion instructions and a completion script.
    Completions {
        /// Target shell.
        shell: clap_complete::Shell,
    },
    /// Check embedded assets, validation, and semantic round trips.
    Doctor {
        /// Health-report representation.
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
    /// Print planner version metadata.
    Version {
        /// Version-report representation.
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

/// Embedded-resource operations.
#[derive(Debug, Subcommand)]
enum ResourceCommand {
    /// Print the embedded resource.
    Show,
}

/// Shared mutation output and write controls.
#[derive(Debug, Clone, Args)]
struct MutationOptions {
    /// Preview the exact replacement without modifying the plan.
    #[arg(long)]
    dry_run: bool,
    /// Change-report representation.
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,
    /// Mutation date; defaults to the current local date.
    #[arg(long)]
    date: Option<String>,
}

/// Task lifecycle operations.
#[derive(Debug, Subcommand)]
enum TaskCommand {
    /// Move a not-started task to ready after its dependencies complete.
    Ready {
        /// Exact plan path.
        path: PathBuf,
        /// Target task id.
        task_id: String,
        #[command(flatten)]
        options: MutationOptions,
    },
    /// Start a ready task.
    Start {
        /// Exact plan path.
        path: PathBuf,
        /// Target task id.
        task_id: String,
        #[command(flatten)]
        options: MutationOptions,
    },
    /// Block an active task.
    Block {
        /// Exact plan path.
        path: PathBuf,
        /// Target task id.
        task_id: String,
        /// Required reason appended to task evidence history.
        #[arg(long)]
        reason: String,
        #[command(flatten)]
        options: MutationOptions,
    },
    /// Complete an in-progress task.
    Complete {
        /// Exact plan path.
        path: PathBuf,
        /// Target task id.
        task_id: String,
        /// Required completion evidence appended to prior evidence.
        #[arg(long)]
        evidence: String,
        #[command(flatten)]
        options: MutationOptions,
    },
    /// Reopen a blocked, completed, or abandoned task as ready.
    Reopen {
        /// Exact plan path.
        path: PathBuf,
        /// Target task id.
        task_id: String,
        /// Required reason appended to task evidence history.
        #[arg(long)]
        reason: String,
        #[command(flatten)]
        options: MutationOptions,
    },
    /// Abandon a nonterminal task.
    Abandon {
        /// Exact plan path.
        path: PathBuf,
        /// Target task id.
        task_id: String,
        /// Required reason appended to task evidence history.
        #[arg(long)]
        reason: String,
        #[command(flatten)]
        options: MutationOptions,
    },
}

/// Plan lifecycle operations.
#[derive(Debug, Subcommand)]
enum PlanCommand {
    /// Approve a draft plan.
    Approve {
        /// Exact plan path.
        path: PathBuf,
        #[command(flatten)]
        options: MutationOptions,
    },
    /// Mark an approved plan implemented after all tasks are terminal.
    Implement {
        /// Exact plan path.
        path: PathBuf,
        #[command(flatten)]
        options: MutationOptions,
    },
    /// Abandon a draft or approved plan.
    Abandon {
        /// Exact plan path.
        path: PathBuf,
        /// Required reason appended to decision history.
        #[arg(long)]
        reason: String,
        #[command(flatten)]
        options: MutationOptions,
    },
}

/// Append-only log operations.
#[derive(Debug, Subcommand)]
enum EntryCommand {
    /// Append one entry from a file or standard input.
    #[command(group(
        ArgGroup::new("entry_source")
            .required(true)
            .args(["file", "stdin"])
    ))]
    Add {
        /// Exact plan path.
        path: PathBuf,
        /// Read the entry body from this file.
        #[arg(long, conflicts_with = "stdin")]
        file: Option<PathBuf>,
        /// Read the entry body from standard input.
        #[arg(long, conflicts_with = "file")]
        stdin: bool,
        #[command(flatten)]
        options: MutationOptions,
    },
}

/// CLI diagnostic representation.
#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    /// Line-oriented diagnostics for people.
    Human,
    /// Stable structured diagnostics for automation.
    Json,
}

/// Inspection concealment role.
#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
enum InspectRole {
    /// Full operator view, including hidden criteria.
    Operator,
    /// Worker-safe view with hidden criteria absent by construction.
    Worker,
}

#[derive(Debug)]
struct DocumentResult {
    path: PathBuf,
    diagnostics: Vec<Diagnostic>,
    valid: bool,
}

fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { format, paths } => validate_paths(format, &paths),
        Command::Inspect { role, format, path } => inspect_path(&path, role, format),
        Command::Project { path } => project_path(&path),
        Command::Spec {
            command: ResourceCommand::Show,
        } => show_resource(tftio_planner::resources::PLAN_SPEC),
        Command::Template {
            command: ResourceCommand::Show,
        } => show_resource(tftio_planner::resources::PLAN_TEMPLATE),
        Command::Init { path, project } => initialize(&path, project.as_deref()),
        Command::Task { command } => execute_task_command(command),
        Command::Plan { command } => execute_plan_command(command),
        Command::Guidance { command } => execute_entry_command(command, EntryKind::Guidance),
        Command::Decision { command } => execute_entry_command(command, EntryKind::Decision),
        Command::Completions { shell } => show_completions(shell),
        Command::Doctor { format } => show_doctor(format),
        Command::Version { format } => show_version(format),
    }
}

fn inspect_path(path: &Path, role: InspectRole, format: OutputFormat) -> anyhow::Result<ExitCode> {
    let source = std::fs::read_to_string(path)?;
    let report = tftio_planner::validate_markdown_path(&source, path)?;
    if !report.is_valid() {
        anyhow::bail!("planning document is invalid");
    }
    let plan = tftio_planner::parse_markdown(&source)?;
    let output = match (role, format) {
        (InspectRole::Operator, OutputFormat::Human) => {
            tftio_planner::inspect::operator_summary(&plan)
        }
        (InspectRole::Operator, OutputFormat::Json) => {
            tftio_planner::inspect::operator_json(&plan)?
        }
        (InspectRole::Worker, OutputFormat::Human) => {
            tftio_planner::inspect::worker_summary(&WorkerPlan::from(&plan))
        }
        (InspectRole::Worker, OutputFormat::Json) => {
            tftio_planner::inspect::worker_json(&WorkerPlan::from(&plan))?
        }
    };
    writeln!(std::io::stdout().lock(), "{output}")?;
    Ok(ExitCode::SUCCESS)
}

fn project_path(path: &Path) -> anyhow::Result<ExitCode> {
    let source = std::fs::read_to_string(path)?;
    let projected = tftio_planner::project_worker_markdown(&source)?;
    write!(std::io::stdout().lock(), "{projected}")?;
    Ok(ExitCode::SUCCESS)
}

fn show_resource(resource: &str) -> anyhow::Result<ExitCode> {
    write!(std::io::stdout().lock(), "{resource}")?;
    Ok(ExitCode::SUCCESS)
}

fn show_completions(shell: clap_complete::Shell) -> anyhow::Result<ExitCode> {
    let output = tftio_lib::render_completion::<Cli>(shell);
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(output.instructions.as_bytes())?;
    stdout.write_all(output.script.as_bytes())?;
    Ok(ExitCode::SUCCESS)
}

fn show_doctor(format: OutputFormat) -> anyhow::Result<ExitCode> {
    let report = tftio_planner::doctor_report();
    match format {
        OutputFormat::Human => write!(std::io::stdout().lock(), "{}", report.render_text())?,
        OutputFormat::Json => {
            let output = serde_json::to_string_pretty(&report.to_json_value())?;
            writeln!(std::io::stdout().lock(), "{output}")?;
        }
    }
    Ok(ExitCode::from(u8::from(report.exit_code() != 0)))
}

fn show_version(format: OutputFormat) -> anyhow::Result<ExitCode> {
    let version = env!("CARGO_PKG_VERSION");
    match format {
        OutputFormat::Human => writeln!(std::io::stdout().lock(), "planner {version}")?,
        OutputFormat::Json => {
            let output = json!({
                "schema_version": 1,
                "name": "planner",
                "version": version,
            });
            let output = serde_json::to_string_pretty(&output)?;
            writeln!(std::io::stdout().lock(), "{output}")?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn initialize(path: &Path, project: Option<&str>) -> anyhow::Result<ExitCode> {
    let contents = match project {
        None => tftio_planner::resources::PLAN_TEMPLATE.to_owned(),
        Some(slug) => {
            let slug = tftio_lib::project::Slug::new(slug)?;
            let filled = format!("project:\n  slug: {slug}\n  remote: null\n");
            let (replaced, count) = replace_once(
                tftio_planner::resources::PLAN_TEMPLATE,
                "project: null\n",
                &filled,
            );
            anyhow::ensure!(
                count == 1,
                "embedded template does not carry exactly one 'project: null' key to fill"
            );
            replaced
        }
    };
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(contents.as_bytes())?;
    Ok(ExitCode::SUCCESS)
}

/// Replace the first occurrence of `find` in `source` with `replacement`,
/// reporting how many occurrences were found so a caller that requires
/// exactly one can fail loudly instead of silently filling the wrong key or
/// none at all.
fn replace_once(source: &str, find: &str, replacement: &str) -> (String, usize) {
    let count = source.matches(find).count();
    (source.replacen(find, replacement, 1), count)
}

fn execute_task_command(command: TaskCommand) -> anyhow::Result<ExitCode> {
    match command {
        TaskCommand::Ready {
            path,
            task_id,
            options,
        } => execute_task_mutation(&path, &task_id, TaskAction::Ready, &options),
        TaskCommand::Start {
            path,
            task_id,
            options,
        } => execute_task_mutation(&path, &task_id, TaskAction::Start, &options),
        TaskCommand::Block {
            path,
            task_id,
            reason,
            options,
        } => execute_task_mutation(&path, &task_id, TaskAction::Block(reason), &options),
        TaskCommand::Complete {
            path,
            task_id,
            evidence,
            options,
        } => execute_task_mutation(&path, &task_id, TaskAction::Complete(evidence), &options),
        TaskCommand::Reopen {
            path,
            task_id,
            reason,
            options,
        } => execute_task_mutation(&path, &task_id, TaskAction::Reopen(reason), &options),
        TaskCommand::Abandon {
            path,
            task_id,
            reason,
            options,
        } => execute_task_mutation(&path, &task_id, TaskAction::Abandon(reason), &options),
    }
}

fn execute_task_mutation(
    path: &Path,
    task_id: &str,
    action: TaskAction,
    options: &MutationOptions,
) -> anyhow::Result<ExitCode> {
    let task_id = TaskId::parse(task_id.to_owned())?;
    execute_mutation(path, Mutation::Task { task_id, action }, options)
}

fn execute_plan_command(command: PlanCommand) -> anyhow::Result<ExitCode> {
    match command {
        PlanCommand::Approve { path, options } => {
            execute_mutation(&path, Mutation::Plan(PlanAction::Approve), &options)
        }
        PlanCommand::Implement { path, options } => {
            execute_mutation(&path, Mutation::Plan(PlanAction::Implement), &options)
        }
        PlanCommand::Abandon {
            path,
            reason,
            options,
        } => execute_mutation(&path, Mutation::Plan(PlanAction::Abandon(reason)), &options),
    }
}

#[derive(Debug, Clone, Copy)]
enum EntryKind {
    Guidance,
    Decision,
}

fn execute_entry_command(command: EntryCommand, kind: EntryKind) -> anyhow::Result<ExitCode> {
    match command {
        EntryCommand::Add {
            path,
            file,
            stdin: _,
            options,
        } => {
            let entry = read_entry(file.as_deref())?;
            let mutation = match kind {
                EntryKind::Guidance => Mutation::AddGuidance(entry),
                EntryKind::Decision => Mutation::AddDecision(entry),
            };
            execute_mutation(&path, mutation, &options)
        }
    }
}

fn read_entry(path: Option<&Path>) -> anyhow::Result<String> {
    if let Some(path) = path {
        Ok(std::fs::read_to_string(path)?)
    } else {
        let mut entry = String::new();
        std::io::stdin().lock().read_to_string(&mut entry)?;
        Ok(entry)
    }
}

fn execute_mutation(
    path: &Path,
    mutation: Mutation,
    options: &MutationOptions,
) -> anyhow::Result<ExitCode> {
    let source = std::fs::read_to_string(path)?;
    let date = options.date.clone().unwrap_or_else(current_local_date);
    let request = MutationRequest { date, mutation };
    let prepared = tftio_planner::prepare_markdown_mutation(&source, &request)?;
    if !options.dry_run {
        tftio_planner::apply_prepared(path, &prepared)?;
    }
    render_change_report(path, &prepared, options)?;
    Ok(ExitCode::SUCCESS)
}

fn current_local_date() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn render_change_report(
    path: &Path,
    prepared: &tftio_planner::PreparedMutation,
    options: &MutationOptions,
) -> anyhow::Result<()> {
    let diff = prepared.unified_diff(path);
    match options.format {
        OutputFormat::Human if options.dry_run => {
            write!(std::io::stdout().lock(), "{diff}")?;
        }
        OutputFormat::Human => {
            let output = format!("{}: {}", path.display(), prepared.summary());
            writeln!(std::io::stdout().lock(), "{output}")?;
        }
        OutputFormat::Json => {
            let output = json!({
                "schema_version": 1,
                "path": path,
                "status": if options.dry_run { "dry_run" } else { "applied" },
                "summary": prepared.summary(),
                "changed": prepared.original() != prepared.replacement(),
                "diff": diff,
            });
            let output = serde_json::to_string_pretty(&output)?;
            writeln!(std::io::stdout().lock(), "{output}")?;
        }
    }
    Ok(())
}

fn validate_paths(format: OutputFormat, paths: &[PathBuf]) -> anyhow::Result<ExitCode> {
    let results = paths
        .iter()
        .map(|path| validate_path(path))
        .collect::<Vec<_>>();
    match format {
        OutputFormat::Human => render_human(&results)?,
        OutputFormat::Json => render_json(&results)?,
    }
    Ok(if results.iter().all(|result| result.valid) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn validate_path(path: &Path) -> DocumentResult {
    match std::fs::read_to_string(path) {
        Ok(source) => match tftio_planner::validate_markdown_path(&source, path) {
            Ok(report) => DocumentResult {
                path: path.to_owned(),
                valid: report.is_valid(),
                diagnostics: report.diagnostics,
            },
            Err(error) => DocumentResult {
                path: path.to_owned(),
                valid: false,
                diagnostics: error.diagnostics,
            },
        },
        Err(error) => DocumentResult {
            path: path.to_owned(),
            valid: false,
            diagnostics: vec![Diagnostic::new(
                tftio_planner::DiagnosticCode::RequiredValue,
                format!("cannot read file: {error}"),
                None,
            )],
        },
    }
}

fn render_human(results: &[DocumentResult]) -> std::io::Result<()> {
    let mut stderr = std::io::stderr().lock();
    for result in results {
        for diagnostic in &result.diagnostics {
            let line = format!(
                "{} {} [{}]: {}",
                if diagnostic.severity == DiagnosticSeverity::Error {
                    "ERROR"
                } else {
                    "WARN "
                },
                result.path.display(),
                diagnostic.code.as_str(),
                diagnostic.message
            );
            writeln!(stderr, "{line}")?;
        }
        if result.valid {
            writeln!(stderr, "OK    {}", result.path.display())?;
        } else {
            let errors = result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
                .count();
            writeln!(stderr, "FAIL  {}: {errors} error(s)", result.path.display())?;
        }
    }
    Ok(())
}

fn render_json(results: &[DocumentResult]) -> anyhow::Result<()> {
    let documents = results
        .iter()
        .map(|result| {
            json!({
                "path": result.path,
                "valid": result.valid,
                "diagnostics": result
                    .diagnostics
                    .iter()
                    .map(diagnostic_json)
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_writer_pretty(std::io::stdout().lock(), &documents)?;
    writeln!(std::io::stdout().lock())?;
    Ok(())
}

fn diagnostic_json(diagnostic: &Diagnostic) -> Value {
    let location = diagnostic.location.map(|location| {
        json!({
            "line": location.line,
            "column": location.column,
            "span": {
                "start": location.span.start,
                "end": location.span.end,
            },
        })
    });
    json!({
        "code": diagnostic.code.as_str(),
        "severity": diagnostic.severity.as_str(),
        "message": diagnostic.message,
        "location": location,
    })
}
