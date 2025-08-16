use clap::Parser;
use ruff_db::system::SystemPathBuf;

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
    /// Run the command within the given project directory.
    ///
    /// All `pyproject.toml` files will be discovered by walking up the directory tree from the given project directory,
    /// as will the project's virtual environment (`.venv`).
    ///
    /// Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.
    #[arg(long, value_name = "PROJECT")]
    pub(crate) project: Option<SystemPathBuf>,

    /// Filter what files to check.
    #[arg(long, value_name = "FILTER")]
    pub(crate) filter: Option<SystemPathBuf>,

    /// Set base exceptions to target when analyzing.
    #[arg(long, value_name = "FILTER")]
    pub(crate) target_exceptions: Vec<String>,
}
