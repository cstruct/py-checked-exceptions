class AliasError(RuntimeError):
    pass


def raises_alias_error() -> None:
    raise AliasError()


exported_raises = raises_alias_error
