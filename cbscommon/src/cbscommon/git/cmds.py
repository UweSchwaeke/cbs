# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (c) 2026 Clyso GmbH
import errno
import logging
import re
import secrets
import shutil
import tempfile
from pathlib import Path
from typing import cast

import git

from cbscommon.process.cmds import async_run_cmd
from cbscommon.process.types import CmdArgs, MaybeSecure

from .exceptions import (
    GitAMApplyError,
    GitCherryPickConflictError,
    GitCherryPickError,
    GitConfigNotSetError,
    GitCreateHeadExistsError,
    GitEmptyPatchDiffError,
    GitError,
    GitFetchError,
    GitFetchHeadNotFoundError,
    GitHeadNotFoundError,
    GitIsTagError,
    GitMissingBranchError,
    GitMissingRemoteError,
    GitPatchDiffError,
    GitPushError,
)
from .types import SHA, PushInfoLine, PushInfoLineStatus

logger = logging.getLogger(__name__)


async def git_check_patches_diff(
    ceph_git_path: Path,
    upstream_ref: str | SHA,
    head_ref: str | SHA,
    *,
    limit: str | SHA | None = None,
) -> tuple[list[str], list[str]]:
    logger.debug(
        f"check ref '{head_ref}' against upstream '{upstream_ref}', limit '{limit}'"
    )

    cmd: CmdArgs = ["cherry", upstream_ref, head_ref]
    if limit:
        cmd.append(limit)

    try:
        res = await _run_git(cmd, path=ceph_git_path)
    except GitError as e:
        msg = (
            f"unable to check patch diff between '{upstream_ref}' and '{head_ref}': {e}"
        )
        logger.error(msg)
        raise GitPatchDiffError(msg=msg) from None

    if not res:
        logger.warning(f"empty diff between '{upstream_ref}' and '{head_ref}")
        raise GitEmptyPatchDiffError()

    patches_res = res.splitlines()
    patches_add: list[str] = []
    patches_drop: list[str] = []

    entry_re = re.compile(r"^([-+])\s+(.*)$")
    for entry in patches_res:
        m = re.match(entry_re, entry)
        if not m:
            logger.error(f"unexpected entry format: {entry}")
            continue

        action = cast(str, m.group(1))
        sha = cast(str, m.group(2))

        match action:
            case "+":
                patches_add.append(sha)
            case "-":
                patches_drop.append(sha)
            case _:
                logger.error(f"unexpected patch action '{action}' for sha '{sha}'")

    logger.debug(f"ref '{head_ref}' add {patches_add}")
    logger.debug(f"ref '{head_ref}' drop {patches_drop}")

    return (patches_add, patches_drop)


async def git_patches_in_interval(
    repo_path: Path, from_ref: SHA, to_ref: SHA
) -> list[tuple[SHA, str]]:
    logger.debug(f"get patch interval from '{from_ref}' to '{to_ref}'")

    cmd: CmdArgs = [
        "rev-list",
        "--ancestry-path",
        "--pretty=oneline",
        f"{from_ref}~1..{to_ref}",
    ]
    try:
        res = await _run_git(cmd, path=repo_path)
    except GitError as e:
        msg = f"unable to obtain patch interval: {e}"
        logger.error(msg)
        raise GitError(msg=msg) from None

    def _split(ln: str) -> tuple[str, str]:
        sha, title = ln.split(maxsplit=1)
        return (sha, title)

    return list(
        map(_split, [line.strip() for line in res.splitlines() if line.strip()])
    )


async def git_get_patch_sha_title(repo_path: Path, sha: SHA) -> tuple[str, str]:
    logger.debug(f"get patch sha and title for '{sha}'")

    cmd: CmdArgs = ["show", "--format=%H %s", "--no-patch", sha]
    try:
        res = await _run_git(cmd, path=repo_path)
    except GitError as e:
        msg = f"unable to obtain patch sha and title for '{sha}': {e}"
        logger.error(msg)
        raise GitError(msg=msg) from None

    logger.debug(res)
    lst = [line.strip() for line in res.splitlines() if line.strip()]
    if len(lst) > 1:
        raise GitError(msg=f"unexpected multiple lines for patch '{sha}'")
    logger.debug(lst)
    patch_sha, patch_title = next(iter(lst)).split(maxsplit=1)
    return (patch_sha, patch_title)


async def git_status(repo_path: Path) -> list[tuple[str, str]]:
    cmd: CmdArgs = ["status", "--porcelain"]
    try:
        res = await _run_git(cmd, path=repo_path)
    except GitError:
        msg = f"unable to run git status on '{repo_path}'"
        logger.error(msg)
        raise GitError(msg=msg) from None

    status_lst: list[tuple[str, str]] = []
    for entry in res.splitlines():
        status, file = entry.split()
        status_lst.append((status, file))

    return status_lst


