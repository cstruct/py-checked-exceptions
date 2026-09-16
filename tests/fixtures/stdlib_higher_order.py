import re
from functools import reduce


class KeyCallbackError(RuntimeError):
    pass


class ReduceCallbackError(RuntimeError):
    pass


class ReplacementCallbackError(RuntimeError):
    pass


def raises_key_callback(value: object) -> object:
    """
    Raises:
        KeyCallbackError: Extracting the key failed.
    """
    raise KeyCallbackError()


def raises_reduce_callback(left: object, right: object) -> object:
    """
    Raises:
        ReduceCallbackError: Reducing the values failed.
    """
    raise ReduceCallbackError()


def raises_replacement_callback(match: re.Match[str]) -> str:
    """
    Raises:
        ReplacementCallbackError: Producing the replacement failed.
    """
    raise ReplacementCallbackError()


def sorted_propagates_key_error() -> None:
    sorted([1], key=raises_key_callback)


def min_propagates_key_error() -> None:
    min([1], key=raises_key_callback)


def max_propagates_key_error() -> None:
    max([1], key=raises_key_callback)


def list_sort_propagates_key_error() -> None:
    values = [1]
    values.sort(key=raises_key_callback)


def reduce_propagates_callback_error() -> None:
    reduce(raises_reduce_callback, [1, 2])


def re_sub_propagates_replacement_error() -> None:
    re.sub("pattern", raises_replacement_callback, "value")


def re_subn_propagates_replacement_error() -> None:
    re.subn("pattern", raises_replacement_callback, "value")


def map_does_not_eagerly_invoke_callback() -> None:
    map(raises_key_callback, [1])


def sort_with_callback(callback) -> None:
    sorted([1], key=callback)


def sort_with_caught_callback_error(callback) -> None:
    try:
        sorted([1], key=callback)
    except KeyCallbackError:
        pass


def forwarded_stdlib_callback_propagates() -> None:
    sort_with_callback(raises_key_callback)


def forwarded_stdlib_callback_can_be_caught() -> None:
    sort_with_caught_callback_error(raises_key_callback)
