use std::num::NonZeroU64;
use poise::serenity_prelude as serenity;
use serenity::http::CacheHttp;

impl super::Handler {
    pub(crate) async fn vc_join_channel_temp_channel(&self, ctx: serenity::Context, guild_id: serenity::GuildId, user_id: serenity::UserId, channel_id: serenity::ChannelId) {
        match sqlx::query!(r#"SELECT $2 = ANY(SELECT creator_channel FROM temp_channels WHERE guild_id = $1) as "is_creator_channel!""#, crate::converti(guild_id.get()), crate::converti(channel_id.get())).fetch_one(&self.pool).await {
            Ok(v) => match v.is_creator_channel {
                true => {
                    self.create_channel(&ctx, user_id, guild_id).await;
                }
                false => {
                    match sqlx::query!("UPDATE temp_channels_created SET mark_delete = NULL WHERE channel_id = $1 AND guild_id = $2", crate::converti(channel_id.get()), crate::converti(guild_id.get())).execute(&self.pool).await {
                        Ok(v) => match v.rows_affected() {
                            0 => {
                                tracing::info!("Channel {channel_id} was already deleted or isn't a created channel");
                            },
                            _ => {},
                        },
                        Err(err) => {
                            tracing::error!("Error updating temp_channels_created: {err}");
                        }
                    }
                    match sqlx::query!("
WITH input AS (SELECT $2::bigint as guild_id, $1::bigint as channel_id, $3::bigint as user_id)

MERGE INTO temp_channels_created_users USING
    (SELECT input.*,
        input.channel_id = ANY(SELECT channel_id FROM temp_channels_created WHERE temp_channels_created.guild_id = input.guild_id) as is_created_channel,
        input.channel_id = ANY(SELECT channel_id FROM temp_channels_ignore WHERE temp_channels_ignore.guild_id = input.guild_id) as is_ignored_channel
     FROM input
    ) AS input
    ON temp_channels_created_users.guild_id = input.guild_id AND temp_channels_created_users.user_id = input.user_id
WHEN MATCHED AND input.is_ignored_channel THEN DELETE
WHEN MATCHED AND input.is_created_channel THEN UPDATE SET channel_id = input.channel_id
WHEN MATCHED THEN DELETE
WHEN NOT MATCHED AND input.is_ignored_channel THEN DO NOTHING
WHEN NOT MATCHED AND input.is_created_channel THEN INSERT (guild_id, channel_id, user_id) VALUES (input.guild_id, input.channel_id, input.user_id)
WHEN NOT MATCHED THEN DO NOTHING
", crate::converti(channel_id.get()), crate::converti(guild_id.get()), crate::converti(user_id.get())).execute(&self.pool).await {
                        Ok(v) => match v.rows_affected() {
                            0 => {
                                tracing::info!("Channel {channel_id} isn't a created channel?");
                            },
                            _ => {},
                        },
                        Err(err) => {
                            tracing::error!("Error updating temp_channels_created_users: {err}");
                        }
                    }
                    self.check_delete_channels(ctx).await
                },
            }
            Err(err) => {
                tracing::error!("Error checking if channel is a creator channel: {err}");
            }
        }
    }

