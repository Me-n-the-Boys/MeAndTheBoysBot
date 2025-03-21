use std::collections::HashSet;
use std::num::NonZeroU64;
use poise::serenity_prelude as serenity;

impl super::Handler {
    pub(crate) async fn vc_join_channel_temp_channel(&self, db: sqlx::PgPool, ctx: serenity::Context, guild_id: serenity::GuildId, user_id: serenity::UserId, channel_id: serenity::ChannelId, old_state: &Option<serenity::all::VoiceState>) {
        match sqlx::query!(r#"SELECT $2 = ANY(SELECT creator_channel FROM temp_channels WHERE guild_id = $1) as "is_creator_channel!""#, crate::converti(guild_id.get()), crate::converti(channel_id.get())).fetch_one(&db).await {
            Ok(v) => match v.is_creator_channel {
                true => {
                    self.create_channel(db, &ctx, user_id, guild_id).await;
                }
                false => {
                    match sqlx::query!("UPDATE temp_channels_created SET mark_delete = NULL WHERE channel_id = $1 AND guild_id = $2", crate::converti(channel_id.get()), crate::converti(guild_id.get())).execute(&db).await {
                        Ok(v) => match v.rows_affected() {
                            0 => {
                                tracing::info!("Channel {channel_id} was already deleted?");
                            },
                            _ => {},
                        },
                        Err(err) => {
                            tracing::error!("Error updating temp_channels_created: {err}");
                        }
                    }
                    self.check_delete_channels(db, ctx, guild_id, old_state).await
                },
            }
            Err(err) => {
                tracing::error!("Error checking if channel is a creator channel: {err}");
            }
        }
    }

    async fn log_error(&self, ctx: &poise::serenity_prelude::Context, channel: serenity::ChannelId, error: String) {
        let send_message = serenity::CreateMessage::new()
            .content(error.clone())
            .allowed_mentions(serenity::CreateAllowedMentions::new().empty_roles().empty_users());
        match channel.send_message(&ctx, send_message).await {
            Ok(_) => {
                tracing::warn!("Logged error: {error}");
            }
            Err(err) => {
                tracing::error!("Original Error: {error}");
                tracing::error!("Error sending error message: {err}");
            }
        }
    }
    async fn is_ignored_channel(&self, db: sqlx::PgPool, channel_id: serenity::ChannelId, guild_id: serenity::GuildId) -> bool {
        match sqlx::query!(r#"SELECT $2 = ANY(SELECT channel_id FROM temp_channels_ignore WHERE guild_id = $1) as "ignored!""#,
            crate::converti(guild_id.get()), crate::converti(channel_id.get())).fetch_one(&db).await
        {
            Ok(v) => v.ignored,
            Err(err) => {
                tracing::error!("Error checking if channel is ignored for temp channels: {err}");
                true
            },
        }
    }
    async fn check_delete_channel(&self, db: sqlx::PgPool, ctx: &poise::serenity_prelude::Context, channel: serenity::ChannelId, guild_id: serenity::GuildId) {
        let out = match sqlx::query!(r#"
MERGE INTO temp_channels_created USING
    ( SELECT
          guild_id,
          $2::bigint as channel_id,
          $2 = ANY(SELECT channel_id FROM temp_channels_ignore WHERE guild_id = $1) as "ignored!",
          delete_non_created_channels,
          creator_channel,
          create_category,
          delete_delay
        from temp_channels
        WHERE guild_id = $1
    ) AS input ON temp_channels_created.guild_id = input.guild_id AND temp_channels_created.channel_id = input.channel_id
WHEN MATCHED AND mark_delete IS NULL THEN UPDATE SET mark_delete = now()
WHEN NOT MATCHED THEN DO NOTHING
RETURNING input.*, temp_channels_created.channel_id as "created_channel_id?", mark_delete = now() as "mark_delete?", now() as "deleted_at!"
        "#,
            crate::converti(guild_id.get()), crate::converti(channel.get())).fetch_one(&db).await
        {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("Error checking if channel might need deletion: {err}");
                return;
            },
        };
        if out.ignored {
            return;
        }
        if !out.delete_non_created_channels && out.created_channel_id.is_none() {
            return;
        }
        if !out.mark_delete.unwrap_or(false) {
            tracing::warn!("Channel {channel} was already deleted?");
            //Channel was already deleted?
            return;
        }

