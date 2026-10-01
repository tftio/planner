//! Compare-before-write atomic replacement for prepared plan mutations.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

use crate::mutate::PreparedMutation;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Failure while applying a prepared mutation to an exact path.
#[derive(Debug, Error)]
pub enum WriteError {
    /// The source changed after mutation preparation.
    #[error("source changed before atomic replacement: {path}")]
    Conflict {
        /// Exact conflicting path.
        path: PathBuf,
    },
    /// A filesystem operation failed before the atomic commit point.
    #[error("failed to {operation} {path}: {source}")]
    Io {
        /// Operation that failed.
        operation: &'static str,
        /// Exact affected path.
        path: PathBuf,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
}

/// Apply a prepared mutation through same-directory atomic replacement.
///
/// The path is compared with the captured original both before temporary-file creation
/// and immediately before replacement. Every error before `rename` leaves the original
/// byte-for-byte unchanged; a successful `rename` exposes the complete validated
/// replacement at once.
///
/// # Errors
///
/// Returns [`WriteError::Conflict`] for stale source bytes, or [`WriteError::Io`] when a
/// filesystem operation fails before replacement.
pub fn apply_prepared(path: &Path, mutation: &PreparedMutation) -> Result<(), WriteError> {
    apply_with_hook(path, mutation, |_| Ok(()))
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum WritePhase {
    BeforeCreate,
    AfterCreate,
    AfterWrite,
    AfterSync,
    BeforeReplace,
}

fn apply_with_hook(
    path: &Path,
    mutation: &PreparedMutation,
    mut hook: impl FnMut(WritePhase) -> io::Result<()>,
) -> Result<(), WriteError> {
    compare_source(path, mutation.original())?;
    let permissions = metadata(path)?.permissions();
    run_hook(path, &mut hook, WritePhase::BeforeCreate)?;

    let temporary_path = temporary_path(path);
    let mut temporary = open_temporary(&temporary_path)?;
    let mut guard = TemporaryGuard::new(temporary_path.clone());
    set_temporary_permissions(&temporary, permissions, &temporary_path)?;
    run_hook(path, &mut hook, WritePhase::AfterCreate)?;
    write_temporary(&mut temporary, mutation.replacement(), &temporary_path)?;
    run_hook(path, &mut hook, WritePhase::AfterWrite)?;
    io_result(temporary.sync_all(), "sync", &temporary_path)?;
    run_hook(path, &mut hook, WritePhase::AfterSync)?;
    drop(temporary);

    run_hook(path, &mut hook, WritePhase::BeforeReplace)?;
    compare_source(path, mutation.original())?;
    atomic_replace(&temporary_path, path)?;
    guard.disarm();
    Ok(())
}

fn compare_source(path: &Path, expected: &str) -> Result<(), WriteError> {
    let current = io_result(fs::read(path), "read", path)?;
    if current == expected.as_bytes() {
        Ok(())
    } else {
        Err(WriteError::Conflict {
            path: path.to_owned(),
        })
    }
}

fn metadata(path: &Path) -> Result<fs::Metadata, WriteError> {
    io_result(fs::metadata(path), "inspect", path)
}

fn open_temporary(path: &Path) -> Result<File, WriteError> {
    io_result(
        OpenOptions::new().write(true).create_new(true).open(path),
        "create temporary file",
        path,
    )
}

fn set_temporary_permissions(
    temporary: &File,
    permissions: fs::Permissions,
    path: &Path,
) -> Result<(), WriteError> {
    io_result(
        temporary.set_permissions(permissions),
        "set permissions on",
        path,
    )
}

fn write_temporary(temporary: &mut File, replacement: &str, path: &Path) -> Result<(), WriteError> {
    io_result(temporary.write_all(replacement.as_bytes()), "write", path)
}

fn atomic_replace(temporary_path: &Path, path: &Path) -> Result<(), WriteError> {
    io_result(fs::rename(temporary_path, path), "atomically replace", path)
}

fn run_hook(
    path: &Path,
    hook: &mut impl FnMut(WritePhase) -> io::Result<()>,
    phase: WritePhase,
) -> Result<(), WriteError> {
    io_result(hook(phase), "execute write phase", path)
}

fn temporary_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{name}.planner-{}-{sequence}.tmp",
        std::process::id()
    ))
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> WriteError {
    WriteError::Io {
        operation,
        path: path.to_owned(),
        source,
    }
}

