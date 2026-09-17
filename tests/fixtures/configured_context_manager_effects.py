from contextlib import contextmanager


class ConfiguredError(RuntimeError):
    pass


class OtherError(RuntimeError):
    pass


@contextmanager
def optional_errors(error_type):
    try:
        yield
    except error_type:
        raise


@contextmanager
def suppress_errors(error_type):
    try:
        yield
    except error_type:
        raise


def configured_error_is_optional():
    with optional_errors(ConfiguredError):
        raise ConfiguredError()


def optional_error_can_still_be_documented():
    """Raise an optional error.

    Raises:
        ConfiguredError: If the operation fails.
    """
    with optional_errors(ConfiguredError):
        raise ConfiguredError()


def other_error_is_still_required():
    with optional_errors(ConfiguredError):
        raise OtherError()


def required_path_wins(make_optional: bool):
    if make_optional:
        with optional_errors(ConfiguredError):
            raise ConfiguredError()
    raise ConfiguredError()


def configured_error_is_suppressed():
    with suppress_errors(ConfiguredError):
        raise ConfiguredError()


def suppression_does_not_hide_other_errors():
    with suppress_errors(ConfiguredError):
        raise OtherError()


from typing import Generic, TypeVar

T = TypeVar("T")


class GenericConfiguredError(RuntimeError, Generic[T]):
    pass


class Payload:
    pass


def generic_configured_error_is_optional():
    with optional_errors(GenericConfiguredError[Payload]):
        raise GenericConfiguredError[Payload]()
