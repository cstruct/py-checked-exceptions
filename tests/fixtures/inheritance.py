class CustomBaseError(Exception):
    pass

class CustomValueError(CustomBaseError):
    pass

class CustomTypeError(CustomBaseError):
    pass

class DeepError(CustomValueError):
    pass

class MultipleInheritanceError(CustomValueError, CustomTypeError):
    pass


def catch_base_should_catch_derived() -> None:
    try:
        raise CustomValueError()
    except CustomBaseError:
        pass


def catch_specific_should_not_catch_base() -> None:
    try:
        raise CustomBaseError()
    except CustomValueError:
        pass


def catch_deep_inheritance() -> None:
    try:
        raise DeepError()
    except CustomBaseError:
        pass


def catch_specific_should_catch_specific() -> None:
    try:
        raise CustomValueError()
    except CustomValueError:
        pass


def multiple_inheritance_handlers() -> None:
    try:
        raise MultipleInheritanceError()
    except CustomValueError:
        pass
    try:
        raise MultipleInheritanceError()
    except CustomTypeError:
        pass


def reraise_with_inheritance() -> None:
    try:
        raise CustomValueError()
    except CustomBaseError:
        raise


def conditional_catch_inheritance(use_base: bool) -> None:
    try:
        if use_base:
            raise CustomBaseError()
        else:
            raise CustomValueError()
    except CustomBaseError:
        pass


def nested_inheritance_handling() -> None:
    try:
        try:
            raise DeepError()
        except CustomValueError:
            raise
    except CustomBaseError:
        pass


def multiple_derived_exceptions(error_type: int) -> None:
    try:
        if error_type == 1:
            raise CustomValueError()
        elif error_type == 2:
            raise CustomTypeError()
        else:
            raise DeepError()
    except CustomBaseError:
        pass


def catch_exception_catches_all() -> None:
    try:
        raise DeepError()
    except Exception:
        pass


def catch_baseexception_catches_all() -> None:
    try:
        raise CustomBaseError()
    except BaseException:
        pass