async def git_am_apply(repo_path: Path, patch_path: Path) -> None:
    cmd: CmdArgs = ["am", str(patch_path)]
    try:
        _ = await _run_git(cmd, path=repo_path)
    except GitError:
        msg = f"unable to apply patch '{patch_path}'"
        logger.error(msg)
        raise GitAMApplyError(msg=msg) from None


async def git_am_abort(repo_path: Path) -> None:
    cmd: CmdArgs = ["am", "--abort"]
    try:
        _ = await _run_git(cmd, path=repo_path)
    except GitError:
        logger.error("found error aborting git-am")


async def git_cleanup_repo(repo_path: Path) -> None:
    cmd: CmdArgs = [
        "submodulede",
        "init",
        "--all",
        "-f",
    ]
    try:
        _ = await _run_git(cmd, path=repo_path)
        cmd = ["clean", "-ffdx"]
        _ = await _run_git(cmd, path=repo_path)
        cmd = ["reset", "--hard"]
        _ = await _run_git(cmd, path=repo_path)
    except GitError as e:
        msg = f"unable to clean up repository '{repo_path}': {e}"
        logger.error(msg)
        raise GitError(msg=msg) from None


async def git_prepare_remote(
    repo_path: Path, remote_uri: str, remote_name: str, token: str
) -> None:
    logger.info(f"prepare remote '{remote_name}' uri '{remote_uri}'")
    remote_url = f"https://crt:{token}@{remote_uri}"
    cmd: CmdArgs = ["remote", "add", remote_name, remote_url]

    try:
        _ = await _run_git(cmd, path=repo_path)
    except GitError as e:
        # git remote add returns ESRCH (3), if remote_name already exists.
        if e.ec != errno.ESRCH:
            msg = f"error occured during git remote add: {e}"
            logger.error(msg)
            raise GitError(msg=msg, ec=e.ec) from e
    else:
        logger.debug(f"created remote '{remote_name}' url '{remote_url}'")

    logger.info(f"update remote '{remote_name}'")
    try:
        cmd = ["remote", "update", remote_name]
        _ = await _run_git(cmd, path=repo_path)
    except GitError:
        logger.error(f"unable to update remote '{remote_name}'")
        raise GitError(msg=f"unable to update remote '{remote_name}'") from None


async def git_remote_exists(repo_path: Path, remote_name: str) -> bool:
    logger.info(f"does remote '{remote_name}' exist.")
    cmd: CmdArgs = ["remote"]
    res = await _run_git(cmd, path=repo_path)
    return remote_name in res.splitlines()


def _get_remote_ref_name(
    remote_name: str, remote_ref: str, *, ref_name: str | None = None
) -> tuple[str, str] | None:
    ref_re = re.compile(rf"^{remote_name}/(.*)$")
    if m := re.match(ref_re, remote_ref):
        name = cast(str, m.group(1))
        if ref_name and ref_name != name:
            return None

        return (remote_name, m.group(1))
    return None


async def git_remote_ref_exists(
    repo_path: Path, ref_name: str, remote_name: str
) -> bool:
    if not await git_remote_exists(repo_path, remote_name):
        logger.error(f"remote '{remote_name}' not found")
        raise GitMissingRemoteError(remote_name) from None

    cmd: CmdArgs = ["branch", "-r", "--list", f"{remote_name}/*"]
    res = await _run_git(cmd, path=repo_path)
    for ref in res.splitlines():
        ref = ref.strip()
        if _get_remote_ref_name(remote_name, ref, ref_name=ref_name):
            return True

    return False


async def _git_pull_ref(
    repo_path: Path, from_ref: str, to_ref: str, remote_name: str
) -> bool:
    cmd: CmdArgs = ["branch", "--show-current"]
    active_branch = await _run_git(cmd, path=repo_path)

    if active_branch != to_ref:
        return False

    if not await git_remote_ref_exists(repo_path, from_ref, remote_name):
        logger.warning(f"ref '{from_ref}' not found in remote '{remote_name}'")
        return False

    cmd = ["pull", remote_name, f"{from_ref}:{to_ref}"]
    try:
        _ = await _run_git(cmd, path=repo_path)
    except GitError as e:
        logger.error(
            f"unable to pull from '{remote_name}' ref '{from_ref}' to '{to_ref}'"
        )
        logger.error(e.msg)
        raise GitFetchError(remote_name, from_ref, to_ref) from None

    return True


