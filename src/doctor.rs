//! Self-contained planner health checks.

use tftio_lib::{DoctorCheck, DoctorChecks, DoctorReport, RepoInfo};

use crate::{parse_markdown, render_markdown, validate_markdown};

const COMPATIBILITY_EXAMPLE: &str =
    include_str!("../resources/examples/2026-06-15-replace-auth-middleware.md");

/// Planner doctor-check provider for shared `tftio-lib` rendering.
#[derive(Debug, Clone, Copy)]
pub struct PlannerDoctor;

impl DoctorChecks for PlannerDoctor {
    fn repo_info() -> RepoInfo {
        RepoInfo::new("tftio", "planner")
    }

    fn current_version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn tool_checks(&self) -> Vec<DoctorCheck> {
        checks_for_sources(crate::resources::PLAN_SPEC, COMPATIBILITY_EXAMPLE)
    }
}

/// Build the deterministic health report used by the CLI.
#[must_use]
pub fn doctor_report() -> DoctorReport {
    DoctorReport::for_tool(&PlannerDoctor)
}

fn checks_for_sources(specification: &str, example: &str) -> Vec<DoctorCheck> {
    let versions_present = ["Planning Document Format v1", "Planning Document Format v2"]
        .iter()
        .all(|heading| specification.contains(heading));
    let specification_check = if versions_present {
        DoctorCheck::pass("Embedded Planning Document Format specification")
    } else {
        DoctorCheck::fail(
            "Embedded Planning Document Format specification",
            "a required format-version heading is absent",
        )
    };
    let validation_check = match validate_markdown(example) {
        Ok(report) if report.is_valid() => {
            DoctorCheck::pass("Embedded compatibility example validates")
        }
        Ok(report) => DoctorCheck::fail(
            "Embedded compatibility example validates",
            format!("{} validation diagnostics", report.diagnostics.len()),
        ),
        Err(error) => DoctorCheck::fail(
            "Embedded compatibility example validates",
            error.to_string(),
        ),
    };
    let round_trip_is_stable = parse_markdown(example)
        .ok()
        .and_then(|plan| render_markdown(&plan).ok().map(|rendered| (plan, rendered)))
        .and_then(|(plan, rendered)| {
            parse_markdown(&rendered)
                .ok()
                .map(|round_trip| (plan, round_trip))
        })
        .is_some_and(|(plan, round_trip)| plan == round_trip);
    let round_trip_check = if round_trip_is_stable {
        DoctorCheck::pass("Markdown semantic round trip")
    } else {
        DoctorCheck::fail(
            "Markdown semantic round trip",
            "example did not round trip through the domain model",
        )
    };
    vec![specification_check, validation_check, round_trip_check]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_and_broken_sources_produce_actionable_checks() {
        let healthy = checks_for_sources(crate::resources::PLAN_SPEC, COMPATIBILITY_EXAMPLE);
        assert!(healthy.iter().all(|check| check.passed));
        assert_eq!(doctor_report().exit_code(), 0);

        let malformed = checks_for_sources("Planning Document Format v1 only", "not a plan");
        assert!(malformed.iter().all(|check| !check.passed));
        assert!(malformed.iter().all(|check| check.message.is_some()));

        let semantically_invalid =
            COMPATIBILITY_EXAMPLE.replacen("depends_on: []", "depends_on: [T001]", 1);
        let invalid_checks = checks_for_sources(crate::resources::PLAN_SPEC, &semantically_invalid);
        assert!(invalid_checks.iter().any(|check| !check.passed));
    }
}
