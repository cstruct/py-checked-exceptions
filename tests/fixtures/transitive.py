from simple import raises_exception


def raises_transitive_exception() -> None:
    raises_exception()

def raises_transitive_exception_indirection() -> None:
    func = raises_exception
    func()