        let creator_channel = serenity::ChannelId::new(crate::convertu(out.creator_channel));
        let instant = {
            const DAYS_PER_MONTH:i64 = 30;
            const SECONDS_PER_DAY:i64 = 24*60*60;
            let month_days = i64::saturating_mul(out.delete_delay.months as i64, DAYS_PER_MONTH);
            let day_seconds = i64::saturating_add(i64::saturating_mul(out.delete_delay.days.unsigned_abs() as i64, SECONDS_PER_DAY), month_days);
            let microseconds = out.delete_delay.microseconds;
            let seconds = i64::saturating_add(microseconds/1_000/1_000, day_seconds);
            let subsec_microseconds = microseconds%(1_000*1_000);
            if seconds < 0  || (seconds == 0 && subsec_microseconds < 0) {
                tracing::error!("Negative deletion delay time for guild {guild_id}. Delay: {:?}. Refusing to delete channel", out.delete_delay);
                self.log_error(&ctx, creator_channel, format!("Negative channel deletion delay time set?")).await;
                return;
            }
            std::time::Instant::now().checked_add(std::time::Duration::new(
                u64::try_from(seconds).unwrap_or(0),
                u32::saturating_mul(u32::try_from(subsec_microseconds).unwrap_or(0), 1_000)
            ))
        };

        if let Some(instant) = instant {
            tracing::info!("Sleeping for channel {channel} for deletion");
            tokio::time::sleep_until(tokio::time::Instant::from(instant)).await;
        }else {
            tracing::error!("Error calculating deletion delay time for guild {guild_id}. Delay: {:?}. Refusing to delete channels.", out.delete_delay);
            self.log_error(&ctx, creator_channel, format!("Error calculating deletion delay time. Delay: {:?}. Refusing to delete channels.", out.delete_delay)).await;
            return;
        }

        match sqlx::query!(r#"DELETE FROM temp_channels_created WHERE guild_id = $2 AND channel_id = $3 AND mark_delete IS NOT NULL AND mark_delete = $1::timestamptz RETURNING *"#,
            out.deleted_at, crate::converti(guild_id.get()), crate::converti(channel.get())).fetch_optional(&db).await
        {
            Err(v) => {
                tracing::error!("Error checking if channel should be deleted: {v}");
                self.log_error(&ctx, creator_channel, format!("Error checking if channel <#{channel}> ({channel}) was rejoined. Was it already deleted?")).await;
                return;
            }
            Ok(None) => {
                tracing::info!("Channel {channel} was rejoined");
                //Channel was rejoined
                return;
            },
            Ok(Some(_)) => {},
        }

