use anyhow::{Context, Result, anyhow};
use clap::Parser;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use py_checked_exceptions::{
    AnalysisEvent, AnalysisGap, AnalysisOptions, analyze_project_with_options,
    resolve_absolute_module_path,
};
use rayon::ThreadPoolBuilder;
use ruff_db::{
    diagnostic::{Diagnostic, DisplayDiagnosticConfig},
    max_parallelism,
    system::{OsSystem, SystemPath, SystemPathBuf},
};
use std::sync::LazyLock;
use std::{fmt::Write, process::ExitCode};
use ty_project::{
    Db, ProjectDatabase, ProjectMetadata, metadata::options::ProjectOptionsOverrides,
};

use crate::{
    args::{AnalysisGapOutput, CheckCommand, Cli, Command, TerminalColor},
    config::load_project_config,
    logging::setup_tracing,
    printer::Printer,
};
use py_checked_exceptions::Exception;

mod args;
mod config;
mod logging;
mod printer;

fn main() -> Result<ExitCode> {
    setup_rayon();
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

fn check(check: CheckCommand, cwd: SystemPathBuf) -> Result<ExitCode> {
    let project_path = match check.project {
        Some(ref path) if path.is_absolute() => path.clone(),
        Some(ref path) => cwd.join(path),
        None => cwd,
    };
    let project_path2 = project_path.clone();
    let check_paths: Vec<_> = check
        .paths
        .iter()
        .map(|path| SystemPath::absolute(path, &project_path2))
        .collect();

    let system = OsSystem::new(&project_path);
    let mut project_metadata =
        ProjectMetadata::discover(SystemPath::new(project_path.as_str()), &system)?;
    let loaded_config = load_project_config(&system, project_metadata.root())?;
    let project_config = &loaded_config.config;
    let color = check.color.or(project_config.color);
    let analysis_gaps = check.analysis_gaps.or(project_config.show_analysis_gaps);
    let target_exception_names = if check.target_exceptions.is_empty() {
        project_config.target_exceptions.clone().unwrap_or_default()
    } else {
        check.target_exceptions.clone()
    };
    let extensions = if check.extensions.is_empty() {
        project_config.extensions.clone().unwrap_or_default()
    } else {
        check.extensions.clone()
    };

    set_colored_override(color);
    let verbosity = check.verbosity.level();
    let _guard = setup_tracing(verbosity, color.unwrap_or_default())?;
    let printer = Printer::default().with_verbosity(verbosity);

    project_metadata.apply_configuration_files(&system)?;
    project_metadata.apply_options(project_config.as_ty_options(&loaded_config.path));
    let project_options_overrides = ProjectOptionsOverrides::new(None, check.options());
    project_metadata.apply_overrides(&project_options_overrides);
    let mut db = ProjectDatabase::new(project_metadata, system.clone())?;

    if !check_paths.is_empty() {
        db.project().set_included_paths(&mut db, check_paths);
    }

    static PB: LazyLock<ProgressBar> = LazyLock::new(|| ProgressBar::new(100));
    PB.set_style(ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} \n({msg})",
    )?);

    // Convert string exceptions to Exception structs
    let target_exceptions: Vec<Exception> = target_exception_names
        .into_iter()
        .map(|path| resolve_absolute_module_path(&db, &path))
        .collect();

    let analysis_options = AnalysisOptions::default()
        .with_extensions(extensions)
        .with_context_manager_effects(
            project_config
                .context_manager_effects
                .clone()
                .unwrap_or_default(),
        );
    let events =
        analyze_project_with_options(db.clone(), target_exceptions, Some(&PB), analysis_options)?;
    let mut diagnostics = Vec::new();
    let mut gaps = Vec::new();
    for event in events {
        match event {
            AnalysisEvent::Diagnostic(diagnostic) => diagnostics.push(diagnostic),
            AnalysisEvent::Gap(gap) => gaps.push(gap),
        }
    }
    PB.finish_and_clear();

    let mut seen_gaps = std::collections::HashSet::new();
    gaps.retain(|gap| seen_gaps.insert(gap.clone()));
    gaps.sort_unstable_by_key(|gap| {
        (
            gap.file().path(&db).as_str().to_string(),
            gap.range().start(),
            gap.kind().code(),
        )
    });

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

    let terminal_settings = db.project().settings(&db).terminal();
    let display_config = DisplayDiagnosticConfig::default()
        .format(terminal_settings.output_format.into())
        .color(colored::control::SHOULD_COLORIZE.should_colorize());

    let diagnostics_count = diagnostics.len();
    let mut stdout = printer.stream_for_details().lock();
    for diagnostic in diagnostics {
        if stdout.is_enabled() {
            write!(stdout, "{}", diagnostic.display(&db, &display_config))?;
        }
    }
    if matches!(analysis_gaps, Some(AnalysisGapOutput::Full)) && stdout.is_enabled() {
        for gap in &gaps {
            let diagnostic = Diagnostic::from(gap);
            write!(stdout, "{}", diagnostic.display(&db, &display_config))?;
        }
    }
    drop(stdout);

    if analysis_gaps.is_some() {
        print_analysis_gap_summary(printer, &gaps)?;
    }

    if diagnostics_count == 0 {
        writeln!(
            printer.stream_for_success_summary(),
            "{}",
            "All checks passed!".green().bold()
        )?;

        Ok(ExitCode::SUCCESS)
    } else {
        writeln!(
            printer.stream_for_failure_summary(),
            "Found {} diagnostic{}",
            diagnostics_count,
            if diagnostics_count > 1 { "s" } else { "" }
        )?;

        Ok(if diagnostics_count > 0 {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        })
    }
}

fn print_analysis_gap_summary(printer: Printer, gaps: &[AnalysisGap]) -> Result<()> {
    if gaps.is_empty() {
        writeln!(
            printer.stream_for_requested_summary(),
            "No analysis gaps found."
        )?;
        return Ok(());
    }
    let mut counts = std::collections::BTreeMap::new();
    for gap in gaps {
        *counts.entry(gap.kind().description()).or_insert(0usize) += 1;
    }
    let mut stdout = printer.stream_for_requested_summary();
    writeln!(
        stdout,
        "Analysis incomplete at {} site{}:",
        gaps.len(),
        if gaps.len() == 1 { "" } else { "s" }
    )?;
    for (description, count) in counts {
        writeln!(stdout, "  {count} {description}")?;
    }
    Ok(())
}

fn set_colored_override(color: Option<TerminalColor>) {
    let Some(color) = color else {
        return;
    };

    match color {
        TerminalColor::Auto => {
            colored::control::unset_override();
        }
        TerminalColor::Always => {
            colored::control::set_override(true);
        }
        TerminalColor::Never => {
            colored::control::set_override(false);
        }
    }
}

/// Initializes the global rayon thread pool to never use more than `TY_MAX_PARALLELISM` threads.
fn setup_rayon() {
    ThreadPoolBuilder::default()
        .num_threads(max_parallelism().get())
        // Use a reasonably large stack size to avoid running into stack overflows too easily. The
        // size was chosen in such a way as to still be able to handle large expressions involving
        // binary operators (x + x + … + x) both during the AST walk in semantic index building as
        // well as during type checking. Using this stack size, we can handle handle expressions
        // that are several times larger than the corresponding limits in existing type checkers.
        .stack_size(16 * 1024 * 1024)
        .build_global()
        .unwrap();
}
