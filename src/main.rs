use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use py_checked_exceptions::analyze_project;
use ruff_db::{
    diagnostic::{Diagnostic, DisplayDiagnosticConfig},
    system::{OsSystem, SystemPath, SystemPathBuf},
};
use std::{io::Write, sync::LazyLock};
use ty_project::{ProjectDatabase, ProjectMetadata};

use crate::args::{CheckCommand, Cli, Command};
use py_checked_exceptions::Exception;

mod args;

fn main() -> Result<()> {
    let args = Cli::parse();
    // The base path to which all CLI arguments are relative to.
    let cwd = {
        let cwd = std::env::current_dir().context("Failed to get the current working directory")?;
        SystemPathBuf::from_path_buf(cwd)
            .map_err(|path| {
                anyhow!(
                    "The current working directory `{}` contains non-Unicode characters. py-checked-exceptions only supports Unicode paths.",
                    path.display()
                )
            })?
    };

    match args.command {
        Command::Check(check_cmd) => check(check_cmd, cwd),
    }
}

fn check(check: CheckCommand, cwd: SystemPathBuf) -> Result<()> {
    let project_path = match check.project {
        Some(path) if path.is_absolute() => path,
        Some(path) => cwd.join(path),
        None => cwd,
    }
    .clone();
    let project_path2 = project_path.clone();
    let filter_path = match check.filter {
        Some(path) if path.is_absolute() && path.starts_with(project_path2) => Some(path),
        Some(path) if path.is_absolute() => bail!("filter_path must be a child of project_path"),
        Some(path) => Some(project_path.join(path)),
        None => None,
    };

    let system = OsSystem::new(&project_path);
    let mut project_metadata =
        ProjectMetadata::discover(SystemPath::new(project_path.as_str()), &system)?;
    project_metadata.apply_configuration_files(&system)?;
    let db = ProjectDatabase::new(project_metadata, system.clone())?;

    static PB: LazyLock<ProgressBar> = LazyLock::new(|| ProgressBar::new(100));
    PB.set_style(ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} \n({msg})",
    )?);

    // Convert string exceptions to Exception structs
    let target_exceptions: Vec<Exception> = check
        .target_exceptions
        .into_iter()
        .map(|name| Exception::new(name, vec![Exception::base_exception()]))
        .collect();

    let mut diagnostics: Vec<Diagnostic> = analyze_project(
        project_path.clone(),
        db.clone(),
        filter_path,
        target_exceptions,
        Some(&PB),
    )?
    .collect();
    PB.finish_and_clear();

    diagnostics.sort_unstable_by_key(|diagnostic| {
        (
            diagnostic.expect_primary_span().expect_ty_file(),
            diagnostic
                .primary_span()
                .and_then(|span| span.range())
                .unwrap_or_default()
                .start(),
        )
    });

    let display_config = DisplayDiagnosticConfig::default();
    let mut stdout = std::io::stdout().lock();

    for diagnostic in &diagnostics {
        write!(stdout, "{}", diagnostic.display(&db, &display_config))?;
    }

    Ok(())
}
