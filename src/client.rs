mod temp_channels;

use std::collections::{HashMap, HashSet};
use poise::{CreateReply, serenity_prelude as serenity};
use serenity::gateway::ShardManager;
use serenity::utils::validate_token;
use serenity::prelude::TypeMapKey;
use serenity::Client;
use std::default::Default;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
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

#[derive(serde::Deserialize)]
#[serde(from="HandlerSerializable")]
struct Handler {
    guild_id: serenity::GuildId,
    creator_channel: serenity::ChannelId,
    create_category: Option<serenity::ChannelId>,
    ignore_channels: HashSet<serenity::ChannelId>,
    delete_non_created_channels: bool,
    created_channels: tokio::sync::RwLock<HashMap<serenity::ChannelId, serenity::GuildChannel>>,
    #[serde(skip)]
    mark_delete_channels: tokio::sync::RwLock<HashMap<serenity::ChannelId, Option<tokio::time::Instant>>>,
    #[serde(skip)]
    incremental_check: tokio::sync::RwLock<Option<(tokio::task::JoinHandle<()>, tokio::sync::oneshot::Sender<()>)>>,
    delete_delay: core::time::Duration,
}
#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct HandlerSerializable {
    guild_id: serenity::GuildId,
    creator_channel: serenity::ChannelId,
    create_category: Option<serenity::ChannelId>,
    ignore_channels: HashSet<serenity::ChannelId>,
    delete_non_created_channels: bool,
    created_channels: HashMap<serenity::ChannelId, serenity::GuildChannel>,
    delete_delay: core::time::Duration,
}
impl From<HandlerSerializable> for Handler {
    fn from(value: HandlerSerializable) -> Self {
        let mark_delete_channels = value.created_channels.keys().map(|k|(*k, None)).collect::<HashMap<_, _>>();
        Self {
            guild_id: value.guild_id,
            creator_channel: value.creator_channel,
            create_category: value.create_category,
            ignore_channels: value.ignore_channels,
            delete_non_created_channels: value.delete_non_created_channels,
            created_channels: tokio::sync::RwLock::from(value.created_channels),
            mark_delete_channels: tokio::sync::RwLock::from(mark_delete_channels),
            incremental_check: Default::default(),
            delete_delay: value.delete_delay,
        }
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
}
#[derive(Clone, Default)]
struct HandlerWrapper(Arc<Handler>);
impl Deref for HandlerWrapper {
    type Target = Arc<Handler>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for HandlerWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl serenity::client::EventHandler for HandlerWrapper {
    fn cache_ready<'life0, 'async_trait>(&'life0 self, ctx: poise::serenity_prelude::Context, _: Vec<serenity::GuildId>)
    -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send + 'async_trait>>
    where Self: 'async_trait, 'life0: 'async_trait
    {
        Box::pin(async move {
            self.check_delete_channels(&ctx, None).await;
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
                    } else {
                        //This needs to be in a separate scope, to make sure, that the write guard is getting dropped before the check_delete_channels call
                        {
                            let mut guard = self.mark_delete_channels.write().await;
                            if let Some(delete) = guard.get_mut(&channel) {
                                #[cfg(debug_assertions)]
                                tracing::info!("Someone joined Channel {channel}");
                                *delete = None;
                                return;
                            }
                        }
                        self.check_delete_channels(&ctx, _old_state).await
                    }
                },
                None => self.check_delete_channels(&ctx, _old_state).await,
            }
        })
    }

    fn shard_stage_update<'life0, 'async_trait>(&'life0 self, ctx: poise::serenity_prelude::Context, event: serenity::gateway::ShardStageUpdateEvent)
                                                -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send + 'async_trait>>
    where Self: 'async_trait, 'life0: 'async_trait
    {
        Box::pin(async move {
            match event.new {
                serenity::gateway::ConnectionStage::Connected => {
                    let slf = self.clone();
                    let ctx_clone = ctx.clone();
                    let (sender, mut receiver) = tokio::sync::oneshot::channel();
                    let task = tokio::spawn(async move {
                        //Interval is every 15 minutes
                        let mut inc = tokio::time::interval(core::time::Duration::from_secs(15*60));
                        inc.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                        loop {
                            tokio::select!(
                                biased;
                                _ = &mut receiver => {
                                    return;
                                }
                                _ = inc.tick() => {
                                    tracing::info!("Incremental Check Start");
                                    slf.check_delete_channels(&ctx_clone, None).await;
                                    tracing::info!("Incremental Check Done");
                                },
                            )
                        }
                    });
                    Handler::end_incremental_check_inner(self.incremental_check.write().await.replace((task, sender))).await;
                }
                serenity::gateway::ConnectionStage::Disconnected => self.end_incremental_check().await,
                _ => {

                }
            }
       })
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

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Cache{
    handler: HandlerSerializable,
    #[serde(skip)]
    handler_arc: HandlerWrapper,
}

impl Cache {
    async fn save(&mut self) {
        tracing::info!("Serializing handler");
        self.handler = self.handler_arc.to_serializable().await;
        tracing::info!("Serialized handler");
        let serialized = match serde_json::to_vec(&self) {
            Err(v) => {
                tracing::error!("Error serializing cache: {v}");
                return;
            }
            Ok(v) => v,
        };
        match tokio::fs::write(CACHE_PATH, serialized).await {
            Ok(_) => {
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

    tracing::info!("Got Cached Creation Channels: {:?}", cache.handler.created_channels);
    cache.handler_arc = HandlerWrapper(Arc::new(cache.handler.clone().into()));
    let handler = cache.handler_arc.clone();

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
        .event_handler(handler.clone())
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