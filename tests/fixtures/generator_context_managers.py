import contextlib
from contextlib import asynccontextmanager as async_cm
from contextlib import contextmanager as cm


class BodyError(RuntimeError):
    pass


class OtherError(RuntimeError):
    pass


class EnterError(RuntimeError):
    pass


class ExitError(RuntimeError):
    pass


@cm
def transparent_generator():
    yield


@contextlib.contextmanager
def suppressing_generator():
    try:
        yield
    except BodyError:
        pass


@cm
def reraising_generator():
    """
    Raises:
        BodyError: The body failed.
    """
    try:
        yield
    except BodyError:
        raise


@cm
def generator_enter_raises():
    """
    Raises:
        EnterError: Setup failed.
    """
    raise EnterError()
    yield


@cm
def generator_exit_raises():
    """
    Raises:
        ExitError: Teardown failed.
    """
    yield
    raise ExitError()


@cm
def suppressing_enter_generator():
    try:
        yield
    except EnterError:
        pass


@async_cm
async def async_suppressing_generator():
    try:
        yield
    except BodyError:
        pass


@async_cm
async def async_generator_enter_raises():
    """
    Raises:
        EnterError: Setup failed.
    """
    raise EnterError()
    yield


@async_cm
async def async_generator_exit_raises():
    """
    Raises:
        ExitError: Teardown failed.
    """
    yield
    raise ExitError()


def argument_raises() -> None:
    """
    Raises:
        EnterError: Argument evaluation failed.
    """
    raise EnterError()


@cm
def generator_with_argument(value: object):
    yield value


def transparent_generator_propagates_body_error() -> None:
    with transparent_generator():
        raise BodyError()


def generator_can_suppress_body_error() -> None:
    with suppressing_generator():
        raise BodyError()


def generator_does_not_suppress_other_errors() -> None:
    with suppressing_generator():
        raise OtherError()


def generator_can_reraise_body_error() -> None:
    with reraising_generator():
        raise BodyError()


def generator_setup_error_propagates() -> None:
    with generator_enter_raises():
        pass


def generator_teardown_error_propagates() -> None:
    with generator_exit_raises():
        pass


def outer_generator_suppresses_inner_enter_error() -> None:
    with suppressing_enter_generator(), generator_enter_raises():
        pass


def generator_arguments_are_evaluated() -> None:
    with generator_with_argument(argument_raises()):
        pass


async def async_generator_can_suppress_body_error() -> None:
    async with async_suppressing_generator():
        raise BodyError()


async def async_generator_setup_error_propagates() -> None:
    async with async_generator_enter_raises():
        pass


async def async_generator_teardown_error_propagates() -> None:
    async with async_generator_exit_raises():
        pass


def creating_generator_context_manager_does_not_run_it() -> None:
    generator_enter_raises()


def assigned_generator_can_suppress_body_error() -> None:
    manager = suppressing_generator()
    with manager:
        raise BodyError()


def assigned_generator_setup_error_propagates() -> None:
    manager = generator_enter_raises()
    with manager:
        pass


generator_factory = generator_exit_raises


def assigned_generator_factory_is_resolved() -> None:
    with generator_factory():
        pass


async def assigned_async_generator_teardown_error_propagates() -> None:
    manager = async_generator_exit_raises()
    async with manager:
        pass
