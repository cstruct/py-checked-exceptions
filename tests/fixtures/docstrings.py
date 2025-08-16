def has_all_exceptions_documented() -> None:
    """
    Raises:
        RuntimeError: Oopsie!
    """
    raise RuntimeError()


def has_extra_exceptions_documented() -> None:
    """
    Raises:
        RuntimeError: Oopsie!
    """
