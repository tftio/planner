//! Run the cross-validator fixture manifest through `planner validate`.
//!
//! `tests/fixtures/format/` holds the corpus that decides whether a document is
//! a valid plan. This harness runs it against this crate; a consumer that pins
//! a planner build can run the same table against that build, so a pinned build
//! that disagrees is visible rather than latent. The manifest's `check-plan`
//! entries record the retired Python validator and are kept as history.
//!
//! A case is a baseline document with one anchor replaced, so a verdict is
//! attributable to exactly one rule. A case marked `divergence = "planner"` is
//! one this validator is known not to enforce: its recorded behaviour is
//! asserted rather than its correct behaviour, so the day it is fixed this test
//! fails and the manifest is updated. Nothing here weakens a fixture to make the
//! columns match.

use assert_cmd::Command;
use serde::Deserialize;

type Failure = Box<dyn std::error::Error>;

const THIS_VALIDATOR: &str = "planner";
const MANIFEST: &str = include_str!("fixtures/format/manifest.toml");
const SPEC: &str = include_str!("../resources/PLAN_SPEC.md");
const V2_HEADING: &str = "# Planning Document Format v2";

/// The five lists `PLAN_SPEC.md`'s v2 section marks as enforced. Every statement
/// in them must be claimed by exactly one rule; that bijection is what makes
/// "the count of v2 rules matches the count of rules with fixtures" checkable
/// rather than asserted.
const ENFORCED_LIST_INTROS: [&str; 5] = [
    "Frontmatter rules (enforced):",
    "Required in this order:",
    "Required, presence only, in any position after the ADR block:",
    "Per-task field rules (enforced):",
    "DAG rules (enforced), unchanged from v1:",
];

#[derive(Deserialize)]
struct Manifest {
    version: u32,
    rule: Vec<Rule>,
    case: Vec<Case>,
    unfixtured: Vec<Unfixtured>,
}

#[derive(Deserialize)]
struct Rule {
    id: String,
    spec_quote: String,
    passing: String,
    #[serde(default)]
    covers: Vec<String>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    baseline: String,
    expect: String,
    rule: Option<String>,
    find: Option<String>,
    replace: Option<String>,
    filename: Option<String>,
    diagnostic: Option<Diagnostic>,
    divergence: Option<String>,
}

/// The message each validator emits for a case, pinned so the table records
/// which rule fired in each implementation and not merely that something did.
#[derive(Deserialize)]
struct Diagnostic {
    planner: Option<String>,
}

#[derive(Deserialize)]
struct Unfixtured {
    statement: String,
    reason: String,
}

fn v2_section() -> Result<&'static str, Failure> {
    SPEC.find(V2_HEADING)
        .and_then(|start| SPEC.get(start..))
        .ok_or_else(|| Failure::from("PLAN_SPEC.md carries no v2 heading"))
}

fn is_list_item(line: &str) -> bool {
    if line.starts_with("- ") {
        return true;
    }
    let digits = line.trim_start_matches(|c: char| c.is_ascii_digit());
    digits.len() < line.len() && digits.starts_with(". ")
}

/// Extract the list items of the five enforced lists, in document order.
fn enforced_statements(section: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let mut collecting = false;
    for line in section.lines() {
        if ENFORCED_LIST_INTROS.contains(&line.trim()) {
            collecting = true;
        } else if collecting {
            if is_list_item(line) {
                statements.push(line);
            } else if !(line.trim().is_empty() || line.starts_with("  ")) {
                collecting = false;
            }
        }
    }
    statements
}

