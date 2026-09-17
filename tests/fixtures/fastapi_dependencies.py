from typing import Annotated


class DependencyError(RuntimeError):
    pass


class SecurityError(RuntimeError):
    pass


def Depends(dependency=None):
    return dependency


def Security(dependency=None, *, scopes=None):
    return dependency


def dependency():
    """Resolve a dependency.

    Raises:
        DependencyError: If dependency resolution fails.
    """
    raise DependencyError()


def security_dependency():
    """Resolve a security dependency.

    Raises:
        SecurityError: If authorization fails.
    """
    raise SecurityError()


def nested_dependency(value=Depends(dependency)):
    """Resolve a nested dependency.

    Raises:
        DependencyError: If dependency resolution fails.
    """
    return value


class Router:
    def get(self, *args, **kwargs):
        def decorator(function):
            return function

        return decorator


router = Router()


@router.get("/default")
def default_dependency(value=Depends(dependency)):
    return value


@router.get("/annotated")
def annotated_dependency(value: Annotated[str, Depends(dependency)]):
    return value


@router.get("/decorator", dependencies=[Depends(dependency)])
def decorator_dependency():
    return None


@router.get("/security")
def security(value: Annotated[str, Security(security_dependency, scopes=["read"])]):
    return value


@router.get("/nested")
def nested(value=Depends(nested_dependency)):
    return value


@router.get(
    "/documented",
    responses={400: {"model": DependencyError}},
)
def documented_dependency(value=Depends(dependency)):
    return value


AliasDependency = Annotated[str, Depends(dependency)]


@router.get("/alias")
def aliased_dependency(value: AliasDependency = Depends()):
    return value


class FactoryError(RuntimeError):
    pass


class CallableDependency:
    def __call__(self):
        """Resolve a callable dependency.

        Raises:
            FactoryError: If dependency resolution fails.
        """
        raise FactoryError()


def dependency_factory():
    return Depends(CallableDependency())


@router.get("/factory", dependencies=[dependency_factory()])
def factory_dependency():
    return None


dynamic_dependencies = [Depends(dependency)]


@router.get("/dynamic", dependencies=dynamic_dependencies)
def dynamic_dependency_list():
    return None


type Pep695AliasDependency = Annotated[str, Depends(dependency)]


@router.get("/pep-695-alias")
def pep_695_aliased_dependency(value: Pep695AliasDependency = Depends()):
    return value