async def _is_tag(repo_path: Path, tag_name: str) -> bool:
    cmd: CmdArgs = ["tag", "--list", tag_name]
    res = await _run_git(cmd, path=repo_path)
    return bool(res.strip())


async def git_reset_head(repo_path: Path, new_head: str) -> None:
    """Reset current checked out head to `new_head`."""
    if not await git_local_head_exists(repo_path, new_head):
        msg = f"unexpected missing local head '{new_head}'"
        logger.error(msg)
        raise GitError(msg)

    cmd: CmdArgs = ["switch", new_head]
    _ = await _run_git(cmd, path=repo_path)
    cmd = ["reset", "--hard", new_head]
    _ = await _run_git(cmd, path=repo_path)


async def git_branch_from(repo_path: Path, src_ref: str, dst_branch: str) -> None:
    """Create a new branch `dst_branch` from `src_ref`."""
    logger.debug(f"create branch '{dst_branch}' from '{src_ref}'")
    cmd: CmdArgs = ["branch", "--show-current"]
    active_branch = await _run_git(cmd, path=repo_path)
    logger.debug(f"repo active branch: {active_branch}")

    if await git_local_head_exists(repo_path, dst_branch):
        msg = f"unable to create branch '{dst_branch}', already exists"
        logger.error(msg)
        raise GitCreateHeadExistsError(dst_branch)

    if await _is_tag(repo_path, src_ref):
        logger.debug(f"source ref '{src_ref}' is a tag")
        src_ref = f"refs/tags/{src_ref}"

    try:
        cmd = ["branch", dst_branch, src_ref]
        _ = await _run_git(cmd)
    except GitError as e:
        msg = f"unable to create branch '{dst_branch}' from '{src_ref}': {e}"
        logger.error(msg)
        logger.error(e.msg)
        raise GitError(msg) from None


async def git_fetch_ref(
    repo_path: Path, from_ref: str, to_ref: str, remote_name: str
) -> bool:
    """
    Fetch a reference from a remote into a given branch.

    If the target branch is already checked out, perform a `git pull` instead.
    If the source ref is a tag, do not fetch.

    Will raise if `from_ref` is a tag, or if it doesn't exist in the specified remote.
    Might raise in other `git fetch` error conditions.
    """
    logger.debug(f"fetch from '{remote_name}' ref '{from_ref}' to '{to_ref}'")
    cmd: CmdArgs = ["branch", "--show-current"]
    active_branch = await _run_git(cmd, path=repo_path)
    logger.debug(f"repo active branch: {active_branch}")

    if active_branch == to_ref:
        logger.warning(f"checked out branch is '{to_ref}', pull instead.")
        return await _git_pull_ref(repo_path, from_ref, to_ref, remote_name)

    # check whether 'from_ref' is a tag
    if await _is_tag(repo_path, from_ref):
        logger.warning(f"can't fetch tag '{from_ref}' from remote '{remote_name}'")
        raise GitIsTagError(from_ref)

    # check whether 'from_ref' is a remote head
    if not await git_remote_ref_exists(repo_path, from_ref, remote_name):
        logger.warning(f"unable to find ref '{from_ref}' in remote '{remote_name}'")
        raise GitFetchHeadNotFoundError(remote_name, from_ref)

    if not await git_remote_exists(repo_path, remote_name):
        msg = f"unexpected error obtaining remote '{remote_name}'"
        logger.error(msg)
        raise GitError(msg) from None

    try:
        cmd = ["fetch", remote_name, f"{from_ref}:{to_ref}"]
        _ = await _run_git(cmd, path=repo_path)
    except git.CommandError as e:
        logger.error(
            f"unable to fetch from remote '{remote_name}' "
            + f"ref '{from_ref}' to '{to_ref}'"
        )
        logger.error(e.stderr)
        raise GitFetchError(remote_name, from_ref, to_ref) from None

    return True


