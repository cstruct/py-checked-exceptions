def raises_exception_recursive() -> None:
    raises_exception_recursive()
    raise RuntimeError()


def raises_exception_mutually_recursive_a(i: int) -> None:
    raises_exception_mutually_recursive_b(i)


def raises_exception_mutually_recursive_b(i: int) -> None:
    if i < 10:
        i += 1
        raises_exception_mutually_recursive_a(i)
    raise RuntimeError()
