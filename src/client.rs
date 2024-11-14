use std::collections::HashSet;
use poise::{CreateReply, serenity_prelude as serenity};
use serenity::gateway::ShardManager;
use serenity::utils::{validate_token};
use serenity::prelude::TypeMapKey;
use serenity::Client;
use std::default::Default;
use std::ops::Add;
use std::sync::Arc;
use std::time::Duration;
use serenity::all::GatewayIntents;

pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<ShardManager>;
}

struct Data {} // User data, which is stored and accessible in all command invocations
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

///Gets the latency of the current shard to discord's gateway.
#[poise::command(
    slash_command,
)]
async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    const CONVERSION_STEP:u32 = 1000;
    let time = ctx.ping().await;
    let ns = time.subsec_nanos();
    //get submicro_nanos
    let us = ns/CONVERSION_STEP;
    let ns = ns - us *CONVERSION_STEP;
    //get submilli_micros
    let ms = us /CONVERSION_STEP;
    let us = us - ms*CONVERSION_STEP;
    //get subsec_milli
    ctx.send(CreateReply::default().content(format!("Pong! Shard {}'s Latency to Gateway: {}s{ms}ms{us}µs{ns}ns", ctx.serenity_context().shard_id, time.as_secs(), )).ephemeral(true).reply(true)).await?;
    Ok(())
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct Handler {
    guild_id: serenity::GuildId,
    creator_channel: serenity::ChannelId,
    create_category: Option<serenity::ChannelId>,
    ignore_channels: HashSet<serenity::ChannelId>,
    delete_non_created_channels: bool,
    created_channels: dashmap::DashMap<serenity::ChannelId, serenity::GuildChannel>,
    #[serde(skip)]
    mark_delete_channels: dashmap::DashMap<serenity::ChannelId, Option<tokio::time::Instant>>,
    delete_delay: core::time::Duration,
}

