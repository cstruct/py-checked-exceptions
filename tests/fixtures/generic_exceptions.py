from typing import Generic, TypeVar


T = TypeVar("T")


class User:
    pass


class NotFoundError(Exception, Generic[T]):
    pass


def documented():
    """Fetch a user.

    Raises:
        NotFoundError[User]: If the user does not exist.
    """
    raise NotFoundError[ User ]()


def source():
    """Fetch a user.

    Raises:
        NotFoundError[User]: If the user does not exist.
    """
    raise NotFoundError[User]()


def transitive():
    source()


def documented_as_unspecialized():
    """Fetch a user.

    Raises:
        NotFoundError: If the user does not exist.
    """
    raise NotFoundError[User]()


def caught_by_unspecialized_base():
    try:
        raise NotFoundError[User]()
    except NotFoundError:
        pass


class Router:
    def get(self, *args, **kwargs):
        def decorator(function):
            return function

        return decorator


router = Router()


@router.get("/users/{user_id}", responses={404: {"model": NotFoundError[User]}})
def fastapi_documented():
    raise NotFoundError[User]()
