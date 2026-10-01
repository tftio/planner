//! Stable diagnostics shared by parsing, validation, and mutation.

/// Stable machine-readable diagnostic identifier.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DiagnosticCode {
    /// The document does not start with a YAML frontmatter delimiter.
    MissingFrontmatter,
    /// The frontmatter has no closing delimiter.
    UnterminatedFrontmatter,
    /// YAML could not be decoded into the format-v1 model.
    InvalidYaml,
    /// A required task-graph marker is absent or ordered incorrectly.
    InvalidTaskGraphMarkers,
    /// The task-graph markers do not contain a supported YAML fence.
    MissingTaskGraphFence,
    /// The document contains carriage returns rather than canonical LF endings.
    NonCanonicalLineEnding,
    /// A required value or section is absent or empty.
    RequiredValue,
    /// A date does not use the format-v1 calendar-date shape.
    InvalidDate,
    /// The task graph is empty.
    EmptyTaskGraph,
    /// A task dependency is invalid.
    InvalidDependency,
    /// The task graph contains a dependency cycle.
    DependencyCycle,
    /// A task status is inconsistent with its dependency statuses.
    DependencyStatus,
    /// A hidden criterion violates the criterion contract.
    InvalidHiddenCriterion,
    /// Required Markdown structure is missing, empty, or out of order.
    InvalidDocumentStructure,
    /// Task Graph and Task Details identifiers do not form a bijection.
    TaskDetailMismatch,
    /// A plan filename does not use the format-v1 naming convention.
    InvalidFilename,
    /// Duplicated Task Graph and Task Details content disagrees.
    DuplicatedStateMismatch,
    /// A v2 `project` mapping's `slug` or `remote` is malformed.
    InvalidProject,
}

impl DiagnosticCode {
    /// Return the compatibility-stable external code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingFrontmatter => "P001",
            Self::UnterminatedFrontmatter => "P002",
            Self::InvalidYaml => "P003",
            Self::InvalidTaskGraphMarkers => "P004",
            Self::MissingTaskGraphFence => "P005",
            Self::NonCanonicalLineEnding => "P006",
            Self::RequiredValue => "V001",
            Self::InvalidDate => "V002",
            Self::EmptyTaskGraph => "V003",
            Self::InvalidDependency => "V004",
            Self::DependencyCycle => "V005",
            Self::DependencyStatus => "V006",
            Self::InvalidHiddenCriterion => "V007",
            Self::InvalidDocumentStructure => "V008",
            Self::TaskDetailMismatch => "V009",
            Self::InvalidFilename => "V010",
            Self::DuplicatedStateMismatch => "V011",
            Self::InvalidProject => "V012",
        }
    }
}

/// Diagnostic disposition used to decide validation success.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    /// The document is invalid.
    Error,
    /// The document remains valid but warrants attention.
    Warning,
}

impl DiagnosticSeverity {
    /// Return the stable lowercase representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// A half-open byte range in the original UTF-8 document.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SourceSpan {
    /// Inclusive byte offset.
    pub start: usize,
    /// Exclusive byte offset.
    pub end: usize,
}

impl SourceSpan {
    /// Construct a half-open source span.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Return whether the span contains no bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// One-based source coordinates plus the corresponding byte span.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SourceLocation {
    /// One-based line number.
    pub line: usize,
    /// One-based Unicode-scalar column number.
    pub column: usize,
    /// Byte span in the original source.
    pub span: SourceSpan,
}

/// A diagnostic suitable for human rendering or stable machine output.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Diagnostic {
    /// Stable diagnostic identifier.
    pub code: DiagnosticCode,
    /// Whether this diagnostic invalidates the document.
    pub severity: DiagnosticSeverity,
    /// Human-readable detail that is not a compatibility key.
    pub message: String,
    /// Source location when the failure can be located.
    pub location: Option<SourceLocation>,
}

impl Diagnostic {
    /// Construct a diagnostic.
    #[must_use]
    pub fn new(
        code: DiagnosticCode,
        message: impl Into<String>,
        location: Option<SourceLocation>,
    ) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Error,
            message: message.into(),
            location,
        }
    }

    /// Construct a warning diagnostic.
    #[must_use]
    pub fn warning(
        code: DiagnosticCode,
        message: impl Into<String>,
        location: Option<SourceLocation>,
    ) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
            location,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, DiagnosticCode, DiagnosticSeverity, SourceSpan};

    #[test]
    fn codes_and_spans_cover_the_public_diagnostic_surface() {
        let codes = [
            (DiagnosticCode::MissingFrontmatter, "P001"),
            (DiagnosticCode::UnterminatedFrontmatter, "P002"),
            (DiagnosticCode::InvalidYaml, "P003"),
            (DiagnosticCode::InvalidTaskGraphMarkers, "P004"),
            (DiagnosticCode::MissingTaskGraphFence, "P005"),
            (DiagnosticCode::NonCanonicalLineEnding, "P006"),
            (DiagnosticCode::RequiredValue, "V001"),
            (DiagnosticCode::InvalidDate, "V002"),
            (DiagnosticCode::EmptyTaskGraph, "V003"),
            (DiagnosticCode::InvalidDependency, "V004"),
            (DiagnosticCode::DependencyCycle, "V005"),
            (DiagnosticCode::DependencyStatus, "V006"),
            (DiagnosticCode::InvalidHiddenCriterion, "V007"),
            (DiagnosticCode::InvalidDocumentStructure, "V008"),
            (DiagnosticCode::TaskDetailMismatch, "V009"),
            (DiagnosticCode::InvalidFilename, "V010"),
            (DiagnosticCode::DuplicatedStateMismatch, "V011"),
            (DiagnosticCode::InvalidProject, "V012"),
        ];
        for (code, expected) in codes {
            assert_eq!(code.as_str(), expected);
        }
        assert!(SourceSpan::new(1, 1).is_empty());
        assert!(!SourceSpan::new(1, 2).is_empty());
        assert_eq!(DiagnosticSeverity::Error.as_str(), "error");
        assert_eq!(DiagnosticSeverity::Warning.as_str(), "warning");
        let warning = Diagnostic::warning(DiagnosticCode::InvalidFilename, "name", None);
        assert_eq!(warning.severity, DiagnosticSeverity::Warning);
    }
}
