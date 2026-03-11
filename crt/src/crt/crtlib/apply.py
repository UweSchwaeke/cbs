# crt - apply manifest
# Copyright (C) 2025  Clyso GmbH
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.


import datetime
import logging
from datetime import datetime as dt
from pathlib import Path
from typing import override

from shell.git import (
    SHA,
    GitAMApplyError,
    GitError,
    git_am_abort,
    git_am_apply,
    git_branch_delete,
    git_checkout_submodule_ref,
    git_cleanup_repo,
    git_prepare_remote,
    git_prepare_repo,
)

from crt.crtlib.models.common import ManifestPatchEntry
from crt.crtlib.models.manifest import ReleaseManifest
from crt.crtlib.patch import PatchExistsError

logger = logging.getLogger(__name__)


class ApplyError(Exception):
    msg: str | None

    def __init__(self, *, msg: str | None = None) -> None:
        super().__init__()
        self.msg = msg

    @override
    def __str__(self) -> str:
        return "error applying manifest" + (f": {self.msg}" if self.msg else "")


class ApplyConflictError(ApplyError):
    sha: SHA
    conflict_files: list[str]

    def __init__(self, sha: SHA, files: list[str]) -> None:
        super().__init__(msg=f"{len(files)} file conflicts on sha '{sha}'")
        self.sha = sha
        self.conflict_files = files


def apply_manifest(
    manifest: ReleaseManifest,
    ceph_repo_path: Path,
    patches_repo_path: Path,
    target_branch: str,
    token: str,
    *,
    no_cleanup: bool = False,
) -> tuple[bool, list[ManifestPatchEntry], list[ManifestPatchEntry]]:
    logger.info(f"apply manifest '{manifest.release_uuid}' to branch '{target_branch}'")

    def _cleanup(*, abort_apply: bool = False) -> None:
        logger.debug(f"cleanup state, branch '{target_branch}'")
        if abort_apply:
            git_am_abort(ceph_repo_path)

        git_cleanup_repo(ceph_repo_path)

        git_branch_delete(ceph_repo_path, target_branch)

    def _apply_patches(
        patches: list[ManifestPatchEntry],
    ) -> tuple[list[ManifestPatchEntry], list[ManifestPatchEntry]]:
        logger.debug(f"apply {len(patches)} patches")

        skipped: list[ManifestPatchEntry] = []
        added: list[ManifestPatchEntry] = []

        for entry in patches:
            logger.debug(f"apply patch uuid '{entry.entry_uuid}'")

            patch_path = (
                patches_repo_path.joinpath("ceph")
                .joinpath("patches")
                .joinpath(f"{entry.entry_uuid}.patch")
            )
            if not patch_path.exists():
                raise ApplyError(msg=f"missing patch uuid '{entry.entry_uuid}'")

            try:
                git_am_apply(ceph_repo_path, patch_path)
            except Exception as e:
                raise e from None

            added.append(entry)

        return (added, skipped)

    try:
        git_prepare_repo(ceph_repo_path)
        repo_name = f"{manifest.base_ref_org}/{manifest.base_ref_repo}"
        git_prepare_remote(ceph_repo_path, f"github.com/{repo_name}", repo_name, token)
    except GitError as e:
        logger.error(e)
        raise ApplyError(msg=e.msg) from None

    try:
        _branch = git_checkout_submodule_ref(
            ceph_repo_path, manifest.base_ref, target_branch
        )
    except GitError as e:
        msg = f"unable to apply manifest patchsets: {e}"
        logger.error(msg)
        if not no_cleanup:
            _cleanup()

        raise ApplyError(msg=msg) from e

    abort_am = True
    try:
        added, skipped = _apply_patches(manifest.patches)
        logger.debug("successfully applied patches to manifest")
    except (GitAMApplyError, Exception) as e:
        msg = f"failed applying manifest patches: {e}"
        logger.error(msg)
        raise ApplyError(msg=msg) from None
    else:
        abort_am = False
        logger.debug("git-am successful, don't abort on cleanup")
    finally:
        if not no_cleanup:
            _cleanup(abort_apply=abort_am)

    return (len(added) > 0, added, skipped)


def patches_apply_to_manifest(
    orig_manifest: ReleaseManifest,
    patch: ManifestPatchEntry,
    ceph_repo_path: Path,
    patches_repo_path: Path,
    token: str,
) -> tuple[bool, list[ManifestPatchEntry], list[ManifestPatchEntry]]:
    manifest = orig_manifest.model_copy(deep=True)
    if not manifest.add_patches(patch):
        raise PatchExistsError(msg=f"uuid '{patch.entry_uuid}'")

    seq = dt.now(datetime.UTC).strftime("%Y%m%dT%H%M%S")
    target_branch = f"{manifest.name}-{manifest.release_git_uid}-{seq}"

    return apply_manifest(
        manifest,
        ceph_repo_path,
        patches_repo_path,
        target_branch,
        token,
        no_cleanup=False,
    )
