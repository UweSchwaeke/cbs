# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (c) 2026 Clyso GmbH


# SHA type was moved from crt.crtlib.git_utils
import abc


class SecureArg(abc.ABC):
    @property
    @abc.abstractmethod
    def value(self) -> str:
        pass


SHA = str
MaybeSecure = str | SecureArg
CmdArgs = list[MaybeSecure]
