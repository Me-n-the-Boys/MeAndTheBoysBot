mod temporary_channels;
use temporary_channels::temporary_channels;

use crate::client::commands::{Context, Error};
///Various commands for changing some settings.
#[poise::command(
    slash_command,
    subcommands(
        "temporary_channels",
    ),
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
    subcommand_required
)]
pub async fn settings(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}