impl serenity::client::EventHandler for Handler {
    fn cache_ready<'life0, 'async_trait>(&'life0 self, ctx: poise::serenity_prelude::Context, _: Vec<serenity::GuildId>)
    -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send + 'async_trait>>
    where Self: 'async_trait, 'life0: 'async_trait
    {
        Box::pin(async move {
            self.check_delete_channels(ctx, None).await;
        })
    }
    fn voice_state_update<'life0, 'async_trait>(&'life0 self, ctx: poise::serenity_prelude::Context, _old_state: std::option::Option<serenity::all::VoiceState>, new_state: serenity::all::VoiceState)
    -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send + 'async_trait>>
    where Self: 'async_trait, 'life0: 'async_trait
    {
        Box::pin(async move {
            if new_state.guild_id != Some(self.guild_id) {
                return;
            }
            match new_state.channel_id {
                Some(channel) => {
                    if new_state.channel_id == Some(self.creator_channel) {
                        self.create_channel(ctx, new_state.user_id).await;
                        return;
                    } else if let Some(mut delete) = self.mark_delete_channels.get_mut(&channel) {
                        #[cfg(debug_assertions)]
                        tracing::info!("Someone joined Channel {channel}");
                        *delete.value_mut() = None;
                        return;
                    } else {
                        self.check_delete_channels(ctx, _old_state).await
                    }
                },
                None => self.check_delete_channels(ctx, _old_state).await,
            }
        })
    }
}
impl Default for Handler {
    fn default() -> Self {
        Self {
            guild_id: serenity::GuildId::new(695718765119275109),
            creator_channel: serenity::ChannelId::new(1241371878761824356),
            create_category: Some(serenity::ChannelId::new(966080144747933757)),
            delete_non_created_channels: false,
            ignore_channels: HashSet::from([serenity::ChannelId::new(695720756189069313)]),
            created_channels: Default::default(),
            mark_delete_channels: Default::default(),
            delete_delay: core::time::Duration::from_secs(15),
        }
    }
}
impl Handler {
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
            #[cfg(debug_assertions)]
            tracing::info!("Channel {channel} is ignored");
            return;
        }
        if channel == self.creator_channel {
            #[cfg(debug_assertions)]
            tracing::info!("Channel {channel} is the creator channel");
            return;
        }
        if !self.delete_non_created_channels && !self.created_channels.contains_key(&channel) {
            #[cfg(debug_assertions)]
            tracing::info!("Channel {channel} is not created by this bot instance and we don't delete non-created channels");
            return;
        }

        #[cfg(debug_assertions)]
        tracing::info!("Channel {channel} might have some need for deletion");
        let instant = match self.mark_delete_channels.get_mut(&channel) {
            None => {
                #[cfg(debug_assertions)]
                tracing::info!("Channel {channel} was already deleted?");
                //Channel was already deleted?
                return;
            }
            Some(mut deletion) => {
                if deletion.is_some() {
                    #[cfg(debug_assertions)]
                    tracing::info!("Channel {channel} is already marked for deletion");
                    return;
                }
                let instant = tokio::time::Instant::now().add(self.delete_delay).add(Duration::from_millis(50));
                *deletion = Some(instant);
                instant
            },
        };

        #[cfg(debug_assertions)]
        tracing::info!("Sleeping for channel {channel} for deletion");
        tokio::time::sleep_until(instant).await;
        match self.mark_delete_channels.get(&channel).map(|r|r.value().clone()) {
            Some(None) | None => {
                #[cfg(debug_assertions)]
                tracing::info!("Channel {channel} was rejoined");
                //Channel shouldn't be deleted
                return;
            }
            Some(Some(deletion)) => {
                if deletion != instant {
                    #[cfg(debug_assertions)]
                    tracing::info!("Channel {channel} was rejoined and lefted. Channel Deletion is not our responsibility anymore.");
                    //Channel was rejoined
                    return;
                }
                #[cfg(debug_assertions)]
                tracing::info!("Actually deleting Channel {channel}.");
            },
        };
        let channel = match self.created_channels.get(&channel) {
            None => {
                #[cfg(debug_assertions)]
                tracing::info!("Channel {channel} was already deleted or shouldn't be deleted.");
                //Channel was already deleted or shouldn't be deleted
                return;
            }
            Some(channel) => channel,
        };

        if channel.parent_id != self.create_category {
            #[cfg(debug_assertions)]
            {
                let channel = channel.id;
                tracing::info!("Channel {channel} is not in the correct Category, to be deleted.");
            }
            return;
        }

        match channel.members(&ctx) {
            Ok(members) => {
                if members.is_empty() {
                    let channel = {
                        let channel_id = channel.id;
                        drop(channel);
                        channel_id
                    };
                    #[cfg(debug_assertions)]
                    tracing::info!("Channel {channel} is empty.");
                    let channel = {
                        self.mark_delete_channels.remove(&channel);
                        match self.created_channels.remove(&channel) {
                            None => {
                                #[cfg(debug_assertions)]
                                tracing::info!("Channel {channel} is already deleted? WTF?");
                                //Channel was already deleted or shouldn't be deleted
                                return;
                            }
                            Some(channel) => channel,
                        }.1
                    };
                    #[cfg(debug_assertions)]
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
                    let channel = channel.id;
                    #[cfg(debug_assertions)]
                    tracing::info!("Channel {channel} is not empty?");
                }
            },
            Err(err) => {
                let channel = channel.id;
                self.log_error(&ctx, channel, format!("Error getting members of channel {channel}: {err}")).await;
                return;
            }
        }
    }
    async fn check_delete_channels(&self, ctx: poise::serenity_prelude::Context, old_state: std::option::Option<serenity::all::VoiceState>) {
        let old_channel = old_state.as_ref().map(|old_state| old_state.channel_id).flatten();
        if let Some(old_channel) = old_channel {
            #[cfg(debug_assertions)]
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
            None => self.mark_delete_channels.iter().map(|r|*r.key()).collect::<HashSet<_>>(),
        };
        let difference = created_channels.difference(&visited_channels);
        #[cfg(debug_assertions)]
        tracing::info!("Checking Channels for deletion: {difference:?}");
        for i in difference {
            self.check_delete_channel(&ctx, *i).await;
        }
    }
    async fn create_channel(&self, ctx: poise::serenity_prelude::Context, user_id: serenity::UserId) {
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
                #[cfg(debug_assertions)]
                tracing::info!("Created channel {channel}, but did not insert yet");
                self.created_channels.insert(channel, v);
                self.mark_delete_channels.insert(channel, None);
                #[cfg(debug_assertions)]
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
#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Cache{
    handler: Arc<Handler>,
}

impl Cache {
    async fn save(&mut self) {
        let serialized = match serde_json::to_vec(&self) {
            Err(v) => {
                tracing::error!("Error serializing cache: {v}");
                return;
            }
            Ok(v) => v,
        };
        match tokio::fs::write(CACHE_PATH, serialized).await {
            Ok(_) => {
                #[cfg(debug_assertions)]
                tracing::info!("Cache saved");
            },
            Err(err) => {
                tracing::error!("Error saving cache: {err}");
            }
        }
    }
}
const CACHE_PATH: &str = "cache.json";
pub async fn init_client() -> Client {
    tracing::debug!("Getting Client Token");
    let token = std::env::var("DISCORD_TOKEN").expect("No Token. Unable to Start Bot!");
    assert!(validate_token(&token).is_ok(), "Invalid discord token!");

    let framework = poise::framework::FrameworkBuilder::default()
        .options(poise::FrameworkOptions {
            commands: vec![ping()],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {})
            })
        })
        .initialize_owners(true)
        .build();

    let mut cache =
        tokio::fs::read(CACHE_PATH).await
            .map(|v|
                serde_json::from_slice::<Cache>(v.as_slice())
                    .inspect_err(|err|tracing::error!("Error deserializing cache. Falling back to Defaults. Error: {err}"))
                    .unwrap_or_default()
            ).inspect_err(|err|tracing::info!("Error reading cache. Falling back to Defaults. Error: {err}"))
            .unwrap_or_default();

    let handler = Arc::make_mut(&mut cache.handler);
    handler.mark_delete_channels = dashmap::DashMap::from_iter(handler.created_channels.iter().map(|r|(*r.key(), None)));
    tracing::info!("Got Cached Creation Channels: {:?}", cache.handler.created_channels);
    let handler = cache.handler.clone();

    let (sender, saver) = {
        let (sender, mut receiver) = tokio::sync::oneshot::channel::<()>();
        let mut interval = tokio::time::interval(core::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        (sender, tokio::spawn(async move{
            let mut cache = cache;
            loop {
                tokio::select!(
                    biased;
                    _ = &mut receiver => return cache,
                    _ = interval.tick() => cache.save().await,
                )
            }
        }))
    };

    let mut client = serenity::Client::builder(&token, GatewayIntents::default())
        .framework(framework)
        .event_handler_arc(handler.clone())
        .await
        .expect("serenity failed sonehow!");

    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(client.shard_manager.clone());
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        let _ = sender.send(());
        tokio::join!(
            async {
                match saver.await {
                    Ok(mut v) => v.save().await,
                    Err(err) => {
                        tracing::error!("Cache saver Thread Panicked: {err}");
                    }
                }
            },
            shard_manager.shutdown_all()
        );
    });

    if let Err(why) = client.start_autosharded().await {
        tracing::error!("Client error: {:?}", why);
    }

    client
}