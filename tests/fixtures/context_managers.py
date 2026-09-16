class InitError(RuntimeError):
    pass


class EnterError(RuntimeError):
    pass


class ExitError(RuntimeError):
    pass


class BodyError(RuntimeError):
    pass


class AsyncEnterError(RuntimeError):
    pass


class ConstructionOnly:
    def __enter__(self):
        """
        Raises:
            EnterError: Enter failed.
        """
        raise EnterError()

    def __exit__(self, exc_type, exc_value, traceback):
        """
        Raises:
            ExitError: Exit failed.
        """
        raise ExitError()


class InitRaises:
    def __init__(self):
        """
        Raises:
            InitError: Initialization failed.
        """
        raise InitError()

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        return False


class EnterRaises:
    def __enter__(self):
        """
        Raises:
            EnterError: Enter failed.
        """
        raise EnterError()

    def __exit__(self, exc_type, exc_value, traceback):
        return False


class ExitRaises:
    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        """
        Raises:
            ExitError: Exit failed.
        """
        raise ExitError()


class NonSuppressing:
    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        return False


class Suppressing:
    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        return True


class SuppressingExitRaises:
    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        """
        Raises:
            ExitError: Exit failed.
        """
        raise ExitError()
        return True


class AsyncEnterRaises:
    async def __aenter__(self):
        """
        Raises:
            AsyncEnterError: Enter failed.
        """
        raise AsyncEnterError()

    async def __aexit__(self, exc_type, exc_value, traceback):
        return False


def construct_without_with() -> None:
    ConstructionOnly()


def init_error_propagates() -> None:
    with InitRaises():
        pass


def enter_error_propagates() -> None:
    with EnterRaises():
        pass


def exit_error_propagates() -> None:
    with ExitRaises():
        pass


def body_error_propagates() -> None:
    with NonSuppressing():
        raise BodyError()


def body_error_is_suppressed() -> None:
    with Suppressing():
        raise BodyError()


def exit_error_is_not_suppressed_by_itself() -> None:
    with SuppressingExitRaises():
        raise BodyError()


def outer_manager_suppresses_inner_enter_error() -> None:
    with Suppressing(), EnterRaises():
        pass


def outer_manager_suppresses_inner_exit_error() -> None:
    with Suppressing(), ExitRaises():
        pass


async def async_enter_error_propagates() -> None:
    async with AsyncEnterRaises():
        pass


def assigned_manager_enter_error_propagates() -> None:
    manager = EnterRaises()
    with manager:
        pass


def assigned_manager_suppresses_body_error() -> None:
    manager = Suppressing()
    with manager:
        raise BodyError()


class InheritedEnterRaises(EnterRaises):
    pass


class InheritedSuppressing(Suppressing):
    pass


def inherited_enter_error_propagates() -> None:
    with InheritedEnterRaises():
        pass


def inherited_exit_suppresses_body_error() -> None:
    with InheritedSuppressing():
        raise BodyError()


class AsyncSuppressing:
    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc_value, traceback):
        return True


class AsyncExitRaises:
    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc_value, traceback):
        """
        Raises:
            ExitError: Exit failed.
        """
        raise ExitError()


async def async_exit_suppresses_body_error() -> None:
    async with AsyncSuppressing():
        raise BodyError()


async def async_exit_error_propagates() -> None:
    async with AsyncExitRaises():
        pass


import contextlib
from contextlib import suppress as suppress_exceptions


class SpecificBodyError(BodyError):
    pass


def contextlib_suppress_exact_exception() -> None:
    with suppress_exceptions(BodyError):
        raise BodyError()


def contextlib_suppress_base_exception() -> None:
    with contextlib.suppress(BodyError):
        raise SpecificBodyError()


def contextlib_suppress_multiple_exceptions(use_body_error: bool) -> None:
    with suppress_exceptions(BodyError, ExitError):
        if use_body_error:
            raise BodyError()
        raise ExitError()


def contextlib_suppress_does_not_hide_other_exceptions() -> None:
    with suppress_exceptions(BodyError):
        raise ExitError()


def contextlib_suppress_handles_inner_enter_error() -> None:
    with suppress_exceptions(EnterError), EnterRaises():
        pass


def contextlib_suppress_handles_inner_exit_error() -> None:
    with suppress_exceptions(ExitError), ExitRaises():
        pass
