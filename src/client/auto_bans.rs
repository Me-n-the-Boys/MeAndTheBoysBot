use serenity::all::{GuildId, Message, RoleId, UserId};

impl super::Handler {
    pub(super) async fn auto_bans_message(&self, ctx: &serenity::client::Context, message: &Message) {
        let channel = message.channel_id;
        let guild = match message.guild_id {
            Some(x) => x,
            None => return,
        };
        let db = crate::get_db().await;
        let res = match
        sqlx::query!("SELECT dmd FROM public.autobanchannels WHERE guild_id = $1 AND channel = $2", guild.get().cast_signed(), channel.get().cast_signed())
            .fetch_optional(&db).await {
            Ok(v) => v,
            Err(err) => {
                log::error!("Failed to query db for Auto Ban Channel: {err}");
                return;
            },
        };
        let res = match res {
            None => return,
            Some(v) => v.dmd
        };
        let res = if res < 0 { 0 } else if res > 7 { 7 } else { res as u8 };

        match guild.ban_with_reason(&ctx, message.author.id, res, "Posted a message in a Channel which has auto-bans enabled.").await {
            Ok(_) => (),
            Err(err) => {
                log::error!("Failed to ban user {} for writing a message in an auto ban channel: {err}", message.author.id);
                return;
            }
        }
        match message.delete(&ctx).await {
            Ok(_) => (),
            Err(err) => {
                log::error!("Failed to delete a message in an auto ban channel: {err}");
                return;
            }
        }

    }
    pub(super) async fn auto_bans_roles(&self, ctx: &serenity::client::Context, guild: GuildId, user: UserId, roles: &[RoleId]){
        let db = crate::get_db().await;
        let roles = roles.iter().map(|v|v.get().cast_signed()).collect::<Vec<_>>();
        let res = match sqlx::query!(r#"SELECT MAX(dmd) as "dmd!" FROM public.autobanroles WHERE guild_id = $1 AND roleid IN (SELECT * FROM UNNEST($2::bigint[]))"#,
            guild.get().cast_signed(), roles.as_slice()
        ).fetch_optional(&db).await {
            Ok(v) => v,
            Err(err) => {
                log::error!("DB error whilst processing Auto Ban Roles: {err}");
                return;
            }
        };
        let res = match res {
            None => return,
            Some(v) => v.dmd
        };
        let res = if res < 0 { 0 } else if res > 7 { 7 } else { res as u8 };

        match guild.ban_with_reason(&ctx, user, res, "User gained a role, which auto-bans.").await{
            Ok(_) => (),
            Err(err) => {
                log::error!("Failed to ban user {user} from {guild}: {err}");
                return
            }
        }

    }
}