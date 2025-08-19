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

class MyCls:
    def has_extra_exceptions_documented_nested(self) -> None:
        """
        Raises:
            RuntimeError: Oopsie!
        """
