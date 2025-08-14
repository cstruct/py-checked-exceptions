class MyError(Exception): ...


def raises_exception(i: int) -> None:
    if i > 0:
        raise MyError()
    raise RuntimeError()
