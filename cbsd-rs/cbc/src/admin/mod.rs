// Copyright (C) 2026  Clyso
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Admin commands: role management, user management, channel management,
//! and build queue status.

pub mod channels;
pub mod queue;
pub mod robots;
pub mod roles;
pub mod users;

use clap::{Args, Subcommand};

use crate::client::ClientOpts;
use crate::error::Error;

// ---------------------------------------------------------------------------
// CLI argument types
// ---------------------------------------------------------------------------

#[derive(Args)]
pub struct AdminArgs {
    #[command(subcommand)]
    command: AdminCommands,
}

#[derive(Subcommand)]
enum AdminCommands {
    /// Role management
    Roles(roles::RolesArgs),
    /// User management and role assignments
    Users(users::UsersArgs),
    /// Robot account management
    Robots(robots::RobotsArgs),
    /// Build queue status
    Queue,
    /// Channel and type management
    Channel(channels::ChannelAdminArgs),
    /// Set a user's default channel
    UserSetDefaultChannel(UserSetDefaultChannelArgs),
}

#[derive(Args)]
struct UserSetDefaultChannelArgs {
    /// User email
    email: String,
    /// Channel ID
    channel_id: i64,
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

pub async fn run(
    args: AdminArgs,
    config_path: Option<&std::path::Path>,
    opts: ClientOpts,
) -> Result<(), Error> {
    match args.command {
        AdminCommands::Roles(a) => roles::run(a, config_path, opts).await,
        AdminCommands::Users(a) => users::run(a, config_path, opts).await,
        AdminCommands::Robots(a) => robots::run(a, config_path, opts).await,
        AdminCommands::Queue => queue::run(config_path, opts).await,
        AdminCommands::Channel(a) => channels::run(a, config_path, opts).await,
        AdminCommands::UserSetDefaultChannel(a) => {
            channels::set_user_default_channel(config_path, opts, &a.email, a.channel_id).await
        }
    }
}
