use super::{Context, Error};

///Various commands for debugging purposes.
#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
)]
pub async fn debug(ctx: Context<'_>) -> Result<(), Error> {
    let handler = &ctx.data().handler;
    ctx.say(format!("{handler:?}")).await?;
    Ok(())
}