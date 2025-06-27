use poise::{serenity_prelude as serenity};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Default, Copy, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct TextXpInfo {
    pub xp: i128,
    pub pending: Option<(u64, serenity::Timestamp)>,
}

impl super::Handler {
    pub(super) async fn apply_previous_message_xp(&self, user_id: Option<serenity::UserId>, guild_id: Option<serenity::GuildId>) {
        let result = match sqlx::query!(
r#"SELECT
    guild_id as "guild_id!",
    user_id as "user_id!",
    total_xp as "total_xp!",
    applyable_xp as "applyable_xp!",
    xp_punish as "xp_punish!",
    duration as "duration!"
FROM apply_previous_message_xp($1, $2)"#,
guild_id.map(|v|crate::converti(v.get())),
user_id.map(|v|crate::converti(v.get()))
        ).fetch_all(&self.pool).await
        {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("Error applying previous message xp: {err}");
                return;
            }
        };
        for result in result {
            if result.xp_punish {
                tracing::warn!("User {} has triggered the xp spam limit. Queued are {} xp from {:?} ago. {} xp are applyable. {} xp are removed as spam.", result.user_id, result.total_xp, result.duration, result.applyable_xp, result.total_xp - result.applyable_xp);
            }
            if result.applyable_xp != result.total_xp {
                tracing::info!("User {} has gotten an unusual amount of xp. Queued outstanding xp for application. Queued are {} xp. {} xp were already applied", result.user_id, result.total_xp, result.applyable_xp);
            }
        }
    }
    pub(in super) async fn message_xp(&self, message: serenity::Message) {
        let guild_id = match message.guild_id {
            Some(v) => v,
            None => return,
        };
        //Ignore message, if sent in an ignored channel or by a bot
        if message.author.bot || message.author.system || self.xp_is_ignored(message.channel_id, guild_id).await {
            return;
        }
        //Apply message xp
        {
            let xp = calculate_message_text_xp(BASE_TEXT_XP, &message);
            self.apply_previous_message_xp(Some(message.author.id), Some(guild_id)).await;
            self.add_tmp_txt_xp(message.author.id, guild_id, xp).await;
        };
    }
    pub(crate) async fn message_xp_react(&self, reaction: &serenity::Reaction) {
        let guild_id = match reaction.guild_id {
            Some(v) => v,
            None => return,
        };
        if self.xp_is_ignored(reaction.channel_id, guild_id).await { return;}

        let mut xp = BASE_XP_REACT;
        if reaction.burst { xp*=2; }
        {
            if let Some(member) = &reaction.member {
                self.apply_previous_message_xp(Some(member.user.id), Some(guild_id)).await;
                self.add_tmp_txt_xp(member.user.id, guild_id, xp).await;
            }
            if let Some(member) =  reaction.message_author_id {
                self.add_tmp_txt_xp(member, guild_id, xp).await;
            }
        }
    }
    async fn xp_is_ignored(&self, channel_id: serenity::ChannelId, guild_id: serenity::GuildId) -> bool {
        match sqlx::query!(r#"SELECT $1 = ANY(SELECT channel_id FROM xp_channels_ignored WHERE guild_id = $2) as "ignored!""#, crate::converti(channel_id.get()), crate::converti(guild_id.get())).fetch_one(&self.pool).await {
            Ok(v) => v.ignored,
            Err(err) => {
                tracing::error!("Error checking if channel is ignored for xp: {err}");
                true
            },
        }
    }
    async fn add_tmp_txt_xp(&self, user_id: serenity::UserId, guild_id: serenity::GuildId, xp: i64) {
        match sqlx::query!(r#"MERGE INTO xp_txt_tmp USING (SELECT $1::bigint as user_id, $2::bigint as guild_id) AS input ON xp_txt_tmp.user_id = input.user_id AND xp_txt_tmp.guild_id = input.guild_id
WHEN MATCHED THEN UPDATE SET xp = xp_txt_tmp.xp + $3
WHEN NOT MATCHED THEN INSERT (guild_id, user_id, xp, time) VALUES(input.guild_id, input.user_id, $3, now())
"#, crate::converti(user_id.get()), crate::converti(guild_id.get()), xp).execute(&self.pool).await {
            Ok(v) => match v.rows_affected() {
                0 => {
                    tracing::error!("Error adding xp to xp_txt_tmp: No rows affected");

                },
                _ => {},
            },
            Err(err) => {
                tracing::error!("Error adding xp to xp_txt_tmp: {err}");
            }
        }
    }
}

const BASE_TEXT_XP: i64 = 10;
const BASE_XP_REACT: i64 = 2;
fn calculate_message_text_xp(base_xp: i64, message: &serenity::Message) -> i64 {
    let mut xp = base_xp as f64;
    xp *=
        1. +
        message.content.len() as f64 / 1000. +
        message.attachments.len() as f64 * 0.25 +
        message.embeds.len() as f64 * 0.1;

    xp as i64
}