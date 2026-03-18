import abc
from collections.abc import Callable, Coroutine
from typing import Any


class SecureArg(abc.ABC):
    @property
    @abc.abstractmethod
    def value(self) -> str:
        pass


MaybeSecure = str | SecureArg
CmdArgs = list[MaybeSecure]
AsyncRunCmdOutCallback = Callable[[str], Coroutine[Any, Any, None]]  # pyright: ignore[reportExplicitAny]
