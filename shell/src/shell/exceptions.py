import errno
from typing import override


class ShellError(Exception):
    msg: str | None
    ec: int | None

    def __init__(self, msg: str | None = None, *, ec: int | None = None):
        super().__init__()
        self.msg = msg
        self.ec = ec

    @override
    def __str__(self) -> str:
        ec_name = (
            errno.errorcode[self.ec] if self.ec and self.ec in errno.errorcode else None
        )
        return (
            "Shell error"
            + (f" ({ec_name})" if ec_name else "")
            + (f": {self.msg}" if self.msg else "")
        )

    def with_maybe_msg(self, prefix: str) -> str:
        return prefix + f": {self.msg}" if self.msg else ""
