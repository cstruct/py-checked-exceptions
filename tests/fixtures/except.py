from simple import raises_exception


def catches_all_errors() -> None:
    try:
        raises_exception()
    except RuntimeError:
        pass


def catches_some_errors(i: int) -> None:
    try:
        if i > 0:
            raises_exception()
        else:
            raise TypeError()
    except RuntimeError:
        pass
