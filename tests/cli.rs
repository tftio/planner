//! CLI integration tests for `planner`.

use assert_cmd::Command;

#[test]
fn validate_accepts_a_valid_plan() -> Result<(), Box<dyn std::error::Error>> {
    let mut command = Command::cargo_bin("planner")?;

    command
        .args([
            "validate",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/resources/examples/2026-06-15-replace-auth-middleware.md"
            ),
        ])
        .assert()
        .success();

    Ok(())
}

#[test]
fn validate_json_reports_multiple_invalid_documents() -> Result<(), Box<dyn std::error::Error>> {
    let mut command = Command::cargo_bin("planner")?;
    let assertion = command
        .args([
            "validate",
            "--format",
            "json",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/legacy/2026-06-22-hidden-criteria-evaluator-mismatch.md"
            ),
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/legacy/2026-06-22-hidden-criteria-missing-why-hidden.md"
            ),
        ])
        .assert()
        .code(1);
    let documents: serde_json::Value = serde_json::from_slice(&assertion.get_output().stdout)?;

    assert_eq!(documents.as_array().map(Vec::len), Some(2));
    assert_eq!(
        documents
            .as_array()
            .and_then(|values| values.first())
            .and_then(|value| value.get("valid"))
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    Ok(())
}

#[test]
fn validate_human_reports_invalid_and_unreadable_paths() -> Result<(), Box<dyn std::error::Error>> {
    let mut invalid = Command::cargo_bin("planner")?;
    invalid
        .args([
            "validate",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/legacy/2026-06-22-hidden-criteria-evaluator-mismatch.md"
            ),
        ])
        .assert()
        .code(1);

    let mut missing = Command::cargo_bin("planner")?;
    missing
        .args(["validate", "/definitely/missing/planner-plan.md"])
        .assert()
        .code(1);
    Ok(())
}

#[test]
fn validate_without_paths_is_a_usage_error() -> Result<(), Box<dyn std::error::Error>> {
    let mut command = Command::cargo_bin("planner")?;

    command.arg("validate").assert().code(2);
    Ok(())
}

#[test]
fn inspect_supports_every_role_and_format() -> Result<(), Box<dyn std::error::Error>> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/legacy/2026-06-22-hidden-criteria-valid.md"
    );
    for arguments in [
        vec!["inspect", "--role", "operator", "--format", "human", path],
        vec!["inspect", "--role", "operator", "--format", "json", path],
        vec!["inspect", "--role", "worker", "--format", "human", path],
        vec!["inspect", "--role", "worker", "--format", "json", path],
    ] {
        let mut command = Command::cargo_bin("planner")?;
        command.args(arguments).assert().success();
    }
    Ok(())
}

#[test]
fn inspect_rejects_an_invalid_plan() -> Result<(), Box<dyn std::error::Error>> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/legacy/2026-06-22-hidden-criteria-evaluator-mismatch.md"
    );
    let mut command = Command::cargo_bin("planner")?;
    command.args(["inspect", path]).assert().failure();
    Ok(())
}

#[test]
fn project_and_resource_commands_emit_content() -> Result<(), Box<dyn std::error::Error>> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/legacy/2026-06-22-hidden-criteria-valid.md"
    );
    let mut project = Command::cargo_bin("planner")?;
    let projected = project.args(["project", path]).assert().success();
    let projected = std::str::from_utf8(&projected.get_output().stdout)?;
    assert!(!projected.contains("    hidden_criteria:"));

    for arguments in [["spec", "show"], ["template", "show"]] {
        let mut command = Command::cargo_bin("planner")?;
        assert!(
            !command
                .args(arguments)
                .assert()
                .success()
                .get_output()
                .stdout
                .is_empty()
        );
    }
    Ok(())
}

