use std::collections::HashSet;
use std::num::NonZeroU64;
use poise::serenity_prelude as serenity;

impl super::Handler {
    pub(crate) async fn vc_join_channel_temp_channel(&self, ctx: serenity::Context, user_id: serenity::UserId, channel_id: serenity::ChannelId, old_state: &Option<serenity::all::VoiceState>) {
        if channel_id.get() == self.creator_channel.load(std::sync::atomic::Ordering::Acquire) {
            self.create_channel(&ctx, user_id).await;
        } else {
            //This needs to be in a separate scope, to make sure, that the write guard is getting dropped before the check_delete_channels call
            {
                if let Some(mut delete) = self.creator_mark_delete_channels.get_async(&channel_id).await {
                    #[cfg(debug_assertions)]
                    tracing::info!("Someone joined Channel {channel_id}");
                    *delete = None;
                    return;
                }
            }
            self.check_delete_channels(ctx, old_state).await
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
    async fn check_delete_channel(&self, ctx: &poise::serenity_prelude::Context, channel: serenity::ChannelId) {
        if self.creator_ignore_channels.contains(&channel) {
            tracing::info!("Channel {channel} is ignored");
            return;
        }
        if channel.get() == self.creator_channel.load(std::sync::atomic::Ordering::Acquire) {
            tracing::info!("Channel {channel} is the creator channel");
            return;
        }
        if !self.creator_delete_non_created_channels.load(std::sync::atomic::Ordering::Acquire) && !self.created_channels.contains(&channel) {
            tracing::info!("Channel {channel} is not created by this bot instance and we don't delete non-created channels");
            return;
        }

        #[cfg(debug_assertions)]
        tracing::info!("Channel {channel} might have some need for deletion");
        let instant = match self.creator_mark_delete_channels.get_async(&channel).await {
            None => {
                tracing::warn!("Channel {channel} was already deleted?");
                //Channel was already deleted?
                return;
            }
            Some(mut deletion) => {
                if deletion.is_some() {
                    tracing::info!("Channel {channel} is already marked for deletion");
                    return;
                }
                let instant = tokio::time::Instant::now() + std::time::Duration::from_nanos(self.creator_delete_delay_nanoseconds.load(std::sync::atomic::Ordering::Acquire)) + std::time::Duration::from_millis(50);
                *deletion = Some(instant);
                instant
            },
        };

        tracing::info!("Sleeping for channel {channel} for deletion");
        tokio::time::sleep_until(instant).await;
        {
            let handle = self.creator_mark_delete_channels.get_async(&channel).await;
            let handle = match handle{
                Some(v) => Some(*v),
                None => None
            };
            match handle {
                Some(None) | None => {
                    tracing::info!("Channel {channel} was rejoined");
                    //Channel shouldn't be deleted
                    return;
                }
                Some(Some(deletion)) => {
                    if deletion != instant {
                        tracing::info!("Channel {channel} was rejoined and lefted. Channel Deletion is not our responsibility anymore.");
                        //Channel was rejoined
                        return;
                    }
                },
            };
        }
        let members = match self.created_channels.get_async(&channel).await {
            None => {
                tracing::info!("Channel {channel} was already deleted or shouldn't be deleted.");
                //Channel was already deleted or shouldn't be deleted
                return;
            }
            Some(channel) => {
                if channel.parent_id.map_or_else(||0, serenity::ChannelId::get) != self.create_category.load(std::sync::atomic::Ordering::Acquire) {
                    {
                        let channel = channel.id;
                        tracing::info!("Channel {channel} is not in the correct Category, to be deleted.");
                    }
                    return;
                };

                channel.members(&ctx)
            },
        };

        match members {
            Ok(members) => {
                if members.is_empty() {
                    self.creator_mark_delete_channels.remove_async(&channel).await;
                    if !self.created_channels.remove_async(&channel).await {
                        tracing::info!("Channel {channel} is already deleted? WTF?");
                        //Channel was already deleted or shouldn't be deleted
                        return;
                    }
                    tracing::info!("Channel {channel} Deleted!");
                    match channel.delete(&ctx).await {
                        Ok(_) => {},
                        Err(err) => {
                            self.log_error(&ctx, channel, format!("There was an error deleting the Channel: {err}")).await;
                            tracing::error!("Error deleting channel: {err}");
                            return;
                        }
                    }
                } else {
                    tracing::info!("Channel {channel} is not empty?");
                    match self.creator_mark_delete_channels.get_async(&channel).await {
                        None => {
                            tracing::warn!("Channel {channel} was already deleted?");
                            //Channel was already deleted?
                            return;
                        }
                        Some(mut deletion) => {
                            if let Some(deletion) = *deletion {
                                if deletion != instant {
                                    tracing::warn!("Channel {channel} was rejoined and lefted. Channel Deletion is not our responsibility anymore. How did we get here though?");
                                    //Channel was rejoined
                                    return;
                                }
                            }
                            *deletion = None;
                        },
                    };
                    return;
                }
            },
            Err(err) => {
                self.log_error(&ctx, channel, format!("Error getting members of channel {channel}: {err}")).await;
                return;
            }
        }

        tracing::info!("Actually deleting Channel {channel}.");
    }
    pub(crate) async fn check_delete_channels(&self, ctx: poise::serenity_prelude::Context, old_state: &Option<serenity::all::VoiceState>) {
        let old_channel = old_state.as_ref().map(|old_state| old_state.channel_id).flatten();
        if let Some(old_channel) = old_channel {
            tracing::info!("Checking Channel for deletion: {old_channel}");
            self.check_delete_channel(&ctx, old_channel).await;
        }

        let (mut visited_channels, created_chanels) = match ctx.cache.guild(self.guild_id) {
            None => {
                tracing::error!("Guild not in cache");
                return;
            }
            Some(guild) => {
                let visited_channels = guild.voice_states.values().filter_map(|voice_state|voice_state.channel_id).collect::<std::collections::HashSet<_>>();
                let created_channels = if self.creator_delete_non_created_channels.load(std::sync::atomic::Ordering::Acquire) {
                    let channels = guild.channels.values().filter_map(|channel| {
                        if channel.kind != serenity::model::channel::ChannelType::Voice ||
                            channel.parent_id.map_or_else(||0, serenity::ChannelId::get) != self.create_category.load(std::sync::atomic::Ordering::Acquire) ||
                            old_channel == Some(channel.id)
                        {
                            None
                        } else {
                            Some(channel.id)
                        }
                    }).collect::<HashSet<_>>();
                    Some(channels)
                } else {
                    None
                };
                (visited_channels, created_channels)
            }
        };
        let created_channels = match created_chanels {
            Some(v) => v,
            None => {
                let guard = sdd::Guard::new();
                visited_channels.retain(|channel|self.created_channels.contains(channel));
                self.created_channels.iter(&guard).map(|(k, _)|k).copied().collect::<HashSet<_>>()
            },
        };
        let difference = created_channels.difference(&visited_channels);
        tracing::info!("Checking Channels for deletion: {difference:?}");
        for i in difference {
            self.check_delete_channel(&ctx, *i).await;
        }
    }
    pub(crate) async fn create_channel(&self, ctx: &poise::serenity_prelude::Context, user_id: serenity::UserId) {
        let mut new_channel = serenity::CreateChannel::new("New Channel")
            .kind(serenity::model::channel::ChannelType::Voice)
            .position(u16::try_from(self.creator_ignore_channels.len() + 1).unwrap_or(u16::MAX))
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
            ])
            ;
        match NonZeroU64::new(self.create_category.load(std::sync::atomic::Ordering::Acquire)) {
            None => {},
            Some(category) => {
                new_channel = new_channel.category(category);
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
        match self.guild_id.create_channel(&ctx, new_channel).await {
            Ok(v) => {
                if let Some(error) = error {
                    self.log_error(&ctx, v.id, format!("There was an error getting the Creator's User: {error}")).await;
                }
                match self.guild_id.move_member(&ctx, user_id, v.id).await {
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
                    let _ = self.created_channels.insert_async(channel, v).await;
                    let _ = self.creator_mark_delete_channels.insert_async(channel, None).await;
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