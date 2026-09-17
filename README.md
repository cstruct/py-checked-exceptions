# py-checked-exceptions

A static analyzer that enforces exception documentation in Python code. It verifies that all raised exceptions are documented in Google-style docstrings and flags docstring exceptions that are never actually raised. Optional extensions add framework-specific behavior. Built upon the excellent foundation provided by [Ruff and Ty](https://github.com/astral-sh/ruff).

![](./demo.gif)

## Installation

```bash
cargo install --git https://github.com/cstruct/py-checked-exceptions.git
```

## Usage

To check your project simply run the CLI from the root of the project:
```console
> py-checked-exceptions check --output-format concise --target-exceptions module.path.MyBaseException
src/foo.py:55:25: error[raise] Raises undocumented error MySpecificException
Found 1 diagnostic
```

The CLI currently only provides a `check` command that can be run to perform static analysis for a project.

Generic exception specializations are tracked independently. For example, an exception raised as
`NotFoundError[User]` must be documented as `NotFoundError[User]`, while still matching a
`NotFoundError` target-exception filter or handler.

## Configuration

Project configuration can be stored in `pyproject.toml`:

```toml
[tool.py-checked-exceptions]
target-exceptions = ["package.errors.BaseError"]
extensions = ["fastapi"]
python = ".venv"
typeshed = "typings/typeshed"
extra-search-paths = ["packages/shared"]
output-format = "concise"
color = "auto"
show-analysis-gaps = "summary"
respect-ignore-files = true
exclude = ["generated", "tests/fixtures/**"]

[[tool.py-checked-exceptions.context-manager-effects]]
function = "onetwo.base.error.suppress_api_error"
exception-parameter = "error_type"
effect = "optional"
```

Paths in `pyproject.toml` are relative to the project root. Command-line options take precedence
over this section, which in turn takes precedence over overlapping settings in `[tool.ty]`.
Positional check paths, `--project`, and verbosity remain command-line-only.

Context-manager effects model application-specific exception handling. `function` is the fully
qualified context-manager function and `exception-parameter` identifies the argument containing
the affected exception type. An effect of `suppress` prevents matching exceptions (including
subclasses) from propagating out of the context manager. An effect of `optional` keeps propagating
them, but does not require them to be documented.

```
Check a project for errors documenting errors

Usage: py-checked-exceptions check [OPTIONS] [PATH]...

Arguments:
  [PATH]...
          List of files or directories to check [default: the project root]

Options:
      --project <PROJECT>
          Run the command within the given project directory.

          All `pyproject.toml` files will be discovered by walking up the directory tree from the given project directory,
          as will the project's virtual environment (`.venv`).

          Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.

      --target-exceptions <FILTER>
          Set base exceptions to target when analyzing

      --extension <EXTENSION>
          Enable an analysis extension (can be passed multiple times)

          Possible values:
          - fastapi: Model FastAPI response documentation and dependency injection

      --python <PATH>
          Path to the Python environment.

          py-checked-exceptions uses the Python environment to resolve type information and third-party dependencies.

          If not specified, py-checked-exceptions will attempt to infer it from the `VIRTUAL_ENV` or `CONDA_PREFIX`
          environment variables, or discover a `.venv` directory in the project root or working directory.

          If a path to a Python interpreter is provided, e.g., `.venv/bin/python3`, py-checked-exceptions will attempt to find
          an environment two directories up from the interpreter's path, e.g., `.venv`. At this time, py-checked-exceptions
          does not invoke the interpreter to determine the location of the environment. This means that py-checked-exceptions
          will not resolve dynamic executables such as a shim.

          py-checked-exceptions will search in the resolved environment's `site-packages` directories for type information and
          third-party imports.

      --typeshed <PATH>
          Custom directory to use for stdlib typeshed stubs

      --extra-search-path <PATH>
          Additional path to use as a module-resolution source (can be passed multiple times)

  -v, --verbose...
          Use verbose output (or `-vv` and `-vvv` for more verbose output)

  -q, --quiet...
          Use quiet output (or `-qq` for silent output)

      --output-format <OUTPUT_FORMAT>
          The format to use for printing diagnostic messages

          Possible values:
          - full:    Print diagnostics verbosely, with context and helpful hints \[default\]
          - concise: Print diagnostics concisely, one per line

      --show-analysis-gaps[=<LEVEL>]
          Report places where exception analysis is incomplete

          Possible values:
          - summary: Print counts grouped by the reason analysis was incomplete
          - full:    Print source diagnostics as well as the summary

      --color <WHEN>
          Control when colored output is used

          Possible values:
          - auto:   Display colors if the output goes to an interactive terminal
          - always: Always display colors
          - never:  Never display colors

  -h, --help
          Print help (see a summary with '-h')

File selection:
      --respect-ignore-files
          Respect file exclusions via `.gitignore` and other standard ignore files. Use `--no-respect-gitignore` to disable

      --exclude <EXCLUDE>
          Glob patterns for files to exclude from static analysis.

          Uses gitignore-style syntax to exclude files and directories from type checking.
          Supports patterns like `tests/`, `*.tmp`, `**/__pycache__/**`.
```

## FastAPI extension

Enable FastAPI-specific analysis with `--extension fastapi`. The extension:

- Treats models in route `responses` dictionaries as exception documentation
- Propagates exceptions from `Depends(...)` and `Security(...)` parameter dependencies
- Supports dependencies declared through `Annotated`
- Supports route-level `dependencies=[Depends(...)]`
- Follows nested injected dependencies
- Resolves assignment-style and PEP 695 `type` aliases for `Annotated` dependencies
- Follows dependency factories and callable dependency objects

FastAPI behavior is disabled unless the extension is explicitly enabled.

## Known Limitations

Use `--show-analysis-gaps` to print a summary of code the analyzer could not fully model, or
`--show-analysis-gaps=full` to include source diagnostics. Analysis gaps are informational and do
not change the command's exit status.

This tool currently doesn't support:
- Higher-order calls through `*args`, `**kwargs`, or dynamically stored and returned callables
- Class-based decorators and decorators that return dynamically constructed callables
- Dynamically determined `__(a)exit__` suppression
- Docstring formats other than Google style
- Dynamic exception types
- FastAPI application-, router-, and `include_router`-level dependencies

## Future Work

- Preserve exception effects through deferred callables such as `functools.partial` and `functools.partialmethod`

## Contributing

Before contributing, install the git hooks:

```bash
cargo install hk
hk install
```

This ensures all commits pass linting and tests.

## LLM disclosure

Large language models (LLMs) are used in the development of commits after the commit titled
`LLMs are used after this commit`. This may include assistance with design, implementation, tests,
documentation, and review. Maintainers review and remain responsible for all changes.