async def git_checkout_ref(
    repo_path: Path,
    ref: str,
    *,
    to_branch: str | None = None,
    remote_name: str | None = None,
    update_from_remote: bool = False,
    fetch_if_not_exists: bool = False,
) -> None:
    """
    Check out a reference, possibly to a new branch.

    If `ref` exists in the repository, checks out said head. Otherwise, either raise
    `GitMissingBranchError`, or attempt to fetch the branch from `remote_name` if
    `remote_name` is `True` and `fetch_if_not_exists` is defined.

    If `to_branch` is defined, either checks out the provided `ref` to the specified
    branch, or attempts to fetch it from remote `remote_name` (if defined).

    If `update_from_remote` is `True`, always attempt to fetch the latest updates in
    the remote branch to the target branch. The target branch can be `ref` or
    `to_branch` depending on whether the latter is defined. If `remote_name` is not
    specified, `update_from_remote` has no effect.
    """

    async def _update_from_remote(head: str, remote: str) -> None:
        logger.debug(f"update '{head}' from remote if it exists")
        try:
            res = await git_fetch_ref(repo_path, head, head, remote)
        except Exception as e:
            logger.error(f"unable to update '{head}' from remote '{remote}: {e}")
            return

        if not res:
            logger.info(f"whatever to update for '{head}' from remote '{remote}'")
        pass

    async def _checkout_head(head: str, *, target_branch: str | None = None) -> None:
        """
        Checkout a given head.

        If `target_branch` is specified, checkout the provided head to a new branch.
        """
        logger.debug(f"checkout head '{head}' to '{target_branch}'")

        if (
            target_branch
            and head != target_branch
            and await git_local_head_exists(repo_path, target_branch)
        ):
            raise GitCreateHeadExistsError(target_branch)
        # should we update from remote first?
        if update_from_remote and remote_name:
            await _update_from_remote(head, remote_name)

        await git_reset_head(repo_path, head)

    target_branch = to_branch if to_branch else ref

    # check if 'ref' exists as a branch locally
    if await git_local_head_exists(repo_path, target_branch):
        await _checkout_head(target_branch, target_branch=target_branch)
        return

    if not fetch_if_not_exists:
        logger.debug(f"not fetching '{ref}' as specified")
        raise GitMissingBranchError(ref)

    if not remote_name:
        msg = f"unable to fetch ref '{ref}', no remote given"
        logger.error(msg)
        raise GitError(msg) from None

    # local head does not exist, fetch it.
    is_tag = False
    try:
        _ = await git_fetch_ref(repo_path, ref, target_branch, remote_name)
    except GitIsTagError:
        logger.debug(f"ref '{ref}' is a tag, must checkout instead.")
        is_tag = True
    except GitFetchHeadNotFoundError as e:
        logger.error(f"ref '{ref}' not found in remote.")
        raise e from None
    except GitError as e:
        logger.error(f"error occurred fetching ref '{ref}': {e}")
        raise e from None

    if is_tag:
        try:
            cmd: CmdArgs = ["checkout", ref, "-b", target_branch]
            _ = await _run_git(cmd, path=repo_path)
        except GitError as e:
            msg = f"unable to checkout ref '{ref}' to '{target_branch}': {e}"
            logger.error(msg)
            logger.error(e.msg)
            raise GitError(msg) from None
        return

    # propagate exceptions
    await git_reset_head(repo_path, target_branch)


async def git_branch_delete(repo_path: Path, branch: str) -> None:
    """Delete a local branch."""
    cmd: CmdArgs = ["branch", "--show-current"]
    active_branch = await _run_git(cmd, path=repo_path)

    if active_branch == branch:
        await git_cleanup_repo(repo_path)
        cmd = ["switch", "main"]
        _ = await _run_git(cmd, path=repo_path)

    cmd = ["branch", "-D", branch]
    _ = await _run_git(cmd, path=repo_path)


async def git_push(
    repo_path: Path,
    ref: str,
    remote_name: str,
    *,
    ref_to: str | None = None,
) -> tuple[bool, list[str], list[str]]:
    """Pushes either a local head of branch or a local tag to the remote."""
    dst_ref = ref_to if ref_to else ref

    if await _is_tag(repo_path, ref):
        ref = f"refs/tags/{ref}"
        dst_ref = f"refs/tags/{dst_ref}"
    elif not await git_local_head_exists(repo_path, ref):
        # ref is neither a local branch nor tag
        logger.error(f"unable to find ref '{ref}' to push")
        raise GitHeadNotFoundError(ref)

    if not await git_remote_exists(repo_path, remote_name):
        logger.error(f"unable to find remote '{remote_name}'")
        raise GitMissingRemoteError(remote_name) from None

    try:
        cmd: CmdArgs = ["push", remote_name, f"{ref}:{dst_ref}", "--porcelain"]
        info = await _run_git(cmd, path=repo_path)
        # skip first line because it is To remote url
        # and skip last line because it is Done
        info = info.splitlines()[1:-1]
        info = [PushInfoLine(line) for line in info]
    except GitError as e:
        msg = f"unable to push '{ref}' to '{dst_ref}': {e}"
        logger.error(msg)
        logger.error(e.msg)
        raise GitPushError(ref, dst_ref, remote_name) from None

    updated: list[str] = []
    rejected: list[str] = []
    failed = len(info) == 0

    for entry in info:
        logger.debug(f"entry '{entry.remote_ref_name}' flags '{entry.flag}'")
        if entry.status == PushInfoLineStatus.REJECTED:
            logger.debug(f"rejected head: {entry.remote_ref_name}")
            rejected.append(entry.remote_ref_name)
        elif entry.status == PushInfoLineStatus.UPDATED:
            logger.debug(f"updated head: {entry.remote_ref_name}")

    return (failed, updated, rejected)


