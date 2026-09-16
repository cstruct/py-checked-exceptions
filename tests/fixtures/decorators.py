from collections.abc import Callable
from typing import Any


def catches_runtime_error(function: Callable[..., Any]) -> Callable[..., Any]:
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        try:
            return function(*args, **kwargs)
        except RuntimeError:
            return None

    return wrapper


def catches_exception(function: Callable[..., Any]) -> Callable[..., Any]:
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        try:
            return function(*args, **kwargs)
        except Exception:
            return None

    return wrapper


def catches_value_error(function: Callable[..., Any]) -> Callable[..., Any]:
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        try:
            return function(*args, **kwargs)
        except ValueError:
            return None

    return wrapper


def identity(function: Callable[..., Any]) -> Callable[..., Any]:
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        return function(*args, **kwargs)

    return wrapper


def catches_runtime_error_factory() -> Callable[..., Any]:
    def decorator(function: Callable[..., Any]) -> Callable[..., Any]:
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            try:
                return function(*args, **kwargs)
            except RuntimeError:
                return None

        return wrapper

    return decorator


@catches_runtime_error
def caught_by_exact_type() -> None:
    raise RuntimeError()


@catches_exception
def caught_by_base_type() -> None:
    raise RuntimeError()


@catches_runtime_error_factory()
def caught_by_decorator_factory() -> None:
    raise RuntimeError()


@catches_runtime_error
@identity
def caught_by_outer_decorator() -> None:
    raise RuntimeError()


@catches_value_error
def not_caught_by_different_type() -> None:
    raise RuntimeError()


@identity
def not_caught_by_identity_decorator() -> None:
    raise RuntimeError()


def call_caught_by_exact_type() -> None:
    caught_by_exact_type()


def call_caught_by_base_type() -> None:
    caught_by_base_type()


def call_caught_by_decorator_factory() -> None:
    caught_by_decorator_factory()


def call_caught_by_outer_decorator() -> None:
    caught_by_outer_decorator()


def call_not_caught_by_different_type() -> None:
    not_caught_by_different_type()


def call_not_caught_by_identity_decorator() -> None:
    not_caught_by_identity_decorator()


import decorator_helpers
from decorator_helpers import catches_runtime_error as imported_catcher


@decorator_helpers.catches_runtime_error
def caught_by_decorator_attribute() -> None:
    raise RuntimeError()


@imported_catcher
def caught_by_imported_decorator() -> None:
    raise RuntimeError()


def call_caught_by_decorator_attribute() -> None:
    caught_by_decorator_attribute()


def call_caught_by_imported_decorator() -> None:
    caught_by_imported_decorator()