#[test]
fn metadata_commands_expose_completions_health_and_version()
-> Result<(), Box<dyn std::error::Error>> {
    let mut completions = Command::cargo_bin("planner")?;
    let output = completions.args(["completions", "zsh"]).assert().success();
    let output = std::str::from_utf8(&output.get_output().stdout)?;
    assert!(output.contains("Shell completion for planner"));
    assert!(output.contains("_planner"));

    let mut doctor_human = Command::cargo_bin("planner")?;
    let output = doctor_human.arg("doctor").assert().success();
    assert!(std::str::from_utf8(&output.get_output().stdout)?.contains("planner health check"));
    let mut doctor_json = Command::cargo_bin("planner")?;
    let output = doctor_json
        .args(["doctor", "--format", "json"])
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&output.get_output().stdout)?;
    assert_eq!(
        report.get("ok").and_then(serde_json::Value::as_bool),
        Some(true)
    );

    let mut version_human = Command::cargo_bin("planner")?;
    let output = version_human.arg("version").assert().success();
    assert!(
        std::str::from_utf8(&output.get_output().stdout)?
            .contains(&format!("planner {}", env!("CARGO_PKG_VERSION")))
    );
    let mut version_json = Command::cargo_bin("planner")?;
    let output = version_json
        .args(["version", "--format", "json"])
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&output.get_output().stdout)?;
    assert_eq!(
        report.get("version").and_then(serde_json::Value::as_str),
        Some(env!("CARGO_PKG_VERSION"))
    );
    Ok(())
}

#[test]
fn init_creates_once_without_overwriting() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("planner-init-{}.md", std::process::id()));
    let mut create = Command::cargo_bin("planner")?;
    create.arg("init").arg(&path).assert().success();
    let content = std::fs::read_to_string(&path)?;
    assert_eq!(content, tftio_planner::resources::PLAN_TEMPLATE);

    let mut overwrite = Command::cargo_bin("planner")?;
    overwrite.arg("init").arg(&path).assert().failure();
    assert_eq!(std::fs::read_to_string(&path)?, content);
    std::fs::remove_file(path)?;
    Ok(())
}

#[test]
fn init_project_fills_the_slug_and_rejects_a_malformed_one()
-> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("planner-init-project-{}.md", std::process::id()));

    let mut create = Command::cargo_bin("planner")?;
    create
        .args(["init", "--project", "kb"])
        .arg(&path)
        .assert()
        .success();
    let content = std::fs::read_to_string(&path)?;
    assert!(content.contains("project:\n  slug: kb\n  remote: null\n"));
    std::fs::remove_file(&path)?;

    let mut rejected = Command::cargo_bin("planner")?;
    rejected
        .args(["init", "--project", "KB"])
        .arg(&path)
        .assert()
        .failure();
    assert!(!path.exists());
    Ok(())
}

#[test]
fn inspect_json_carries_project_only_for_the_v2_plan_that_declares_one()
-> Result<(), Box<dyn std::error::Error>> {
    let v1 = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/examples/2026-06-15-replace-auth-middleware.md"
    );
    let v2 = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/examples/2026-08-04-cache-session-lookups.md"
    );

    let mut v1_inspect = Command::cargo_bin("planner")?;
    let v1_output = v1_inspect
        .args(["inspect", "--format", "json", v1])
        .assert()
        .success();
    let v1_json = String::from_utf8(v1_output.get_output().stdout.clone())?;
    assert!(!v1_json.contains("\"project\""), "{v1_json}");

    let mut v2_inspect = Command::cargo_bin("planner")?;
    let v2_output = v2_inspect
        .args(["inspect", "--format", "json", v2])
        .assert()
        .success();
    let v2_json = String::from_utf8(v2_output.get_output().stdout.clone())?;
    assert!(v2_json.contains("\"project\": {"), "{v2_json}");
    assert!(v2_json.contains("\"slug\": \"gateway\""), "{v2_json}");
    assert!(
        v2_json.contains("\"remote\": \"github.com/example/gateway\""),
        "{v2_json}"
    );
    Ok(())
}

