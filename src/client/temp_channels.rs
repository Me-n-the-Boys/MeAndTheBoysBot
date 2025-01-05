use std::collections::HashSet;
use std::sync::Arc;
use poise::serenity_prelude as serenity;

impl super::Handler {
    pub(crate) async fn end_incremental_check(&self) {
        Self::end_incremental_check_inner({
            self.incremental_check.write().await.take()
        }).await
    }
    pub(crate) async fn end_incremental_check_inner(task: Option<(tokio::task::JoinHandle<()>, tokio::sync::oneshot::Sender<()>)>) {
        match task {
            Some((handle, sender)) => {
                match sender.send(()){
                    Ok(()) => {},
                    Err(()) => {
                        tracing::error!("Error sending termination signal to incremental check thread");
                    }
                };
                match handle.await {
                    Ok(()) => {}
                    Err(err) => {
                        tracing::error!("Incremental check thread panicked: {err}");
                    }
                }
            }
            None => {}
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
        if self.ignore_channels.contains(&channel) {
            tracing::info!("Channel {channel} is ignored");
            return;
        }
        if channel == self.creator_channel {
            tracing::info!("Channel {channel} is the creator channel");
            return;
        }
        if !self.delete_non_created_channels && !self.created_channels.read().await.contains_key(&channel) {
            tracing::info!("Channel {channel} is not created by this bot instance and we don't delete non-created channels");
            return;
        }

        #[cfg(debug_assertions)]
        tracing::info!("Channel {channel} might have some need for deletion");
        let instant = match self.mark_delete_channels.write().await.get_mut(&channel) {
            None => {
                tracing::warn!("Channel {channel} was already deleted?");
                //Channel was already deleted?
                return;
            }
            Some(deletion) => {
                if deletion.is_some() {
                    tracing::info!("Channel {channel} is already marked for deletion");
                    return;
                }
                let instant = tokio::time::Instant::now() + self.delete_delay + std::time::Duration::from_millis(50);
                *deletion = Some(instant);
                instant
            },
        };

        tracing::info!("Sleeping for channel {channel} for deletion");
        tokio::time::sleep_until(instant).await;
        match self.mark_delete_channels.read().await.get(&channel) {
            Some(None) | None => {
                tracing::info!("Channel {channel} was rejoined");
                //Channel shouldn't be deleted
                return;
            }
            Some(Some(deletion)) => {
                if *deletion != instant {
                    tracing::info!("Channel {channel} was rejoined and lefted. Channel Deletion is not our responsibility anymore.");
                    //Channel was rejoined
                    return;
                }
            },
        };
        let members = match self.created_channels.read().await.get(&channel) {
            None => {
                tracing::info!("Channel {channel} was already deleted or shouldn't be deleted.");
                //Channel was already deleted or shouldn't be deleted
                return;
            }
            Some(channel) => {
                if channel.parent_id != self.create_category {
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
                    let channel = {
                        let (mut created_channels, mut mark_delete_channels) = tokio::join!(self.created_channels.write(), self.mark_delete_channels.write());
                        mark_delete_channels.remove(&channel);
                        match created_channels.remove(&channel) {
                            None => {
                                tracing::info!("Channel {channel} is already deleted? WTF?");
                                //Channel was already deleted or shouldn't be deleted
                                return;
                            }
                            Some(channel) => channel,
                        }
                    };
                    tracing::info!("Channel {channel} Deleted!");
                    match channel.delete(&ctx).await {
                        Ok(_) => {},
                        Err(err) => {
                            self.log_error(&ctx, channel.id, format!("There was an error deleting the Channel: {err}")).await;
                            tracing::error!("Error deleting channel: {err}");
                            return;
                        }
                    }
                } else {
                    tracing::info!("Channel {channel} is not empty?");
                    match { self.mark_delete_channels.write().await.get_mut(&channel) } {
                        None => {
                            tracing::warn!("Channel {channel} was already deleted?");
                            //Channel was already deleted?
                            return;
                        }
                        Some(deletion) => {
                            if let Some(deletion) = deletion {
                                if *deletion != instant {
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
    pub(crate) async fn check_delete_channels(self: &Arc<Self>, ctx: &poise::serenity_prelude::Context, old_state: std::option::Option<serenity::all::VoiceState>) {
        let old_channel = old_state.as_ref().map(|old_state| old_state.channel_id).flatten();
        if let Some(old_channel) = old_channel {
            tracing::info!("Checking Channel for deletion: {old_channel}");
            self.check_delete_channel(&ctx, old_channel).await;
        }

        let (visited_channels, created_chanels) = match ctx.cache.guild(self.guild_id) {
            None => {
                tracing::error!("Guild not in cache");
                return;
            }
            Some(guild) => {
                let visited_channels = guild.voice_states.values().filter_map(|voice_state|voice_state.channel_id).collect::<std::collections::HashSet<_>>();
                let created_channels = if self.delete_non_created_channels {
                    let channels = guild.channels.values().filter_map(|channel| {
                        if channel.kind != serenity::model::channel::ChannelType::Voice ||
                            channel.parent_id != self.create_category ||
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
                self.created_channels.read().await.keys().copied().collect::<HashSet<_>>()
            },
        };
        let difference = created_channels.difference(&visited_channels);
        tracing::info!("Checking Channels for deletion: {difference:?}");
        for i in difference {
            self.check_delete_channel(&ctx, *i).await;
        }
    }
    pub(crate) async fn create_channel(&self, ctx: poise::serenity_prelude::Context, user_id: serenity::UserId) {
        let mut new_channel = serenity::CreateChannel::new("New Channel")
            .kind(serenity::model::channel::ChannelType::Voice)
            .position(u16::try_from(self.ignore_channels.len() + 1).unwrap_or(u16::MAX))
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
        if let Some(category) = self.create_category {
            new_channel = new_channel.category(category);
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
                    let (mut created_channels, mut mark_delete_channels) = tokio::join!(self.created_channels.write(), self.mark_delete_channels.write());
                    created_channels.insert(channel, v);
                    mark_delete_channels.insert(channel, None);
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