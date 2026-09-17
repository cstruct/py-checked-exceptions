from alias_helpers import exported_raises

local_raises = exported_raises


def calls_cross_module_alias() -> None:
    local_raises()
