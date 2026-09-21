class RouteError(RuntimeError):
    pass


class Router:
    def get(self, *args, **kwargs):
        def decorator(function):
            return function

        return decorator


router = Router()


def raises_from_helper() -> None:
    raise RouteError()


@router.get("/route")
def route() -> None:
    raises_from_helper()


def unrelated() -> None:
    raise RouteError()
