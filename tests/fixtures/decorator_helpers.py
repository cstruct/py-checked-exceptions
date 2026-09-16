from collections.abc import Callable
from typing import Any


def catches_runtime_error(function: Callable[..., Any]) -> Callable[..., Any]:
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        try:
            return function(*args, **kwargs)
        except RuntimeError:
            return None

    return wrapper
