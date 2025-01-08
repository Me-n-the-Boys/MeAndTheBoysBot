use super::{Context, Error};

///Various commands for debugging purposes.
#[poise::command(
    slash_command,
    subcommands(
        "debug_guild_id",
        "debug_temp_channels",
        "debug_xp",
    ),
    ephemeral,
    owners_only,
    subcommand_required
)]
pub async fn debug(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}
///Various commands for debugging temporary channels.
#[poise::command(
    slash_command,
    subcommands(
        "debug_creator_channel",
        "debug_create_category",
        "debug_creator_ignore_channels",
        "debug_creator_delete_non_created_channels",
        "debug_created_channels",
        "debug_creator_delete_delay",
        "debug_creator_mark_delete_channels",
    ),
    rename = "temp_channels",
    ephemeral,
    owners_only,
    subcommand_required
)]
async fn debug_temp_channels(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}
///Various commands for debugging xp.
#[poise::command(
    slash_command,
    subcommands(
        "debug_xp_ignored_channels",
        "debug_xp_vc_tmp",
        "debug_xp_vc",
        "debug_xp_txt",
        "debug_xp_txt_spam_delay_seconds",
        "debug_xp_txt_punish_seconds",
        "debug_xp_txt_reset",
    ),
    rename = "xp",
    ephemeral,
    owners_only,
    subcommand_required
)]
async fn debug_xp(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "guild_id"
)]
async fn debug_guild_id(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Configured Guild ID: {}", ctx.data().handler.guild_id)).await?;
    Ok(())
}
#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "channel"
)]
async fn debug_creator_channel(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Configured Channel for creating Temporary Channels: <#{}>", ctx.data().handler.creator_channel.get())).await?;
    Ok(())
}
#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "create_category"
)]
async fn debug_create_category(ctx: Context<'_>) -> Result<(), Error> {
    match ctx.data().handler.create_category{
        None => ctx.say("Configured Category for Temporary Channels: None").await?,
        Some(v) => ctx.say(format!("Configured Category for Temporary Channels: <#{0:?}> ({0:?})", v.get())).await?,
    };
    
    Ok(())
}
#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "ignore_channels"
)]
async fn debug_creator_ignore_channels(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Ignored Channels for TempChannels (e.g. for ignoring certain channels from being deleted, even if creator_delete_non_created_channels=true): {:?}", ctx.data().handler.creator_ignore_channels.iter().map(|v|format!("<#{:?}>", v.get())).collect::<Vec<_>>())).await?;
    Ok(())
}
#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "delete_non_created_channels"
)]
async fn debug_creator_delete_non_created_channels(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Should Non-Created channels in the Temporary Channel Category (if set) be deleted (excluding ignored ones): {}", ctx.data().handler.creator_delete_non_created_channels)).await?;
    Ok(())
}
#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "created_channels"
)]
async fn debug_created_channels(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Currently Active Temporary Channels: {:?}", ctx.data().handler.created_channels.read().await.keys().map(|v|format!("<#{:?}>", v.get())).collect::<Vec<_>>())).await?;
    Ok(())
}
#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "delete_delay"
)]
async fn debug_creator_delete_delay(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Temporary Channel Delete Delay: {:?}", ctx.data().handler.creator_delete_delay)).await?;
    Ok(())
}
#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "mark_delete_channels"
)]
async fn debug_creator_mark_delete_channels(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Temporary Channel Delete Queue: {:?}", ctx.data().handler.creator_mark_delete_channels.read().await.iter().map(|(c, i)|format!("<#{:?}> : {i:?}", c.get())).collect::<Vec<_>>())).await?;
    Ok(())
}
#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "ignored_channels"
)]
async fn debug_xp_ignored_channels(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Ignored Channels for Xp Gain: {:?}", ctx.data().handler.xp_ignored_channels.iter().map(|v|format!("<#{:?}>", v.get())).collect::<Vec<_>>())).await?;
    Ok(())
}

#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "vc_join"
)]
async fn debug_xp_vc_tmp(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Queue of In-VC Time that has not been applied: {:?}", ctx.data().handler.xp_vc_tmp.lock().await.iter().map(|(u, v)|format!("<@{:?}> : {v}", u.get())).collect::<Vec<_>>())).await?;
    Ok(())
}

#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "vc"
)]
async fn debug_xp_vc(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Voice Xp: {:?}", ctx.data().handler.xp_vc.lock().await.iter().map(|(u, v)|format!("<@{:?}> : {v:?}", u.get())).collect::<Vec<_>>())).await?;
    Ok(())
}

#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "text"
)]
async fn debug_xp_txt(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Text Xp: {:?}", ctx.data().handler.xp_txt.lock().await.iter().map(|(u, v)|format!("<@{:?}> : {v:?}", u.get())).collect::<Vec<_>>())).await?;
    Ok(())
}

#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "text_apply_milliseconds"
)]
async fn debug_xp_txt_spam_delay_seconds(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("\
This setting controls how long a Text Xp \"takes\" to apply.
This setting is used to limit xp gain per time period.
If a person were to gain more xp than this, it will be removed instead.
This is intended as a way to prevent spamming from being a viable strategy to gain xp.
(in reality the duration of time from receiving the last (non ignored) message of that user and the this setting is used to calculate the max xp gain. If necessary any xp over this limit is removed, and xp until that limit is added to the user's xp.)
Time in Milliseconds to gain one Xp: {}", ctx.data().handler.xp_txt_apply_milliseconds)).await?;
    Ok(())
}
#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "text_punish_seconds"
)]
async fn debug_xp_txt_punish_seconds(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say(format!("Time after Seconds of theoretical max xp gain to start punishing people (some other punishments will still happen for technical reasons): {}", ctx.data().handler.xp_txt_punish_seconds)).await?;
    Ok(())
}

#[poise::command(
    slash_command,
    ephemeral,
    owners_only,
    rename = "text_reset"
)]
async fn debug_xp_txt_reset(ctx: Context<'_>) {
    ctx.data().handler.xp_txt.lock().await.clear();
    ctx.say("Text Xp has been reset.").await?;
    Ok(())
}