use clap::Parser;
use ruff_db::system::SystemPathBuf;
use ty_project::metadata::{
    Options,
    options::{EnvironmentOptions, SrcOptions, TerminalOptions},
    value::{RangedValue, RelativeGlobPattern, RelativePathBuf},
};

use crate::logging::Verbosity;
use py_checked_exceptions::AnalysisExtension;

#[derive(Debug, Parser)]
#[command(
    author,
    name = "py-checked-exceptions",
    about = "Static-analysize tool for making sure you document your exceptions."
)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, clap::Subcommand)]
pub(crate) enum Command {
    /// Check a project for errors documenting errors.
    Check(CheckCommand),
}

#[derive(Debug, Parser)]
pub(crate) struct CheckCommand {
    /// List of files or directories to check.
    #[clap(
        help = "List of files or directories to check [default: the project root]",
        value_name = "PATH"
    )]
    pub paths: Vec<SystemPathBuf>,

    /// Run the command within the given project directory.
    ///
    /// All `pyproject.toml` files will be discovered by walking up the directory tree from the given project directory,
    /// as will the project's virtual environment (`.venv`).
    ///
    /// Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.
    #[arg(long, value_name = "PROJECT")]
    pub(crate) project: Option<SystemPathBuf>,

    /// Set base exceptions to target when analyzing.
    #[arg(long, value_name = "FILTER")]
    pub(crate) target_exceptions: Vec<String>,

    /// Enable an analysis extension (can be passed multiple times).
    #[arg(long = "extension", value_name = "EXTENSION")]
    pub(crate) extensions: Vec<AnalysisExtension>,

    /// Path to the Python environment.
    ///
    /// py-checked-exceptions uses the Python environment to resolve type information and third-party dependencies.
    ///
    /// If not specified, py-checked-exceptions will attempt to infer it from the `VIRTUAL_ENV` or `CONDA_PREFIX`
    /// environment variables, or discover a `.venv` directory in the project root or working
    /// directory.
    ///
    /// If a path to a Python interpreter is provided, e.g., `.venv/bin/python3`, py-checked-exceptions will attempt to
    /// find an environment two directories up from the interpreter's path, e.g., `.venv`. At this
    /// time, py-checked-exceptions does not invoke the interpreter to determine the location of the environment. This
    /// means that py-checked-exceptions will not resolve dynamic executables such as a shim.
    ///
    /// py-checked-exceptions will search in the resolved environment's `site-packages` directories for type
    /// information and third-party imports.
    #[arg(long, value_name = "PATH")]
    pub(crate) python: Option<SystemPathBuf>,

    /// Additional path to use as a module-resolution source (can be passed multiple times).
    #[arg(long, value_name = "PATH")]
    pub(crate) extra_search_path: Option<Vec<SystemPathBuf>>,

    #[clap(flatten)]
    pub(crate) verbosity: Verbosity,

    /// The format to use for printing diagnostic messages.
    #[arg(long)]
    pub(crate) output_format: Option<OutputFormat>,

    /// Report places where exception analysis is incomplete.
    #[arg(
        long = "show-analysis-gaps",
        value_name = "LEVEL",
        default_missing_value = "summary",
        num_args = 0..=1,
        require_equals = true
    )]
    pub(crate) analysis_gaps: Option<AnalysisGapOutput>,

    /// Control when colored output is used.
    #[arg(long, value_name = "WHEN")]
    pub(crate) color: Option<TerminalColor>,

    /// Respect file exclusions via `.gitignore` and other standard ignore files.
    /// Use `--no-respect-gitignore` to disable.
    #[arg(
        long,
        overrides_with("no_respect_ignore_files"),
        help_heading = "File selection",
        default_missing_value = "true",
        num_args = 0..1
    )]
    respect_ignore_files: Option<bool>,
    #[clap(long, overrides_with("respect_ignore_files"), hide = true)]
    no_respect_ignore_files: bool,

    /// Glob patterns for files to exclude from static analysis.
    ///
    /// Uses gitignore-style syntax to exclude files and directories from type checking.
    /// Supports patterns like `tests/`, `*.tmp`, `**/__pycache__/**`.
    #[arg(long, help_heading = "File selection")]
    exclude: Option<Vec<String>>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum AnalysisGapOutput {
    /// Print counts grouped by the reason analysis was incomplete.
    Summary,
    /// Print source diagnostics as well as the summary.
    Full,
}

