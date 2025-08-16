from simple2 import raises_exception


def catches_all_errors() -> None:
    try:
        raises_exception()
    except FileNotFoundError:
        pass


def catches_some_errors(i: int) -> None:
    try:
        if i > 0:
            raises_exception()
        else:
            raise TypeError()
    except FileNotFoundError:
        pass


def bare_reraise() -> None:
    try:
        raises_exception()
    except FileNotFoundError:
        raise


def named_reraise() -> None:
    try:
        raises_exception()
    except FileNotFoundError as e:
        raise e


def catch_all_reraise() -> None:
    try:
        raises_exception()
    except:
        raise


def catch_all_no_reraise() -> None:
    try:
        raises_exception()
    except:
        pass


def multiple_except_blocks() -> None:
    try:
        raises_exception()
    except ValueError:
        pass
    except FileNotFoundError:
        raise
    except Exception:
        pass


def nested_try_reraise() -> None:
    try:
        try:
            raises_exception()
        except FileNotFoundError:
            raise
    except FileNotFoundError:
        pass


def conditional_reraise(should_reraise: bool) -> None:
    try:
        raises_exception()
    except FileNotFoundError as e:
        if should_reraise:
            raise e
