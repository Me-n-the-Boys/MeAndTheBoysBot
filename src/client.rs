use std::collections::{HashMap, HashSet};
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

struct Handler {
    guild_id: serenity::GuildId,
    creator_channel: serenity::ChannelId,
    create_category: Option<serenity::ChannelId>,
    ignore_channels: HashSet<serenity::ChannelId>,
    delete_non_created_channels: bool,
    created_channels: tokio::sync::RwLock<std::collections::HashMap<serenity::ChannelId, serenity::GuildChannel>>,
    mark_delete_channels: tokio::sync::RwLock<std::collections::HashMap<serenity::ChannelId, Option<tokio::time::Instant>>>,
    delete_delay: core::time::Duration,
}
#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct HandlerSerializable {
    guild_id: serenity::GuildId,
    creator_channel: serenity::ChannelId,
    create_category: Option<serenity::ChannelId>,
    ignore_channels: HashSet<serenity::ChannelId>,
    delete_non_created_channels: bool,
    created_channels: std::collections::HashMap<serenity::ChannelId, serenity::GuildChannel>,
    delete_delay: core::time::Duration,
}

impl serenity::client::EventHandler for Handler {
    fn ready<'life0, 'async_trait>(&'life0 self, ctx: poise::serenity_prelude::Context, _: serenity::model::gateway::Ready)
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
            //Prevent deleting AFK channel
            if let Some(channel) = new_state.channel_id {
                if self.ignore_channels.contains(&channel) {
                    return;
                }
            }
            match new_state.channel_id {
                Some(channel) => {
                    if new_state.channel_id == Some(self.creator_channel) {
                        self.create_channel(ctx, new_state.user_id).await;
                        return;
                    } else if let Some(delete) = self.mark_delete_channels.write().await.get_mut(&channel) {
                        #[cfg(debug_assertions)]
                        tracing::info!("Someone joined Channel {channel}");
                        *delete = None;
                        return;
                    }
                },
                None => self.check_delete_channels(ctx, _old_state).await,
            }
        })
    }
}
impl From<HandlerSerializable> for Handler {
    fn from(value: HandlerSerializable) -> Self {
        let HandlerSerializable{
            guild_id,
            creator_channel,
            create_category,
            ignore_channels,
            delete_non_created_channels,
            created_channels,
            delete_delay
        } = value;
        let mark_delete_channels = HashMap::from_iter(created_channels.keys().map(|k|(k.clone(), None)));
        Self{
            guild_id,
            creator_channel,
            create_category,
            ignore_channels,
            delete_non_created_channels,
            created_channels: tokio::sync::RwLock::new(created_channels),
            mark_delete_channels: tokio::sync::RwLock::new(mark_delete_channels),
            delete_delay
        }
    }
}
impl Default for HandlerSerializable {
    fn default() -> Self {
        Self {
            guild_id: serenity::GuildId::new(695718765119275109),
            creator_channel: serenity::ChannelId::new(1241371878761824356),
            create_category: Some(serenity::ChannelId::new(966080144747933757)),
            delete_non_created_channels: false,
            ignore_channels: HashSet::from([serenity::ChannelId::new(695720756189069313)]),
            created_channels: Default::default(),
            delete_delay: core::time::Duration::from_secs(15),
        }
    }
}
impl Default for Handler {
    fn default() -> Self {
        Self::from(HandlerSerializable::default())
    }
}
impl Handler {
    async fn to_serializable(&self) -> HandlerSerializable {
        HandlerSerializable {
            guild_id: self.guild_id,
            creator_channel: self.creator_channel,
            create_category: self.create_category,
            ignore_channels: self.ignore_channels.clone(),
            delete_non_created_channels: self.delete_non_created_channels,
            created_channels: self.created_channels.read().await.clone(),
            delete_delay: self.delete_delay,
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
        if
            self.ignore_channels.contains(&channel) || //Channel is ignored
            channel == self.creator_channel || //Channel is the creator channel
            (!self.delete_non_created_channels && !self.created_channels.read().await.contains_key(&channel)) //Channel is not created by this bot instance and we don't delete non-created channels
        { return; }

        #[cfg(debug_assertions)]
        tracing::info!("Checking channel {channel} for deletion");
        let instant = match self.mark_delete_channels.write().await.get_mut(&channel) {
            None => {
                //Channel was already deleted?
                return;
            }
            Some(deletion) => {
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
        match self.mark_delete_channels.read().await.get(&channel) {
            Some(None) | None => {
                #[cfg(debug_assertions)]
                tracing::info!("Channel {channel} was rejoined");
                //Channel shouldn't be deleted
                return;
            }
            Some(Some(deletion)) => {
                if *deletion != instant {
                    #[cfg(debug_assertions)]
                    tracing::info!("Channel {channel} was rejoined and lefted. Channel Deletion is not our responsibility anymore.");
                    //Channel was rejoined
                    return;
                }
                #[cfg(debug_assertions)]
                tracing::info!("Actually deleting Channel {channel}.");
            },
        };
        let channel_lock = self.created_channels.read().await;
        let channel = match channel_lock.get(&channel) {
            None => {
                //Channel was already deleted or shouldn't be deleted
                return;
            }
            Some(channel) => channel,
        };

        match channel.members(&ctx) {
            Ok(members) => {
                if members.is_empty() {
                    #[cfg(debug_assertions)]
                    tracing::info!("Channel {channel} is empty.");
                    let channel_id = channel.id;
                    drop(channel_lock);
                    let channel = match self.created_channels.write().await.remove(&channel_id) {
                        None => {
                            #[cfg(debug_assertions)]
                            tracing::info!("Channel {channel_id} is already deleted? WTF?");
                            //Channel was already deleted or shouldn't be deleted
                            return;
                        }
                        Some(channel) => channel,
                    };
                    self.mark_delete_channels.write().await.remove(&channel_id);
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
                    #[cfg(debug_assertions)]
                    tracing::info!("Channel {channel} is not empty?");
                }
            },
            Err(err) => {
                self.log_error(&ctx, channel.id, format!("Error getting members of channel: {err}")).await;
                return;
            }
        }
    }
    async fn check_delete_channels(&self, ctx: poise::serenity_prelude::Context, _old_state: std::option::Option<serenity::all::VoiceState>) {
        if let Some(old_state) = _old_state {
            if let Some(old_channel) = old_state.channel_id {
                self.check_delete_channel(&ctx, old_channel).await;
            }
        }

        if let Some(category) = self.create_category {
            match self.guild_id.channels(&ctx).await {
                Ok(v) => {
                    for channel in v.values().filter(|channel|channel.parent_id == Some(category)).map(|channel|channel.id).collect::<Vec<_>>() {
                        self.check_delete_channel(&ctx, channel).await;
                    }
                }
                Err(v) => {
                    tracing::error!("Error getting channels: {v}");
                    return;
                }
            }
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
                self.mark_delete_channels.write().await.insert(v.id, None);
                self.created_channels.write().await.insert(v.id, v);
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
    handler: HandlerSerializable,
    #[serde(skip)]
    handler_arc: Arc<Handler>,
}

impl Cache {
    async fn save(&mut self) {
        self.handler = self.handler_arc.to_serializable().await;
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

    let mut cache = match tokio::fs::read(CACHE_PATH).await {
        Ok(v) => match serde_json::from_slice(v.as_slice()) {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("Error deserializing cache. Falling back to Defaults. Error: {err}");
                Cache::default()
            }
        },
        Err(err) => {
            tracing::info!("Error reading cache. Falling back to Defaults. Error: {err}");
            Cache::default()
        }
    };
    tracing::info!("Got Cached Creation Channels: {:?}", cache.handler.created_channels.keys());
    cache.handler_arc = Arc::new(Handler::from(cache.handler.clone()));
    let handler = cache.handler_arc.clone();

    let (sender, saver) = {
        let (sender, mut receiver) = tokio::sync::oneshot::channel::<()>();
        let mut interval = tokio::time::interval(core::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        (sender, tokio::spawn(async move{
            let mut cache = cache;
            loop {
                match receiver.try_recv() {
                    Ok(()) | Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                        return cache;
                    }
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {},
                }
                interval.tick().await;
                cache.save().await;
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