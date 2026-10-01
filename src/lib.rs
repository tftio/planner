//! Authoritative planning-document domain library.

pub mod diagnostic;
pub mod doctor;
pub mod inspect;
pub mod markdown;
pub mod model;
pub mod mutate;
pub mod project;
pub mod resources;
pub mod state;
pub mod validate;
pub mod write;

pub use diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity, SourceLocation, SourceSpan};
pub use doctor::{PlannerDoctor, doctor_report};
pub use markdown::{ParseError, RenderError, parse_markdown, render_markdown};
pub use model::{OperatorPlan, WorkerPlan};
pub use mutate::{MutationError, PreparedMutation, prepare_markdown_mutation};
pub use project::{ProjectionError, project_worker_markdown};
pub use state::{
    Mutation, MutationOutcome, MutationRequest, PlanAction, StateError, TaskAction, apply_mutation,
};
pub use validate::{ValidationReport, validate_markdown, validate_markdown_path, validate_plan};
pub use write::{WriteError, apply_prepared};
