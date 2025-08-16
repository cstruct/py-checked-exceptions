use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use py_checked_exceptions::{analyze_project, extract_exception};
use ruff_db::{
    diagnostic::{Diagnostic, DisplayDiagnosticConfig},
    system::{OsSystem, SystemPath, SystemPathBuf},
};
use ty_python_semantic::{resolve_module, semantic_index::global_scope, types::resolve_definition::find_symbol_in_scope, ModuleName};
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
        .filter_map(|path| {
            let parts = path.split(".").collect_vec();
            let exception_name = parts.last().expect("target exception has to be in 'full.module.path.Exception' format.");
            let module_components = parts[..parts.len() - 1].to_vec();
            let module_name = ModuleName::from_components(module_components).expect("target exception has to be a valid module path.");
            let module = resolve_module(&db, &module_name).expect("target exception has to resolve to an existing module.");
            let module_file = module.file(&db).unwrap();
            let global_scope = global_scope(&db, module_file);
            let definitions_in_module = find_symbol_in_scope(&db, global_scope, exception_name);
            for def in definitions_in_module {
                let file = def.file(&db);
                let Some(exc) = extract_exception(&db, file, def) else { continue; };
                return Some(exc)
            }
            None
        })
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
