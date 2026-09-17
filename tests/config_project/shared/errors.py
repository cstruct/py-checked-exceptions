from contextlib import contextmanager


class ConfiguredError(RuntimeError):
    pass


@contextmanager
def optional_errors(error_type):
    yield
