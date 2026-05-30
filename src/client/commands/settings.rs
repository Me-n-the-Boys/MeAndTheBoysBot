mod temporary_channels;
mod reaction_roles;
mod role_limiter;
mod twitch_channel;
mod auto_bans;

use temporary_channels::temporary_channels;
use reaction_roles::reaction_roles;
use role_limiter::role_limiter;
use auto_bans::auto_bans;

use crate::client::commands::{Context, Error};
///Various commands for changing some settings.
#[poise::command(
    slash_command,
    subcommands(
        "temporary_channels",
        "reaction_roles",
        "role_limiter",
        "auto_bans",
    ),
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
    subcommand_required
)]
pub async fn settings(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}