async def git_tag(
    repo_path: Path,
    tag_name: str,
    ref: str,
    *,
    msg: str | None = None,
    push_to: str | None = None,
) -> None:
    logger.debug(f"create tag '{tag_name}' at ref '{ref}'")
    cmd: CmdArgs = ["tag", tag_name, ref]
    if msg:
        cmd.append("-m")
        cmd.append("msg")
    try:
        _ = await _run_git(cmd, path=repo_path)
    except GitError as e:
        msg = f"unable to create tag '{tag_name}' at ref '{ref}': {e}"
        logger.error(msg)
        raise GitError(msg=msg) from None

    if push_to:
        logger.debug(f"push tag '{tag_name}' to remote '{push_to}'")
        cmd = ["push", push_to, "tag", tag_name]
        try:
            _ = await _run_git(cmd, path=repo_path)
        except GitError as e:
            msg = f"unable to push tag '{tag_name}' to remote '{push_to}': {e}"
            logger.error(msg)
            raise GitError(msg=msg) from None


def git_patch_id(repo_path: Path, sha: SHA) -> str:
    repo = git.Repo(repo_path)
    with tempfile.TemporaryFile() as tmp:
        try:
            repo.git.show(sha, output_stream=tmp)  # pyright: ignore[reportAny]
        except git.CommandError:
            msg = f"unable to find patch sha '{sha}'"
            logger.error(msg)
            raise GitError(msg=msg) from None

        _ = tmp.seek(0)
        res = cast(str, repo.git.patch_id(["--stable"], istream=tmp))  # pyright: ignore[reportAny]

    if not res:
        raise GitError(msg="unable to obtain git patch id")
    return res.split()[0]


async def git_revparse(repo_path: Path, commitish: SHA | str) -> str:
    cmd: CmdArgs = ["rev-parse", commitish]
    try:
        res = await _run_git(cmd, path=repo_path)
    except GitError as e:
        msg = f"unable to obtain revision for '{commitish}': {e}"
        logger.error(msg)
        raise GitError(msg=msg) from None
    return res


async def git_format_patch(
    repo_path: Path, rev: SHA, *, base_rev: SHA | None = None
) -> str:
    cmd: CmdArgs = ["format-patch", "--stdout"]

    if not base_rev:
        cmd.append("-1")

    rev_str = f"{base_rev}..{rev}" if base_rev else rev
    cmd.append(rev_str)

    try:
        res = await _run_git(cmd, path=repo_path)
    except GitError as e:
        msg = f"unable to obtain format patch for '{rev_str}': {e}"
        logger.error(msg)
        raise GitError(msg=msg) from None

    return res


async def git_tag_exists_in_remote(
    repo_path: Path, remote_name: str, tag_name: str
) -> bool:
    cmd: CmdArgs = ["ls-remote", "--tags", remote_name, ""]
    try:
        raw_tag = await _run_git(cmd, path=repo_path)
        return bool(raw_tag.strip())
    except GitError as e:
        msg = f"unable to execute git ls-remote --tags {remote_name} refs/tags/{tag_name}: {e}"
        logger.error(msg)
        raise GitError(msg) from None


async def git_remote_ref_names(repo_path: Path, remote_name: str) -> list[str]:
    cmd: CmdArgs = ["branch", "-r", "--list", remote_name, "--format", "%(refname)"]
    try:
        res = await _run_git(cmd, path=repo_path)
        lines = res.splitlines()
        return [line.removeprefix("refs/remotes/") for line in lines]
    except GitError as e:
        msg = f"unable to list remote names of remote '{remote_name}': '{e}'"
        logger.error(msg)
        raise GitError(msg) from None


