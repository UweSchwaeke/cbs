// Copyright (C) 2026  Clyso
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

pub(crate) mod admin;
pub(crate) mod auth;
pub(crate) mod builds;
pub(crate) mod channels;
pub(crate) mod components;
pub(crate) mod periodic;
pub(crate) mod permissions;
pub(crate) mod robots;
pub(crate) mod workers;

#[cfg(test)]
mod audit_identity_lint;

#[cfg(test)]
pub(crate) mod test_support;
