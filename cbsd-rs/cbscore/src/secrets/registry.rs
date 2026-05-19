// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Registry-secret-specific helpers — building the `--creds user:pass`
//! flag value expected by `podman push` / `podman pull` and `skopeo
//! copy --src-creds` / `--dest-creds`.
//!
//! Phase 3 lands the construction helper; Phase 5's container build
//! / push pipeline calls into it.

use std::borrow::Cow;

use cbscore_types::utils::secrets::RegistryCreds;

use crate::utils::subprocess::{CmdArg, SecureArg};

/// `user:password`-formatted secure arg for podman / skopeo `--creds`
/// flags. Redacts to `user:****`, preserving the username for
/// operator-visible diagnostics.
struct CredsArg {
    username: String,
    password: String,
}

impl SecureArg for CredsArg {
    fn plaintext(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}:{}", self.username, self.password))
    }
    fn redacted(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}:****", self.username))
    }
}

/// Build the [`CmdArg::Secure`] payload for a `--creds` flag from a
/// resolved registry credential.
///
/// Returns `None` if the entry is still in its [`RegistryCreds::Vault`]
/// shape; callers must call
/// [`super::SecretsMgr::resolve_vault_refs`] first.
///
/// # Examples
///
/// ```
/// use cbscore::secrets::registry::creds_arg;
/// use cbscore_types::utils::secrets::RegistryCreds;
///
/// let creds = RegistryCreds::Plain {
///     username: "deploy".into(),
///     password: "hunter2".into(),
///     address: "quay.io".into(),
/// };
/// let arg = creds_arg(&creds).expect("plain creds resolved");
/// // The Debug impl on CmdArg yields the redacted form.
/// assert_eq!(format!("{arg:?}"), "deploy:****");
/// ```
#[must_use]
pub fn creds_arg(creds: &RegistryCreds) -> Option<CmdArg> {
    let RegistryCreds::Plain {
        username, password, ..
    } = creds
    else {
        return None;
    };
    Some(CmdArg::Secure(Box::new(CredsArg {
        username: username.clone(),
        password: password.clone(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creds_arg_plain_redacts_password() {
        let creds = RegistryCreds::Plain {
            username: "deploy".into(),
            password: "hunter2".into(),
            address: "quay.io".into(),
        };
        let arg = creds_arg(&creds).expect("plain");
        let CmdArg::Secure(s) = &arg else {
            panic!("expected Secure variant");
        };
        assert_eq!(s.plaintext(), "deploy:hunter2");
        assert_eq!(s.redacted(), "deploy:****");
    }

    #[test]
    fn creds_arg_vault_returns_none() {
        let creds = RegistryCreds::Vault {
            key: "registry/quay".into(),
            username: "deploy".into(),
            password: "ignored".into(),
            address: "quay.io".into(),
        };
        assert!(creds_arg(&creds).is_none());
    }
}