        match channel.delete(&ctx).await {
            Ok(v) => {
                let id = v.id();
                let name = v.guild().map(|v| v.name).unwrap_or_else(|| "Unknown".to_string());
                tracing::info!("Deleted Channel {channel} (id: {id}) (name: {name}).");
                self.log_error(&ctx, creator_channel, format!("Deleted Channel {channel} (id: {id}) (name: {name}).")).await;
            },
            Err(err) => {
                self.log_error(&ctx, channel, format!("There was an error deleting the Channel: {err}")).await;
                tracing::error!("Error deleting channel: {err}");
                return;
            }
        }
    }
    pub(crate) async fn check_delete_channels(&self, db: sqlx::PgPool, ctx: poise::serenity_prelude::Context, guild_id: serenity::GuildId, old_state: &Option<serenity::all::VoiceState>) {
        let old_channel = old_state.as_ref().map(|old_state| old_state.channel_id).flatten();
        if let Some(old_channel) = old_channel {
            tracing::info!("Checking Channel for deletion: {old_channel}");
            self.check_delete_channel(db.clone(), &ctx, old_channel, guild_id).await;
        }

        let channels = match sqlx::query!(r#"SELECT channel_id FROM temp_channels_created WHERE guild_id = $1 AND mark_delete IS NULL"#, crate::converti(guild_id.get())).fetch_all(&db).await {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("Error getting created channels: {err}");
                return;
            }
        };

        for channel in channels {
            self.check_delete_channel(db.clone(), &ctx, serenity::ChannelId::new(crate::convertu(channel.channel_id)), guild_id).await;
        }
    }
    async fn create_channel(&self, db: sqlx::PgPool, ctx: &poise::serenity_prelude::Context, user_id: serenity::UserId, guild_id: serenity::GuildId) {
        let res = match sqlx::query!(r#"SELECT
(SELECT Count(*) FROM temp_channels_created WHERE guild_id = $1) + (SELECT COUNT(*) FROM temp_channels_ignore WHERE guild_id = $1) as "count!",
create_category
FROM temp_channels
WHERE guild_id = $1
"#, crate::converti(guild_id.get())).fetch_one(&db).await {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("Error getting channel count: {err}");
                return;
            }
        };
        let mut new_channel = serenity::CreateChannel::new("New Channel")
            .kind(serenity::model::channel::ChannelType::Voice)
            .position(u16::try_from(res.count).unwrap_or(u16::MAX))
            .permissions([
                serenity::model::channel::PermissionOverwrite {
                    allow:
                    serenity::all::Permissions::MANAGE_CHANNELS |
                        serenity::all::Permissions::MANAGE_ROLES |
                        serenity::all::Permissions::MOVE_MEMBERS |
                        serenity::all::Permissions::VIEW_CHANNEL |
                        serenity::all::Permissions::CONNECT,
                    deny: serenity::all::Permissions::empty(),
                    kind: serenity::model::channel::PermissionOverwriteType::Member(user_id),
                },
            ]);
        if let Some(create_category) = res.create_category {
            match NonZeroU64::new(crate::convertu(create_category)) {
                None => {},
                Some(category) => {
                    new_channel = new_channel.category(category);
                }
            }
        }
        let mut error = None;
        match user_id.to_user(&ctx).await{
            Ok(user) => {
                new_channel = new_channel.name(format!("{}'s Channel", user.global_name.unwrap_or(user.name)));
            },
            Err(v) => {
                error = Some(v);
            }
        }
        match guild_id.create_channel(&ctx, new_channel).await {
            Ok(v) => {
                if let Some(error) = error {
                    self.log_error(&ctx, v.id, format!("There was an error getting the Creator's User: {error}")).await;
                }
                match guild_id.move_member(&ctx, user_id, v.id).await {
                    Ok(_) => {},
                    Err(err) => {
                        tracing::error!("Error moving member to temporary channel. Deleting temporary channel. Error: {err}");
                        match v.delete(&ctx).await {
                            Ok(_) => {},
                            Err(err) => {
                                tracing::error!("Error deleting temporary channel after Moving user to temporary channel failed: {err}");
                            }
                        }
                    }
                }
                let channel = v.id;
                tracing::info!("Created channel {channel}, but did not insert yet");
                {
                    match sqlx::query!("INSERT INTO temp_channels_created (guild_id, channel_id, mark_delete) VALUES($1, $2, NULL)", crate::converti(guild_id.get()), crate::converti(channel.get())).execute(&db).await {
                        Ok(_) => {},
                        Err(err) => {
                            tracing::error!("Error inserting channel into temp_channels_created: {err}");
                            return;
                        }
                    }
                }
                tracing::info!("Created channel {channel}");
            }
            Err(err) => {
                if let Some(error) = error {
                    tracing::error!("Error getting the Creator's User with id {user_id}: {error}");
                }
                tracing::error!("Error creating channel for User {user_id}: {err}");
                return;
            }
        }
    }
}