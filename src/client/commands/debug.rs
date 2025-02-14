use super::{Context, Error};

///Various commands for debugging purposes.
#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
)]
pub async fn debug(ctx: Context<'_>) -> Result<(), Error> {
    let handler = &ctx.data().handler;
    let mut reply = ctx.reply_builder(poise::CreateReply::default());
    let content = serde_json::to_vec(handler)?;
    reply = reply.attachment(serenity::builder::CreateAttachment::bytes(content, "state.json"));
    ctx.send(reply).await?;
    Ok(())
}