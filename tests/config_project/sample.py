from errors import ConfiguredError, optional_errors


class OtherError(RuntimeError):
    pass


class Router:
    def get(self, *args, **kwargs):
        def decorator(function):
            return function

        return decorator


router = Router()


@router.get("/documented", responses={400: {"model": ConfiguredError}})
def documented_by_fastapi_extension():
    raise ConfiguredError()


def undocumented():
    raise ConfiguredError()


def not_targeted():
    raise OtherError()
def optional_by_config():
    with optional_errors(ConfiguredError):
        raise ConfiguredError()
