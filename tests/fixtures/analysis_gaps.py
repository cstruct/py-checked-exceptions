from opaque_native import native_call


def passthrough_decorator(function):
    return function


decorator = passthrough_decorator


@decorator
def decorated():
    native_call()


def dynamic(registry, name):
    registry[name]()


def context_manager(factory):
    with factory():
        pass


def higher_order(callback):
    callback()


def unsupported_callback():
    higher_order(lambda: None)