    async fn log_error(&self, ctx: impl CacheHttp, channel: serenity::ChannelId, error: String) {
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
    async fn is_channel_empty(&self, guild_id: serenity::GuildId, channel_id: serenity::ChannelId) -> bool {
        match sqlx::query!(
            r#"SELECT Count(*) as "count!" FROM temp_channels_created_users WHERE guild_id = $1 AND channel_id = $2"#,
            crate::converti(guild_id.get()), crate::converti(channel_id.get())
        ).fetch_one(&self.pool).await {
            Ok(v) => match v.count {
                0 => true,
                _ => {
                    tracing::info!("Channel {channel_id} is not empty.");
                    false
                },
            },
            Err(err) => {
                tracing::error!("Error checking if channel is empty: {err}");
                false
            }
        }
    }

    async fn check_delete_channel(&self, ctx: impl CacheHttp, channel: serenity::ChannelId, guild_id: serenity::GuildId) {
        if !self.is_channel_empty(guild_id, channel).await {
            return;
        }
        let mark_delete = match sqlx::query!(r#"
MERGE INTO temp_channels_created USING
    ( SELECT
          $1::bigint as guild_id,
          $2::bigint as channel_id
    ) AS input ON temp_channels_created.guild_id = input.guild_id AND temp_channels_created.channel_id = input.channel_id
WHEN MATCHED AND mark_delete IS NULL THEN UPDATE SET mark_delete = now()
WHEN NOT MATCHED THEN DO NOTHING
RETURNING mark_delete = now() as "mark_delete!", now() as "deleted_at!"
        "#,
            crate::converti(guild_id.get()), crate::converti(channel.get())).fetch_optional(&self.pool).await
        {
            Ok(None) => {
                tracing::info!("We did not create or have permission to delete channel: <#{channel}>");
                return;
            },
            Ok(Some(v)) => v,
            Err(err) => {
                tracing::error!("Error checking if channel might need deletion: {err}");
                return;
            },
        };

        let info = match sqlx::query!(r#"
SELECT
  guild_id,
  $2 = ANY(SELECT channel_id FROM temp_channels_ignore WHERE temp_channels_ignore.guild_id = temp_channels.guild_id) as "ignored!",
  creator_channel,
  create_category,
  delete_delay
from temp_channels
WHERE guild_id = $1
        "#,
            crate::converti(guild_id.get()), crate::converti(channel.get())).fetch_one(&self.pool).await
        {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("Error checking if channel might need deletion: {err}");
                return;
            },
        };
        if info.ignored {
            return;
        }
        if !mark_delete.mark_delete {
            tracing::warn!("Channel {channel} was already deleted?");
            //Channel was already deleted?
            return;
        }

        let creator_channel = serenity::ChannelId::new(crate::convertu(info.creator_channel));
        let instant = {
            const DAYS_PER_MONTH:i64 = 30;
            const SECONDS_PER_DAY:i64 = 24*60*60;
            let month_days = i64::saturating_mul(info.delete_delay.months as i64, DAYS_PER_MONTH);
            let day_seconds = i64::saturating_add(i64::saturating_mul(info.delete_delay.days.unsigned_abs() as i64, SECONDS_PER_DAY), month_days);
            let microseconds = info.delete_delay.microseconds;
            let seconds = i64::saturating_add(microseconds/1_000/1_000, day_seconds);
            let subsec_microseconds = microseconds%(1_000*1_000);
            if seconds < 0  || (seconds == 0 && subsec_microseconds < 0) {
                tracing::error!("Negative deletion delay time for guild {guild_id}. Delay: {:?}. Refusing to delete channel", info.delete_delay);
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
            tracing::error!("Error calculating deletion delay time for guild {guild_id}. Delay: {:?}. Refusing to delete channels.", info.delete_delay);
            self.log_error(&ctx, creator_channel, format!("Error calculating deletion delay time. Delay: {:?}. Refusing to delete channels.", info.delete_delay)).await;
            return;
        }

        if !self.is_channel_empty(guild_id, channel).await {
            return;
        }

        match sqlx::query!(r#"DELETE FROM temp_channels_created WHERE guild_id = $2 AND channel_id = $3 AND mark_delete IS NOT NULL AND mark_delete = $1::timestamptz RETURNING *"#,
            mark_delete.deleted_at, crate::converti(guild_id.get()), crate::converti(channel.get())).fetch_optional(&self.pool).await
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

        match channel.delete(ctx.http()).await {
            Ok(v) => {
                let id = v.id();
                let name = v.guild().map(|v| v.name).unwrap_or_else(|| "Unknown".to_string());
                tracing::info!("Deleted Channel {channel} (id: {id}) (name: {name}).");
                self.log_error(&ctx, creator_channel, format!("Deleted Channel <#{channel}> (id: {id}) (name: {name}).")).await;
            },
            Err(err) => {
                self.log_error(&ctx, channel, format!("There was an error deleting the Channel: {err}")).await;
                tracing::error!("Error deleting channel: {err}");
                return;
            }
        }
    }
    pub(crate) async fn check_delete_channels(&self, ctx: impl CacheHttp) {
        let channels = match sqlx::query!(r#"SELECT guild_id, channel_id FROM temp_channels_created WHERE (
SELECT COUNT(*) FROM temp_channels_created_users WHERE temp_channels_created_users.guild_id = temp_channels_created.guild_id AND temp_channels_created_users.channel_id = temp_channels_created.channel_id
) = 0"#).fetch_all(&self.pool).await {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("Error getting created channels: {err}");
                return;
            }
        };

        for channel in channels {
            self.check_delete_channel(&ctx, serenity::ChannelId::new(crate::convertu(channel.channel_id)), serenity::GuildId::new(crate::convertu(channel.guild_id))).await;
        }
    }
    async fn create_channel(&self, ctx: &poise::serenity_prelude::Context, user_id: serenity::UserId, guild_id: serenity::GuildId) {
        let res = match sqlx::query!(
            r#"SELECT create_category FROM temp_channels WHERE guild_id = $1"#,
            crate::converti(guild_id.get())
        ).fetch_one(&self.pool).await {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("Error getting channel count: {err}");
                return;
            }
        };
        let position = {
            (async ||{
                let channels = match guild_id.channels(&ctx).await {
                    Ok(guild) => guild,
                    Err(err) => {
                        tracing::error!("Error getting guild channels: {err}");
                        return None;
                    }
                };
                channels.iter().filter(|(_, channel)| {
                    channel.parent_id.map(|v|v.get()) == res.create_category.map(crate::convertu)
                }).map(|(_, channel)| {
                    channel.position
                }).max().map(|v|v.saturating_add(1))
            })().await
        };

        let mut new_channel = serenity::CreateChannel::new("New Channel")
            .kind(serenity::model::channel::ChannelType::Voice)
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
        if let Some(position) = position {
            new_channel = new_channel.position(position);
        }
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
               self.channel_create(&v, true).await;
                tracing::info!("Created channel {v}");
                match guild_id.move_member(&ctx, user_id, v.id).await {
                    Ok(_) => {},
                    Err(err) => {
                        tracing::error!("Error moving member to temporary channel. Deleting temporary channel. Error: {err}");
                        self.log_error(&ctx, v.id, format!("Error moving member to temporary channel. Deleting temporary channel. Error: {err}")).await;
                        match v.delete(&ctx).await {
                            Ok(_) => {},
                            Err(err) => {
                                tracing::error!("Error deleting temporary channel after Moving user to temporary channel failed: {err}");
                            }
                        }
                        //Channel deletion is handled automatically by the event handler.
                    }
                }
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