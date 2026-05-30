use serenity::all::{ChannelId};
use crate::client::commands::{Context, Error};

#[poise::command(
    slash_command,
    subcommands(
        "list_channels",
        "add_channel",
        "remove_channel",
    ),
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
    subcommand_required,
)]
pub async fn auto_bans(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(
    slash_command,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
)]
///Add a channel where any message will insta-ban the sender
pub async fn add_channel(ctx: Context<'_>, channel: ChannelId, delete_message_days: u8) -> Result<(), Error> {
    let guild_id = match ctx.guild_id() {
        Some(v) => v,
        None => return Err("This command needs to be run from a guild".into())
    };
    let db = crate::get_db().await;
    sqlx::query!(
        r#"INSERT INTO public.autobanchannels (guild_id, channel, dmd) VALUES ($1, $2, $3)"#,
        guild_id.get().cast_signed(), channel.get().cast_signed(), i16::from(delete_message_days),
    )
        .execute(&db)
        .await?;
    ctx.say("Added Auto-Ban Channel").await?;
    Ok(())
}
#[poise::command(
    slash_command,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
)]
///Remove a channel where any message will insta-ban the sender
pub async fn remove_channel(ctx: Context<'_>, channel: ChannelId) -> Result<(), Error> {
    let guild_id = match ctx.guild_id() {
        Some(v) => v,
        None => return Err("This command needs to be run from a guild".into())
    };
    let db = crate::get_db().await;
    sqlx::query!(
        r#"DELETE FROM public.autobanchannels WHERE guild_id = $1 AND channel = $2"#,
        guild_id.get().cast_signed(), channel.get().cast_signed(),
    )
        .execute(&db)
        .await?;
    ctx.say("Added Auto-Ban Channel").await?;
    Ok(())
}
#[poise::command(
    slash_command,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
)]
///Remove a channel where any message will insta-ban the sender
pub async fn list_channels(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = match ctx.guild_id() {
        Some(v) => v,
        None => return Err("This command needs to be run from a guild".into())
    };
    let db = crate::get_db().await;
    let res = sqlx::query!(
        r#"SELECT channel FROM public.autobanchannels WHERE guild_id = $1"#,
        guild_id.get().cast_signed(),
    )
        .fetch_all(&db)
        .await?;
    let mut out = "Currently Banned Channels:\n".to_string();
    for v in res {
        out.push_str(&format!("\t<#{}>\n", v.channel));
    }

    ctx.say(out).await?;
    Ok(())
}