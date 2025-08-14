class MyClass:
    def raises_exception(self) -> None:
        raise RuntimeError()

    def raises_transitive_exception(self) -> None:
        self.raises_exception()