fn scratch_directory(label: &str) -> Result<std::path::PathBuf, Failure> {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("planner-fixtures-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

fn materialise(case: &Case, into: &std::path::Path) -> Result<std::path::PathBuf, Failure> {
    let baseline = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/format")
            .join(&case.baseline),
    )?;
    let text = match (&case.find, &case.replace) {
        (Some(find), Some(replace)) => {
            assert_eq!(
                baseline.matches(find.as_str()).count(),
                1,
                "case {}: anchor must occur exactly once in {}",
                case.id,
                case.baseline
            );
            baseline.replacen(find.as_str(), replace, 1)
        }
        _ => baseline,
    };
    let name = case
        .filename
        .clone()
        .unwrap_or_else(|| format!("2026-09-10-{}.md", case.id));
    let path = into.join(name);
    std::fs::write(&path, text)?;
    Ok(path)
}

fn verdict_for(path: &std::path::Path) -> Result<(String, String), Failure> {
    let assertion = Command::cargo_bin("planner")?
        .args(["validate"])
        .arg(path)
        .assert();
    let output = assertion.get_output();
    let verdict = if output.status.success() {
        "valid"
    } else {
        "invalid"
    };
    Ok((
        verdict.to_owned(),
        String::from_utf8(output.stderr.clone())?,
    ))
}

#[test]
fn the_manifest_claims_every_enforced_statement_the_v2_spec_makes() -> Result<(), Failure> {
    let manifest: Manifest = toml::from_str(MANIFEST)?;
    assert_eq!(manifest.version, 1, "unrecognised manifest version");
    let section = v2_section()?;

    let mut claimed: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
    for rule in &manifest.rule {
        assert!(
            section.contains(rule.spec_quote.as_str()),
            "rule {}: spec_quote is not in the v2 section verbatim",
            rule.id
        );
        let passing = manifest
            .case
            .iter()
            .find(|case| case.id == rule.passing)
            .ok_or_else(|| format!("rule {}: passing case {} is unknown", rule.id, rule.passing))?;
        assert_eq!(
            passing.expect, "valid",
            "rule {}: passing case is not a valid document",
            rule.id
        );
        let failing: Vec<&Case> = manifest
            .case
            .iter()
            .filter(|case| case.rule.as_deref() == Some(rule.id.as_str()))
            .collect();
        assert!(!failing.is_empty(), "rule {}: no failing case", rule.id);
        for case in failing {
            assert_eq!(
                case.expect, "invalid",
                "case {}: a rule's own case must be invalid",
                case.id
            );
        }
        for statement in &rule.covers {
            let previous = claimed.insert(statement.as_str(), rule.id.as_str());
            assert!(
                previous.is_none(),
                "statement claimed twice, by {previous:?} and {}: {statement}",
                rule.id
            );
        }
    }

    let statements = enforced_statements(section);
    assert!(
        !statements.is_empty(),
        "no enforced statement was extracted"
    );
    for statement in &statements {
        assert!(
            claimed.contains_key(statement),
            "enforced statement has no rule: {statement}"
        );
    }
    for statement in claimed.keys() {
        assert!(
            statements.contains(statement),
            "rule claims a statement the spec does not make: {statement}"
        );
    }

    for entry in &manifest.unfixtured {
        assert!(
            section.contains(entry.statement.as_str()),
            "unfixtured statement is not in the v2 section verbatim: {}",
            entry.statement
        );
        assert!(
            !claimed.contains_key(entry.statement.as_str()),
            "statement is both fixtured and unfixtured: {}",
            entry.statement
        );
        assert!(
            !entry.reason.trim().is_empty(),
            "unfixtured statement carries no reason: {}",
            entry.statement
        );
    }
    Ok(())
}

#[test]
fn every_case_gets_the_verdict_the_manifest_records() -> Result<(), Failure> {
    let manifest: Manifest = toml::from_str(MANIFEST)?;
    let directory = scratch_directory("cases")?;
    let mut rows = Vec::new();

    for case in &manifest.case {
        let path = materialise(case, &directory)?;
        let (actual, output) = verdict_for(&path)?;
        let diverges = case.divergence.as_deref() == Some(THIS_VALIDATOR);
        // The recorded behaviour of the validator that does not enforce this
        // rule, not the behaviour the spec asks for.
        let expected = if diverges {
            if case.expect == "invalid" {
                "valid"
            } else {
                "invalid"
            }
        } else {
            case.expect.as_str()
        };
        assert_eq!(
            actual, expected,
            "case {}: {THIS_VALIDATOR} disagrees with the manifest",
            case.id
        );
        let diagnostic = case
            .diagnostic
            .as_ref()
            .and_then(|pinned| pinned.planner.as_deref());
        match diagnostic {
            None => assert_ne!(
                actual, "invalid",
                "case {}: {THIS_VALIDATOR} rejects it but the manifest pins no diagnostic",
                case.id
            ),
            Some(pinned) => assert!(
                output.contains(pinned),
                "case {}: {THIS_VALIDATOR} did not emit the pinned diagnostic {pinned:?}: {output}",
                case.id
            ),
        }
        rows.push((case.id.clone(), case.expect.clone(), actual));
    }

    std::fs::remove_dir_all(&directory)?;
    assert_eq!(rows.len(), manifest.case.len());
    Ok(())
}