impl CheckCommand {
    pub(crate) fn options(&self) -> Options {
        // --no-respect-gitignore defaults to false and is set true by CLI flag. If passed, override config file
        // Otherwise, only pass this through if explicitly set (don't default to anything here to
        // make sure that doesn't take precedence over an explicitly-set config file value)
        let respect_ignore_files = self
            .no_respect_ignore_files
            .then_some(false)
            .or(self.respect_ignore_files);
        Options {
            environment: Some(EnvironmentOptions {
                python_version: None,
                python_platform: None,
                python: self.python.clone().map(RelativePathBuf::cli),
                typeshed: None,
                extra_paths: self.extra_search_path.clone().map(|extra_search_paths| {
                    extra_search_paths
                        .into_iter()
                        .map(RelativePathBuf::cli)
                        .collect()
                }),
                ..EnvironmentOptions::default()
            }),
            terminal: Some(TerminalOptions {
                output_format: self
                    .output_format
                    .map(|output_format| RangedValue::cli(output_format.into())),
                error_on_warning: None,
            }),
            src: Some(SrcOptions {
                respect_ignore_files,
                exclude: self.exclude.clone().map(|excludes| {
                    RangedValue::cli(excludes.iter().map(RelativeGlobPattern::cli).collect())
                }),
                ..SrcOptions::default()
            }),
            rules: None,
            ..Options::default()
        }
    }
}

/// The diagnostic output format.
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// Print diagnostics verbosely, with context and helpful hints \[default\].
    ///
    /// Diagnostic messages may include additional context and
    /// annotations on the input to help understand the message.
    #[default]
    #[value(name = "full")]
    Full,
    /// Print diagnostics concisely, one per line.
    ///
    /// This will guarantee that each diagnostic is printed on
    /// a single line. Only the most important or primary aspects
    /// of the diagnostic are included. Contextual information is
    /// dropped.
    #[value(name = "concise")]
    Concise,
}

impl From<OutputFormat> for ty_project::metadata::options::OutputFormat {
    fn from(format: OutputFormat) -> ty_project::metadata::options::OutputFormat {
        match format {
            OutputFormat::Full => Self::Full,
            OutputFormat::Concise => Self::Concise,
        }
    }
}

/// Control when colored output is used.
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default, clap::ValueEnum)]
pub(crate) enum TerminalColor {
    /// Display colors if the output goes to an interactive terminal.
    #[default]
    Auto,

    /// Always display colors.
    Always,

    /// Never display colors.
    Never,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{AnalysisExtension, AnalysisGapOutput, Cli, Command};

    #[test]
    fn analysis_gap_output_is_opt_in() {
        let cli = Cli::try_parse_from(["py-checked-exceptions", "check"]).unwrap();
        let Command::Check(check) = cli.command;
        assert_eq!(check.analysis_gaps, None);
    }

    #[test]
    fn analysis_gap_output_defaults_to_summary() {
        let cli = Cli::try_parse_from(["py-checked-exceptions", "check", "--show-analysis-gaps"])
            .unwrap();
        let Command::Check(check) = cli.command;
        assert_eq!(check.analysis_gaps, Some(AnalysisGapOutput::Summary));
    }

    #[test]
    fn analysis_gap_output_accepts_full() {
        let cli = Cli::try_parse_from([
            "py-checked-exceptions",
            "check",
            "--show-analysis-gaps=full",
        ])
        .unwrap();
        let Command::Check(check) = cli.command;
        assert_eq!(check.analysis_gaps, Some(AnalysisGapOutput::Full));
    }

    #[test]
    fn accepts_fastapi_extension() {
        let cli = Cli::try_parse_from(["py-checked-exceptions", "check", "--extension", "fastapi"])
            .unwrap();
        let Command::Check(check) = cli.command;
        assert_eq!(check.extensions, vec![AnalysisExtension::Fastapi]);
    }
}
