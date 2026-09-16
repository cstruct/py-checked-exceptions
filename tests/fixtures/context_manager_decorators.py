from contextlib import asynccontextmanager, contextmanager


class BodyError(RuntimeError):
    pass


class SetupError(RuntimeError):
    pass


class TeardownError(RuntimeError):
    pass


@contextmanager
def transparent_manager():
    yield


@contextmanager
def suppressing_manager():
    try:
        yield
    except BodyError:
        pass


@contextmanager
def setup_error_manager():
    """
    Raises:
        SetupError: Setup failed.
    """
    raise SetupError()
    yield


@contextmanager
def teardown_error_manager():
    """
    Raises:
        TeardownError: Teardown failed.
    """
    yield
    raise TeardownError()


@contextmanager
def suppressing_teardown_manager():
    try:
        yield
    except TeardownError:
        pass


@asynccontextmanager
async def async_suppressing_manager():
    try:
        yield
    except BodyError:
        pass


@asynccontextmanager
async def async_teardown_error_manager():
    """
    Raises:
        TeardownError: Teardown failed.
    """
    yield
    raise TeardownError()


@transparent_manager()
def transparent_decorated() -> None:
    """
    Raises:
        BodyError: The function failed.
    """
    raise BodyError()


@suppressing_manager()
def suppressed_decorated() -> None:
    raise BodyError()


@setup_error_manager()
def setup_error_decorated() -> None:
    """
    Raises:
        SetupError: Setup failed.
    """
    pass


@teardown_error_manager()
def teardown_error_decorated() -> None:
    """
    Raises:
        TeardownError: Teardown failed.
    """
    pass


@suppressing_teardown_manager()
@teardown_error_manager()
def stacked_decorated() -> None:
    pass


@async_suppressing_manager()
async def async_suppressed_decorated() -> None:
    raise BodyError()


@async_teardown_error_manager()
async def async_teardown_error_decorated() -> None:
    """
    Raises:
        TeardownError: Teardown failed.
    """
    pass


def transparent_decorator_propagates_body_error() -> None:
    transparent_decorated()


def context_manager_decorator_suppresses_body_error() -> None:
    suppressed_decorated()


def context_manager_decorator_propagates_setup_error() -> None:
    setup_error_decorated()


def context_manager_decorator_propagates_teardown_error() -> None:
    teardown_error_decorated()


def outer_context_manager_decorator_suppresses_inner_teardown_error() -> None:
    stacked_decorated()


async def async_context_manager_decorator_suppresses_body_error() -> None:
    await async_suppressed_decorated()


async def async_context_manager_decorator_propagates_teardown_error() -> None:
    await async_teardown_error_decorated()