#[test]
fn lifecycle_commands_apply_atomic_synchronized_changes() -> Result<(), Box<dyn std::error::Error>>
{
    let path = mutation_fixture("lifecycle")?;
    let entry_path = path.with_extension("entry.txt");
    std::fs::write(&entry_path, "operator supplied guidance\n")?;

    run_mutation(["plan", "approve"], &path, &[])?;

    let mut guidance = Command::cargo_bin("planner")?;
    guidance
        .args(["guidance", "add"])
        .arg(&path)
        .arg("--file")
        .arg(&entry_path)
        .args(["--date", "2026-08-28"])
        .assert()
        .success();

    let mut decision = Command::cargo_bin("planner")?;
    decision
        .args(["decision", "add"])
        .arg(&path)
        .args(["--stdin", "--date", "2026-08-28"])
        .write_stdin("operator supplied decision\n")
        .assert()
        .success();

    run_mutation(["task", "ready"], &path, &["T001"])?;
    run_mutation(["task", "start"], &path, &["T001"])?;
    run_mutation(
        ["task", "block"],
        &path,
        &["T001", "--reason", "waiting for input"],
    )?;
    run_mutation(
        ["task", "reopen"],
        &path,
        &["T001", "--reason", "input arrived"],
    )?;
    run_mutation(["task", "start"], &path, &["T001"])?;

    let mut complete = Command::cargo_bin("planner")?;
    let assertion = complete
        .args(["task", "complete"])
        .arg(&path)
        .args([
            "T001",
            "--evidence",
            "checks passed",
            "--format",
            "json",
            "--date",
            "2026-08-28",
        ])
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&assertion.get_output().stdout)?;
    assert_eq!(
        report.get("status").and_then(serde_json::Value::as_str),
        Some("applied")
    );
    assert_eq!(
        report.get("changed").and_then(serde_json::Value::as_bool),
        Some(true)
    );

    run_mutation(["plan", "implement"], &path, &[])?;
    let source = std::fs::read_to_string(&path)?;
    let plan = tftio_planner::parse_markdown(&source)?;
    assert_eq!(
        plan.metadata.status,
        tftio_planner::model::PlanStatus::Implemented
    );
    assert_eq!(
        plan.tasks.first().ok_or("mutated plan has no task")?.status,
        tftio_planner::model::TaskStatus::Done
    );
    assert!(source.contains("Status: `done`"));
    assert!(source.contains("operator supplied guidance"));
    assert!(source.contains("operator supplied decision"));

    std::fs::remove_dir_all(path.parent().ok_or("mutation path has no parent")?)?;
    Ok(())
}

#[test]
fn dry_run_and_abandon_commands_cover_nonwriting_and_terminal_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let task_path = mutation_fixture("task-abandon")?;
    let original = std::fs::read_to_string(&task_path)?;
    let mut preview = Command::cargo_bin("planner")?;
    let output = preview
        .args(["task", "ready"])
        .arg(&task_path)
        .args(["T001", "--dry-run"])
        .assert()
        .success();
    assert!(output.get_output().stdout.starts_with(b"--- "));
    assert_eq!(std::fs::read_to_string(&task_path)?, original);

    let mut abandon_task = Command::cargo_bin("planner")?;
    let output = abandon_task
        .args(["task", "abandon"])
        .arg(&task_path)
        .args([
            "T001",
            "--reason",
            "not needed",
            "--dry-run",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&output.get_output().stdout)?;
    assert_eq!(
        report.get("status").and_then(serde_json::Value::as_str),
        Some("dry_run")
    );
    assert_eq!(std::fs::read_to_string(&task_path)?, original);
    run_mutation(
        ["task", "abandon"],
        &task_path,
        &["T001", "--reason", "not needed"],
    )?;

    let plan_path = mutation_fixture("plan-abandon")?;
    run_mutation(["plan", "abandon"], &plan_path, &["--reason", "superseded"])?;
    let plan = tftio_planner::parse_markdown(&std::fs::read_to_string(&plan_path)?)?;
    assert_eq!(
        plan.metadata.status,
        tftio_planner::model::PlanStatus::Abandoned
    );

    std::fs::remove_dir_all(task_path.parent().ok_or("mutation path has no parent")?)?;
    std::fs::remove_dir_all(plan_path.parent().ok_or("mutation path has no parent")?)?;
    Ok(())
}

fn mutation_fixture(label: &str) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("planner-cli-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("plan.md");
    std::fs::write(
        &path,
        include_str!("fixtures/legacy/2026-06-22-hidden-criteria-valid.md"),
    )?;
    Ok(path)
}

fn run_mutation<const N: usize>(
    command: [&str; 2],
    path: &std::path::Path,
    arguments: &[&str; N],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut process = Command::cargo_bin("planner")?;
    process
        .args(command)
        .arg(path)
        .args(arguments)
        .args(["--date", "2026-08-28"])
        .assert()
        .success();
    Ok(())
}