fn io_result<T>(
    result: io::Result<T>,
    operation: &'static str,
    path: &Path,
) -> Result<T, WriteError> {
    result.map_err(|source| io_error(operation, path, source))
}

struct TemporaryGuard {
    path: Option<PathBuf>,
}

impl TemporaryGuard {
    const fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for TemporaryGuard {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(label: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "planner-write-test-{}-{sequence}-{label}",
                std::process::id()
            ));
        fs::create_dir_all(&directory)?;
        Ok(directory.join("plan.md"))
    }

    fn prepared() -> Result<PreparedMutation, Box<dyn std::error::Error>> {
        let task_id = crate::model::TaskId::parse("T001")?;
        let request = crate::state::MutationRequest {
            date: "2026-08-28".to_owned(),
            mutation: crate::state::Mutation::Task {
                task_id,
                action: crate::state::TaskAction::Ready,
            },
        };
        crate::mutate::prepare_markdown_mutation(
            include_str!("../tests/fixtures/legacy/2026-06-22-hidden-criteria-valid.md"),
            &request,
        )
        .map_err(Into::into)
    }

    #[test]
    fn every_precommit_fault_preserves_the_original() -> Result<(), Box<dyn std::error::Error>> {
        for phase in [
            WritePhase::BeforeCreate,
            WritePhase::AfterCreate,
            WritePhase::AfterWrite,
            WritePhase::AfterSync,
            WritePhase::BeforeReplace,
        ] {
            let path = test_path(&format!("{phase:?}"))?;
            let prepared = prepared()?;
            fs::write(&path, prepared.original())?;
            let result = apply_with_hook(&path, &prepared, |current| {
                if current == phase {
                    Err(io::Error::other("injected failure"))
                } else {
                    Ok(())
                }
            });
            assert!(matches!(result, Err(WriteError::Io { .. })));
            assert_eq!(fs::read_to_string(&path)?, prepared.original());
            fs::remove_dir_all(path.parent().ok_or("test path has no parent")?)?;
        }
        Ok(())
    }

    #[test]
    fn stale_sources_are_rejected_before_and_during_write() -> Result<(), Box<dyn std::error::Error>>
    {
        let stale_path = test_path("stale-before")?;
        let prepared = prepared()?;
        fs::write(&stale_path, "concurrent\n")?;
        assert!(matches!(
            apply_prepared(&stale_path, &prepared),
            Err(WriteError::Conflict { .. })
        ));
        assert_eq!(fs::read_to_string(&stale_path)?, "concurrent\n");
        fs::remove_dir_all(stale_path.parent().ok_or("test path has no parent")?)?;

        let racing_path = test_path("stale-during")?;
        fs::write(&racing_path, prepared.original())?;
        let result = apply_with_hook(&racing_path, &prepared, |phase| {
            if phase == WritePhase::BeforeReplace {
                fs::write(&racing_path, "concurrent\n")?;
            }
            Ok(())
        });
        assert!(matches!(result, Err(WriteError::Conflict { .. })));
        assert_eq!(fs::read_to_string(&racing_path)?, "concurrent\n");
        fs::remove_dir_all(racing_path.parent().ok_or("test path has no parent")?)?;
        Ok(())
    }

    #[test]
    fn successful_apply_exposes_the_complete_replacement() -> Result<(), Box<dyn std::error::Error>>
    {
        let path = test_path("success")?;
        let prepared = prepared()?;
        fs::write(&path, prepared.original())?;
        apply_prepared(&path, &prepared)?;
        assert_eq!(fs::read_to_string(&path)?, prepared.replacement());
        fs::remove_dir_all(path.parent().ok_or("test path has no parent")?)?;
        Ok(())
    }

    #[test]
    fn filesystem_helpers_report_errors_and_cover_path_fallbacks()
    -> Result<(), Box<dyn std::error::Error>> {
        let missing = test_path("missing")?;
        let prepared = prepared()?;
        assert!(matches!(
            apply_prepared(&missing, &prepared),
            Err(WriteError::Io { .. })
        ));
        assert!(matches!(metadata(&missing), Err(WriteError::Io { .. })));

        fs::write(&missing, "occupied")?;
        assert!(matches!(
            open_temporary(&missing),
            Err(WriteError::Io { .. })
        ));
        assert!(
            temporary_path(Path::new("/"))
                .to_string_lossy()
                .contains("planner-")
        );
        fs::remove_dir_all(missing.parent().ok_or("test path has no parent")?)?;
        Ok(())
    }
}
