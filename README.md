# py-checked-exceptions

A static analyzer that enforces exception documentation in Python code. It verifies that all raised exceptions are documented in docstrings and flags any documented exceptions that are never actually raised. Built upon the excellent foundation provided by [Ruff and Ty](https://github.com/astral-sh/ruff).

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
Found 11 diagnostics
```

The CLI currently only provides a `check` command that can be run to perform static analysis for a project.

```
Check a project for errors documenting errors

Usage: py-checked-exceptions check [OPTIONS] [PATH]...

Arguments:
  [PATH]...
          List of files or directories to check [default: the project root]

Options:
      --project <PROJECT>
          Run the command within the given project directory.

          All `pyproject.toml` files will be discovered by walking up the directory tree from the given project directory, as will the project's virtual environment (`.venv`).

          Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.

      --target-exceptions <FILTER>
          Set base exceptions to target when analyzing

      --python <PATH>
          Path to the Python environment.

          ty uses the Python environment to resolve type information and third-party dependencies.

          If not specified, ty will attempt to infer it from the `VIRTUAL_ENV` or `CONDA_PREFIX` environment variables, or discover a `.venv` directory in the project root or working directory.

          If a path to a Python interpreter is provided, e.g., `.venv/bin/python3`, ty will attempt to find an environment two directories up from the interpreter's path, e.g., `.venv`. At this time, ty does not invoke the interpreter to determine the location of the environment. This means that ty will not resolve dynamic executables such as a shim.

          ty will search in the resolved environment's `site-packages` directories for type information and third-party imports.

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

          Uses gitignore-style syntax to exclude files and directories from type checking. Supports patterns like `tests/`, `*.tmp`, `**/__pycache__/**`.
```

## Known Limitations

This tool currently doesn't support:
- Higher-order functions and exception propagation through them
- Exception handling in decorators
- Proper context manager support (`__(a)enter__`/`__(a)exit__` methods)
- Docstring formats other than Google style
- Dynamic exception types

## Contributing

Before contributing, install the git hooks:

```bash
cargo install hk
hk install
```

This ensures all commits pass linting and tests.
