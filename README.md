# py-checked-exceptions

A static analyzer that enforces exception documentation in Python code. It verifies that all raised exceptions are documented in docstrings and flags any documented exceptions that are never actually raised. Built upon the excellent foundation provided by [Ruff and Ty](https://github.com/astral-sh/ruff).

## Known Limitations

This tool currently doesn't support:
- Higher-order functions and exception propagation through them
- Exception handling in decorators
- Proper context manager support (`__(a)enter__`/`__(a)exit__` methods)
- Docstring formats other than Google style
- Dynamic exception types and re-raising

## Installation

```bash
cargo install py-checked-exceptions
```

## Contributing

Before contributing, install the git hooks:

```bash
cargo install hk
hk install
```

This ensures all commits pass linting and tests.
