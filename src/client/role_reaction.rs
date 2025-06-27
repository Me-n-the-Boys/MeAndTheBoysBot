use serenity::client::Context;
use serenity::all::{ReactionAddEvent, ReactionRemoveEvent};

pub async fn add_reaction(ctx: &'_ Context, add: &ReactionAddEvent) {
    let member = match &add.reaction.member {
        None => return,
        Some(v) => v,
    };
    let guild_id = member.guild_id;

    let db = crate::get_db().await;
    let roles = member.roles.iter().map(|v|v.get().cast_signed()).collect::<Vec<_>>();
    match sqlx::query!("SELECT give_role_id FROM role_reactions
LEFT JOIN role_limiter ON role_limiter.guild_id = $1 AND role_reactions.give_role_id = role_limiter.role_id
WHERE role_reactions.guild_id = $1 AND role_reactions.message_id = $2 AND role_limit_predicate($3, role_limiter.bind_roles)
", guild_id.get().cast_signed(), add.reaction.message_id.get().cast_signed(), roles.as_slice())
        .fetch_optional(&db)
        .await
    {
        Ok(Some(v)) => {
            let role = v.give_role_id.cast_unsigned();
            match member.add_role(&ctx, role).await {
                Ok(()) => {},
                Err(err) => {
                    log::error!("Failed to add role {role} to user {}: {err}", member.user.id.get());
                    return;
                }
            }
        },
        Ok(None) => {},
        Err(v) => {
            log::error!("Error whilst getting trying to handle role reaction: {v}");
            return;
        }
    }
}
pub async fn remove_reaction(ctx: &'_ Context, remove: &ReactionRemoveEvent) {
    let member = match &remove.reaction.user_id {
        None => return,
        Some(v) => v,
    };
    let guild_id = match &remove.reaction.guild_id {
        None => return,
        Some(v) => v,
    };

    let db = crate::get_db().await;
    match sqlx::query!("SELECT give_role_id FROM role_reactions
WHERE role_reactions.guild_id = $1 AND role_reactions.message_id = $2
", guild_id.get().cast_signed(), remove.reaction.message_id.get().cast_signed())
        .fetch_optional(&db)
        .await
    {
        Ok(Some(v)) => {
            let role = v.give_role_id.cast_unsigned();
            match guild_id.member(&ctx, member).await {
                Ok(member) => match member.remove_role(&ctx, role).await {
                    Ok(()) => {},
                    Err(err) => {
                        log::error!("Failed to remove role {role} from user {member} in guild {guild_id}: {err}");
                        return;
                    }
                },
                Err(err) => {
                    log::error!("Failed to get user {member} in guild {guild_id}: {err}");
                    return;
                }
            }
        },
        Ok(None) => {},
        Err(v) => {
            log::error!("Error whilst getting trying to handle role reaction: {v}");
            return;
        }
    }
}