async def git_checkout_from_local_ref(
    repo_path: Path, from_ref: str, branch_name: str
) -> None:
    logger.debug(f"checkout ref '{from_ref}' to '{branch_name}'")
    if await git_local_head_exists(repo_path, branch_name):
        logger.debug(f"branch '{branch_name}' already exists, simply checkout")
        await git_reset_head(repo_path, branch_name)
        return

    cmd: CmdArgs = ["branch", "--list", branch_name]
    try:
        branch = await _run_git(cmd, path=repo_path)
        assert not branch.strip()
    except GitError as e:
        msg = f"unable to list local branches matching '{branch_name}': {e}"
        logger.error(msg)
        raise GitError(msg) from None

    cmd = ["branch", branch_name, from_ref]
    try:
        _ = await _run_git(cmd, path=repo_path)
    except GitError:
        msg = f"unable to create new head '{branch_name}' " + f"from '{from_ref}'"
        logger.exception(msg)
        raise GitError(msg=msg) from None

    await git_reset_head(repo_path, branch_name)

    try:
        await git_cleanup_repo(repo_path)
        await git_update_submodules(repo_path)
    except GitError as e:
        msg = f"unable to clean up repo state after checkout: {e}"
        logger.error(msg)
        raise GitError(msg=msg) from None


async def git_update_submodules(repo_path: Path) -> None:
    logger.debug("update submodules")
    cmd: CmdArgs = ["submodule", "update", "--init", "--recursive"]
    try:
        _ = await _run_git(cmd, path=repo_path)
    except GitError as e:
        msg = f"unable to update repository's submodules: {e}"
        logger.error(msg)
        raise GitError(msg=msg) from None


async def git_local_head_exists(repo_path: Path, name: str) -> bool:
    cmd: CmdArgs = ["branch", "--list", name]
    res = await _run_git(cmd, path=repo_path)
    return bool(res.strip())


