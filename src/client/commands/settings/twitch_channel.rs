use serenity::all::{ChannelId};
use crate::client::commands::{Context, Error};

#[poise::command(
    slash_command,
    subcommands(
        "add",
        "set_add_self",
        "remove"
    ),
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
    subcommand_required,
)]
pub async fn twitch_channel(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(
    slash_command,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
)]
///Add a channel for announcing twitch channel livestreams
pub async fn add(ctx: Context<'_>, channel: ChannelId, add_self: Option<bool>) -> Result<(), Error> {
    let add_self = add_self.unwrap_or(false);
    let guild_id = match ctx.guild_id(){
        None => return Err("The command needs to be run in a guild".into()),
        Some(v) => v,
    };
    let channel_id = crate::converti(channel.get());
    let db = crate::get_db().await;
    match sqlx::query!("INSERT INTO twitch_channel (guild, channel_id, add_self) VALUES ($1, $2, $3) ON CONFLICT (guild, channel_id) DO UPDATE SET add_self = $3", crate::converti(guild_id.get()), channel_id, add_self).execute(&db).await {
        Ok(_) => Ok(()),
        Err(err) => Err(err.into())
    }
}
#[poise::command(
    slash_command,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
)]
///Add a channel for announcing twitch channel livestreams
pub async fn set_add_self(ctx: Context<'_>, channel: ChannelId, add_self: bool) -> Result<(), Error> {
    let guild_id = match ctx.guild_id(){
        None => return Err("The command needs to be run in a guild".into()),
        Some(v) => v,
    };
    let channel_id = crate::converti(channel.get());
    let db = crate::get_db().await;
    match sqlx::query!("UPDATE twitch_channel SET add_self = $3 WHERE guild = $1 AND channel_id = $2", crate::converti(guild_id.get()), channel_id, add_self).execute(&db).await {
        Ok(_) => Ok(()),
        Err(err) => Err(err.into())
    }
}

#[poise::command(
    slash_command,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
)]
///Add a channel for announcing twitch channel livestreams
pub async fn remove(ctx: Context<'_>, channel: ChannelId) -> Result<(), Error> {
    let guild_id = match ctx.guild_id(){
        None => return Err("The command needs to be run in a guild".into()),
        Some(v) => v,
    };
    let channel_id = crate::converti(channel.get());
    let db = crate::get_db().await;
    match sqlx::query!("DELETE FROM twitch_channel WHERE guild = $1 AND channel_id = $2", crate::converti(guild_id.get()), channel_id).execute(&db).await {
        Ok(_) => Ok(()),
        Err(err) => Err(err.into())
    }
}
