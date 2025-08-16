use anyhow::{Result, bail};
use ruff_source_file::LineIndex;
use std::env::current_dir;

use itertools::{EitherOrBoth, Itertools};
use py_checked_exceptions::{Exception, analyze_project};
use ruff_db::{
    diagnostic::Diagnostic,
    files::{File, FilePath},
    source::source_text,
    system::{OsSystem, SystemPath, SystemPathBuf},
};
use ty_project::{ProjectDatabase, ProjectMetadata};

#[test]
fn test_simple() -> Result<()> {
    assert_diagnostics(
        "simple.py",
        None,
        vec![("Raises RuntimeError", (2, 5), (2, 25))],
    )
}

#[test]
fn test_transitive() -> Result<()> {
    assert_diagnostics(
        "transitive.py",
        None,
        vec![
            ("Raises RuntimeError", (5, 5), (5, 23)),
            ("Raises RuntimeError", (9, 5), (9, 11)),
        ],
    )
}

#[test]
fn test_class() -> Result<()> {
    assert_diagnostics(
        "class.py",
        None,
        vec![
            ("Raises RuntimeError", (3, 9), (3, 29)),
            ("Raises RuntimeError", (6, 9), (6, 32)),
        ],
    )
}

#[test]
fn test_recursion() -> Result<()> {
    assert_diagnostics(
        "recursive.py",
        None,
        vec![
            ("Raises RuntimeError", (3, 5), (3, 25)),
            ("Raises RuntimeError", (7, 5), (7, 45)),
            ("Raises RuntimeError", (14, 5), (14, 25)),
        ],
    )
}

#[test]
fn test_target_exception() -> Result<()> {
    assert_diagnostics(
        "target_exception.py",
        Some("MyError".into()),
        vec![("Raises MyError", (6, 9), (6, 24))],
    )
}

#[test]
fn test_except() -> Result<()> {
    assert_diagnostics(
        "except.py",
        None,
        vec![
            ("Raises TypeError", (16, 13), (16, 30)),
            ("Raises RuntimeError", (25, 9), (25, 14)),
            ("Raises RuntimeError", (32, 9), (32, 16)),
            ("Raises RuntimeError", (39, 9), (39, 14)),
            ("Raises RuntimeError", (55, 9), (55, 14)),
            ("Raises RuntimeError", (75, 13), (75, 20)),
        ],
    )
}

#[test]
fn test_except2() -> Result<()> {
    assert_diagnostics(
        "except2.py",
        None,
        vec![
            ("Raises TypeError", (16, 13), (16, 30)),
            ("Raises FileNotFoundError", (25, 9), (25, 14)),
            ("Raises FileNotFoundError", (32, 9), (32, 16)),
            ("Raises FileNotFoundError", (39, 9), (39, 14)),
            ("Raises FileNotFoundError", (55, 9), (55, 14)),
            ("Raises FileNotFoundError", (75, 13), (75, 20)),
        ],
    )
}

#[test]
fn test_inheritance() -> Result<()> {
    assert_diagnostics(
        "inheritance.py",
        None,
        vec![
            ("Raises CustomBaseError", (26, 9), (26, 32)),
            ("Raises CustomBaseError", (67, 9), (67, 14)),
        ],
    )
}

fn assert_diagnostics(
    test_file: &str,
    target_exception: Option<String>,
    expected_diagnostics: Vec<(&str, (usize, usize), (usize, usize))>,
) -> Result<()> {
    let project_path = SystemPathBuf::from_path_buf(current_dir()?)
        .unwrap()
        .join("tests/fixtures");
    let filter_path = project_path.join(test_file);
    let system = OsSystem::new(&project_path);
    let mut project_metadata =
        ProjectMetadata::discover(&SystemPath::new(project_path.as_str()), &system)?;
    project_metadata.apply_configuration_files(&system)?;
    let db = ProjectDatabase::new(project_metadata, system.clone())?;
    let db2 = db.clone();
    let project_path2 = project_path.clone();
    let target_exceptions = target_exception
        .iter()
        .map(|e| Exception::new(e.clone(), vec![Exception::base_exception()]))
        .collect::<Vec<_>>();
    let diagnostics: Vec<Diagnostic> =
        analyze_project(project_path, db, Some(filter_path), target_exceptions, None)?.collect();

    let expected_file = File::new(&db2, FilePath::System(project_path2.join(test_file)));
    let source = source_text(&db2, expected_file);
    let index = LineIndex::from_source_text(&source);

    for item in diagnostics.iter().zip_longest(expected_diagnostics) {
        match item {
            EitherOrBoth::Left(diag) => bail!("Found unexpected diagnostic {diag:?}"),
            EitherOrBoth::Right(expected) => {
                bail!("Expected diagnostic but none were found {expected:?}")
            }
            EitherOrBoth::Both(
                diag,
                (
                    expected_msg,
                    (expected_line_start, expected_column_start),
                    (expected_line_end, expected_column_end),
                ),
            ) => {
                let diag_file = diag
                    .primary_annotation()
                    .unwrap()
                    .get_span()
                    .expect_ty_file();
                assert_eq!(diag.primary_message(), expected_msg);
                assert_eq!(diag_file.path(&db2), expected_file.path(&db2));
                let range = diag
                    .primary_annotation()
                    .unwrap()
                    .get_span()
                    .range()
                    .unwrap();
                let diag_start = index.line_column(range.start(), &source);
                let diag_end = index.line_column(range.end(), &source);
                assert_eq!(
                    (
                        format!("{}:{}", diag_start.line, diag_start.column),
                        format!("{}:{}", diag_end.line, diag_end.column)
                    ),
                    (
                        format!("{}:{}", expected_line_start, expected_column_start),
                        format!("{}:{}", expected_line_end, expected_column_end)
                    )
                );
            }
        }
    }
    Ok(())
}
