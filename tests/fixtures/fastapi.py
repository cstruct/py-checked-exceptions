from typing import Annotated, Union


class DirectError(RuntimeError):
    pass


class UnionError(RuntimeError):
    pass


class GenericError(RuntimeError):
    pass


class ExtraError(RuntimeError):
    pass


class Router:
    def get(self, *args, **kwargs):
        def decorator(function):
            return function

        return decorator

    post = get
    put = get
    patch = get


router = Router()


@router.get("/direct", responses={400: {"model": DirectError}})
def direct_response_model() -> None:
    raise DirectError()


@router.post(
    "/union",
    responses={400: {"model": UnionError | GenericError[str]}},
)
def union_and_generic_response_models(use_generic: bool) -> None:
    if use_generic:
        raise GenericError()
    raise UnionError()


@router.put(
    "/typing",
    responses={
        400: {"model": Union[DirectError, UnionError]},
        401: {"model": Annotated[GenericError, "metadata"]},
    },
)
def typing_response_models(value: int) -> None:
    if value == 1:
        raise DirectError()
    if value == 2:
        raise UnionError()
    raise GenericError()


@router.patch("/extra", responses={400: {"model": ExtraError}})
def extra_response_model() -> None:
    pass


@router.patch(
    "/undocumented",
    response_model=DirectError,
    responses={400: {"description": "No response model"}},
)
def response_model_is_not_error_documentation() -> None:
    raise DirectError()


def other_decorator(*args, **kwargs):
    return router.get(*args, **kwargs)


@other_decorator(responses={400: {"model": DirectError}})
def non_fastapi_decorator_is_not_error_documentation() -> None:
    raise DirectError()