async def _run_git(args: CmdArgs, *, path: Path | None = None) -> str:
    """
    Run a git command within the repository.

    If `path` is provided, run the command in `path`. Otherwise, run in the current
    directory.
    """
    cmd: CmdArgs = ["git"]
    if path is not None:
        cmd.extend(["-C", path.resolve().as_posix()])

    cmd.extend(args)
    logger.debug(f"run {cmd}")
    try:
        rc, stdout, stderr = await async_run_cmd(cmd)
    except Exception as e:
        msg = f"unexpected error running command: {e}"
        logger.error(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e

    if rc != 0:
        logger.error(f"unable to obtain result from git '{args}': {stderr}")
        raise GitError(stderr, ec=rc)

    return stdout


async def get_git_user() -> tuple[str, str]:
    """Obtain the current repository's git user and email, returned as a tuple."""

    async def _run_git_config_for(v: str) -> str:
        val = await _run_git(["config", v])
        if len(val) == 0:
            logger.error(f"'{v}' not set in git config")
            raise GitConfigNotSetError(v)

        return val.strip()

    user_name = await _run_git_config_for("user.name")
    user_email = await _run_git_config_for("user.email")
    assert len(user_name) > 0 and len(user_email) > 0
    return (user_name, user_email)


async def get_git_repo_root() -> Path:
    """Obtain the root of the current git repository."""
    val = await _run_git(["rev-parse", "--show-toplevel"])
    if len(val) == 0:
        logger.error("unable to obtain toplevel git directory path")
        raise GitError("top-level git directory not found", ec=errno.ENOENT)

    return Path(val.strip())


async def _clone(repo: MaybeSecure, dest_path: Path) -> None:
    """Clones a repository from `repo` to `dest_path`."""
    try:
        _ = await _run_git(
            [
                "clone",
                "--mirror",
                "--quiet",
                repo,
                dest_path.resolve().as_posix(),
            ]
        )
    except GitError as e:
        msg = f"unable to clone '{repo}' to '{dest_path}': {e}"
        logger.error(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e


async def _update(repo: MaybeSecure, repo_path: Path) -> None:
    """Update a git repository in `repo_path` from its upstream at `repo`."""
    try:
        _ = await _run_git(["remote", "set-url", "origin", repo], path=repo_path)
        _ = await _run_git(["remote", "update"], path=repo_path)
    except GitError as e:
        msg = f"unable to update '{repo_path}': {e}'"
        logger.error(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e


async def git_checkout(repo_path: Path, ref: str, worktrees_base_path: Path) -> Path:
    """
    Checkout a reference pointed to by `ref`, in repository `repo_path`.

    Uses git worktrees to checkout the reference into a new worktree under
    `worktrees_base_path`.

    Returns the path to the checked out worktree.
    """
    try:
        worktrees_base_path.mkdir(parents=True, exist_ok=True)
    except Exception as e:
        msg = f"unable to create worktrees base path at '{worktrees_base_path}': {e}"
        logger.error(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e

    worktree_rnd_suffix = secrets.token_hex(5)
    worktree_name = ref.replace("/", "--") + f".{worktree_rnd_suffix}"
    worktree_path = worktrees_base_path / worktree_name
    logger.info(f"checkout ref '{ref}' into worktree at '{worktree_path}'")

    try:
        _ = await _run_git(
            [
                "worktree",
                "add",
                "--track",
                "-b",
                worktree_name,
                "--quiet",
                worktree_path.resolve().as_posix(),
                ref,
            ],
            path=repo_path,
        )
    except GitError as e:
        msg = f"unable to checkout ref '{ref}' in repository '{repo_path}': {e}"
        logger.error(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e

    return worktree_path


async def git_remove_worktree(repo_path: Path, worktree_path: Path) -> None:
    """Remove a git worktree at `worktree_path` from repository `repo_path`."""
    logger.info(f"removing worktree at '{worktree_path}' from repository '{repo_path}'")
    try:
        _ = await _run_git(
            ["worktree", "remove", "--force", worktree_path.resolve().as_posix()],
            path=repo_path,
        )
    except GitError as e:
        msg = f"unable to remove worktree at '{worktree_path}': {e}"
        logger.error(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e


async def git_clone(repo: MaybeSecure, base_path: Path, repo_name: str) -> Path:
    """
    Clone a mirror git repository if it doesn't exist; update otherwise.

    Clone a git repository from `repo` to a directory `repo_name` in `base_path`. If a
    git repository exists at the destination, ensure the repository is updated instead.

    Returns the path to the repository.
    """
    logger.info(f"cloning '{repo}' to new destination '{base_path}' name '{repo_name}'")
    if not base_path.exists():
        logger.warning(f"base path at '{base_path}' does not exist -- creating")
        try:
            base_path.mkdir(parents=True, exist_ok=True)
        except Exception as e:
            msg = f"unable to create base path at '{base_path}': {e}"
            logger.error(msg)
            raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e

    dest_path = base_path / f"{repo_name}.git"

    if dest_path.exists():
        if not dest_path.is_dir() or not dest_path.joinpath("HEAD").exists():
            logger.warning(
                f"destination path at '{dest_path}' exists, "
                + "but is not a valid git repository -- nuke it!"
            )
            try:
                shutil.rmtree(dest_path)
            except Exception as e:
                msg = f"unable to remove invalid git repository at '{dest_path}': {e}"
                logger.error(msg)
                raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e

        # propagate exception to caller
        await _update(repo, dest_path)
        return dest_path

    # propagate exception to caller
    await _clone(repo, dest_path)
    return dest_path


async def git_apply(repo_path: Path, patch_path: Path) -> None:
    """Apply a patch onto the repository specified by `repo_path`."""
    try:
        _ = await _run_git(["apply", patch_path.resolve().as_posix()], path=repo_path)
    except GitError as e:
        msg = f"error applying patch '{patch_path}' to '{repo_path}': {e}"
        logger.exception(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e
    pass


async def git_get_sha1(repo_path: Path) -> str:
    """For the repository in `repo_path`, obtain its currently checked out SHA1."""
    val = await _run_git(["rev-parse", "HEAD"], path=repo_path)
    if len(val) == 0:
        msg = f"unable to obtain current SHA1 on repository '{repo_path}"
        logger.error(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE)

    return val.strip()


# unused functions


async def _git_cherry_pick(repo_path: Path, sha: SHA) -> None:  # pyright: ignore[reportUnusedFunction, reportRedeclaration]
    repo = git.Repo(repo_path)

    try:
        repo.git.cherry_pick(["-x", "-s", sha])  # pyright: ignore[reportAny]
    except git.CommandError as e:
        msg = f"unable to cherry-pick patch sha '{sha}'"
        logger.error(msg)

        status_files = await git_status(repo_path)
        conflicts: list[str] = [f for s, f in status_files if s == "UU"]

        if conflicts:
            raise GitCherryPickConflictError(sha, conflicts) from None

        logger.error(e.stderr)
        raise GitCherryPickError(msg=msg) from None


def _git_abort_cherry_pick(repo_path: Path) -> None:  # pyright: ignore[reportUnusedFunction]
    repo = git.Repo(repo_path)

    try:
        _ = repo.git.cherry_pick("--abort")  # pyright: ignore[reportAny]
    except git.CommandError as e:
        logger.error(f"found error aborting cherry-pick: {e.stderr}")


async def _get_git_modified_paths(  # pyright: ignore[reportUnusedFunction]
    base_sha: str,
    ref: str,
    *,
    in_repo_path: str | None = None,
    repo_path: Path | None = None,
) -> tuple[list[Path], list[Path]]:
    """
    Obtain all modifications since `ref` on the repository.

    If `path` is specified, perform the action within the context of `path`. Otherwise,
    on the git repository existing in current directory.
    """
    try:
        cmd: CmdArgs = [
            "diff-tree",
            "--diff-filter=ACDMR",
            "--ignore-all-space",
            "--no-commit-id",
            "--name-status",
            "-r",
            base_sha,
            ref,
        ]

        if in_repo_path:
            cmd.extend(["--", in_repo_path])

        val = await _run_git(cmd, path=repo_path)
    except GitError as e:
        logger.error(f"error: unable to obtain latest patch: {e}")
        raise GitError(
            f"unable to obtain patches between {base_sha} and {ref}",
            ec=errno.ENOTRECOVERABLE,
        ) from e

    if len(val) == 0:
        logger.debug(f"no relevant patches found between {base_sha} and {ref}")
        return [], []

    descs_deleted: list[Path] = []
    descs_modified: list[Path] = []

    lines = val.splitlines()
    regex = (
        re.compile(rf"^\s*([ACDMR])\s+({repo_path}.*)\s*$")
        if repo_path is not None
        else re.compile(r"\s*([ACDMR])\s+([^\s]+)\s*$")
    )
    for line in lines:
        m = re.match(regex, line)
        if m is None:
            logger.debug(f"'{line}' does not match")
            continue

        action = m.group(1)
        target = m.group(2)
        logger.debug(f"action: {action}, target: {target}")

        match action:
            case "D":
                descs_deleted.append(Path(target))
            case "A" | "C" | "M" | "R":
                descs_modified.append(Path(target))
            case _:
                logger.error(
                    f"unexpected action '{action}' on '{target}', line: '{line}'"
                )
                raise GitError(
                    f"unexpected action '{action}' on '{target}'",
                    ec=errno.ENOTRECOVERABLE,
                )

    return descs_modified, descs_deleted


async def _git_pull(  # pyright: ignore[reportUnusedFunction]
    remote: MaybeSecure,
    *,
    from_branch: str | None = None,
    to_branch: str | None = None,
    repo_path: Path | None = None,
) -> None:
    """Pull commits from `remote`."""
    logger.debug(f"Pull from '{remote}' (from: {from_branch}, to: {to_branch})")
    try:
        cmd: CmdArgs = ["pull", remote]
        branches: str | None = None
        if from_branch:
            branches = from_branch
            if to_branch:
                branches = f"{branches}:{to_branch}"
        if branches:
            cmd.append(branches)
        _ = await _run_git(cmd, path=repo_path)
    except GitError as e:
        msg = f"unable to pull from '{remote}': {e}"
        logger.exception(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e


async def _git_cherry_pick(  # pyright: ignore[reportUnusedFunction]
    sha: str, *, sha_end: str | None = None, repo_path: Path | None = None
) -> None:
    """
    Cherry-picks a given SHA to the currently checked out branch.

    If `sha_end` is provided, will cherry-pick the patches `[sha~1, sha_end]`.
    If `repo_path` is provided, run the command in said repository; otherwise, run
    in the current directory.
    """
    commit_to_pick = sha if not sha_end else f"{sha}~1..{sha_end}"
    logger.debug(f"cherry-pick commit '{commit_to_pick}'")
    try:
        _ = await _run_git(["cherry-pick", "-x", commit_to_pick], path=repo_path)
    except GitError as e:
        msg = f"unable to cherry-pick '{commit_to_pick}': {e}"
        logger.exception(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e


async def _git_get_current_branch(repo_path: Path) -> str:  # pyright: ignore[reportUnusedFunction]
    """Obtain the name of the currently checked out branch."""
    val = await _run_git(["rev-parse", "--abbrev-ref", "HEAD"], path=repo_path)
    if not val:
        msg = (
            "unable to obtain current checked out branch's "
            + f"name on repository '{repo_path}'"
        )
        logger.error(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE)

    return val.strip()


async def _git_fetch(  # pyright: ignore[reportUnusedFunction]
    remote: str, from_ref: str, to_branch: str, *, repo_path: Path | None = None
) -> None:
    """
    Fetch a reference from a remote to a new branch.

    Fetches the reference pointed to by `from_ref` from remote `remote` to a new branch
    `to_branch`. If `repo_path` is specified, run the command in said path; otherwise,
    run in current directory.
    """
    logger.debug(f"fetch from '{remote}', source: {from_ref}, dest: {to_branch}")
    try:
        _ = await _run_git(["fetch", remote, f"{from_ref}:{to_branch}"], path=repo_path)
    except GitError as e:
        msg = f"unable to fetch '{from_ref}' from '{remote}' to '{to_branch}': {e}"
        logger.exception(msg)
        raise GitError(msg, ec=errno.ENOTRECOVERABLE) from e
