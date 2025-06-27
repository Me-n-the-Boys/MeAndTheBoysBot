use crate::client::commands::{Context, Error};
use serenity::all::{ChannelId, MessageId, ReactionType, RoleId};

///Various commands for changing some settings.
#[poise::command(
    slash_command,
    subcommands(
        "add",
        "remove"
    ),
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
    subcommand_required,
)]
pub async fn reaction_roles(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(
    slash_command,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
)]
///Add a reaction role
pub async fn add(ctx: Context<'_>, message: MessageId, give_role: RoleId, #[min_length = 1] emoji: String) -> Result<(), Error> {
    let emoji = match serenity::utils::parse_emoji(&emoji) {
        Some(v) => v.into(),
        None => ReactionType::Unicode(emoji)
    };
    ctx.channel_id().message(&ctx, message).await?.react(&ctx, emoji.clone()).await?;
    let emoji = serde_json::to_value(&emoji)?;
    let db = crate::get_db().await;
    let guild_id = match ctx.guild_id() {
        Some(v) => v,
        None => return Err("This command needs to be run from a guild".into())
    };
    sqlx::query!(
        r#"INSERT INTO public.role_reactions (guild_id, message_id, emoji, give_role_id, channel_id) VALUES ($1, $2, $3, $4, $5)"#,
        guild_id.get().cast_signed(), message.get().cast_signed(), emoji, give_role.get().cast_signed(), ctx.channel_id().get().cast_signed()
    )
        .execute(&db)
        .await?;
    ctx.say("Added role reaction").await?;
    Ok(())
}
#[poise::command(
    slash_command,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
)]
/// Removes a RoleReaction from a message with the specified give_roles (or all, if unspecified).
pub async fn remove(ctx: Context<'_>, message_id: MessageId, give_roles: Vec<RoleId>) -> Result<(), Error> {
    let db = crate::get_db().await;
    let guild_id = match ctx.guild_id() {
        Some(v) => v,
        None => return Err("This command needs to be run from a guild".into())
    };
    let give_roles = give_roles.into_iter().map(|v|v.get().cast_signed()).collect::<Vec<_>>();
    let emojis = sqlx::query!(
        r#"DELETE FROM public.role_reactions WHERE guild_id = $1 AND message_id = $2 AND give_role_id = ANY($3) RETURNING emoji, channel_id"#,
        guild_id.get().cast_signed(), message_id.get().cast_signed(),
        give_roles.as_slice(),
    )
        .fetch_all(&db)
        .await?;
    for emoji in emojis {
        let channel_id = emoji.channel_id.cast_unsigned();
        let emoji:ReactionType = serde_json::from_value(emoji.emoji)?;
        ChannelId::new(channel_id).delete_reaction(&ctx, message_id, None, emoji).await?;
    }
    ctx.say("Remove role reactions").await?;
    Ok(())
}
