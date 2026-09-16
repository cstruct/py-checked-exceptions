class CallbackError(RuntimeError):
    pass


class FirstCallbackError(RuntimeError):
    pass


class SecondCallbackError(RuntimeError):
    pass


class MethodCallbackError(RuntimeError):
    pass


def raises_callback_error() -> None:
    """
    Raises:
        CallbackError: The callback failed.
    """
    raise CallbackError()


def raises_first_callback_error() -> None:
    """
    Raises:
        FirstCallbackError: The first callback failed.
    """
    raise FirstCallbackError()


def raises_second_callback_error() -> None:
    """
    Raises:
        SecondCallbackError: The second callback failed.
    """
    raise SecondCallbackError()


class Worker:
    def fail(self) -> None:
        """
        Raises:
            MethodCallbackError: The method callback failed.
        """
        raise MethodCallbackError()


def invoke(callback) -> None:
    callback()


def invoke_keyword(*, callback) -> None:
    callback()


def invoke_two(first, second) -> None:
    first()
    second()


def invoke_with_value(value: object, callback) -> None:
    callback(value)


def forward(callback) -> None:
    invoke(callback)


def forward_twice(callback) -> None:
    forward(callback)


def catch_callback_error(callback) -> None:
    try:
        callback()
    except CallbackError:
        pass


def ignore_callback(callback) -> None:
    pass


def direct_callback_propagates() -> None:
    invoke(raises_callback_error)


def keyword_callback_propagates() -> None:
    invoke_keyword(callback=raises_callback_error)


def multiple_callbacks_propagate() -> None:
    """
    Raises:
        FirstCallbackError: The first callback failed.
    """
    invoke_two(raises_first_callback_error, raises_second_callback_error)


def callback_after_ordinary_argument_propagates() -> None:
    invoke_with_value(object(), raises_callback_error)


def forwarded_callback_propagates() -> None:
    forward_twice(raises_callback_error)


def callback_can_be_caught_by_higher_order_function() -> None:
    catch_callback_error(raises_callback_error)


def callback_is_not_assumed_to_run() -> None:
    ignore_callback(raises_callback_error)


def bound_method_callback_propagates() -> None:
    worker = Worker()
    invoke(worker.fail)


def caller_can_catch_callback_error() -> None:
    try:
        invoke(raises_callback_error)
    except CallbackError:
        pass


class CallbackInvoker:
    def invoke(self, callback) -> None:
        callback()


def higher_order_method_propagates() -> None:
    invoker = CallbackInvoker()
    invoker.invoke(raises_callback_error)
