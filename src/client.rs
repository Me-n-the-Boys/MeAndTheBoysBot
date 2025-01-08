mod temp_channels;
mod xp;
mod commands;

use std::collections::{HashMap, HashSet};
use poise::serenity_prelude as serenity;
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

/// User data, which is stored and accessible in all command invocations
struct Data {
    handler: Arc<Handler>,
}

#[derive(serde::Deserialize)]
#[serde(from="HandlerSerializable")]
struct Handler {
    guild_id: serenity::GuildId,
    creator_channel: serenity::ChannelId,
    create_category: Option<serenity::ChannelId>,
    creator_ignore_channels: HashSet<serenity::ChannelId>,
    creator_delete_non_created_channels: bool,
    created_channels: tokio::sync::RwLock<HashMap<serenity::ChannelId, serenity::GuildChannel>>,
    #[serde(skip)]
    creator_mark_delete_channels: tokio::sync::RwLock<HashMap<serenity::ChannelId, Option<tokio::time::Instant>>>,
    creator_delete_delay: core::time::Duration,
    #[serde(skip)]
    incremental_check: tokio::sync::RwLock<Option<(tokio::task::JoinHandle<()>, tokio::sync::oneshot::Sender<()>)>>,
    xp_ignored_channels: HashSet<serenity::ChannelId>,
    #[serde(skip)]
    xp_vc_tmp: tokio::sync::Mutex<HashMap<serenity::UserId, serenity::Timestamp>>,
    xp_vc: tokio::sync::Mutex<HashMap<serenity::UserId, u64>>,
    xp_txt: tokio::sync::Mutex<HashMap<serenity::UserId, xp::TextXpInfo>>,
    xp_txt_apply_milliseconds: u32,
    xp_txt_punish_seconds: u64,
}
#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct HandlerSerializable {
    guild_id: serenity::GuildId,
    creator_channel: serenity::ChannelId,
    create_category: Option<serenity::ChannelId>,
    creator_ignore_channels: HashSet<serenity::ChannelId>,
    creator_delete_non_created_channels: bool,
    created_channels: HashMap<serenity::ChannelId, serenity::GuildChannel>,
    delete_delay: core::time::Duration,
    xp_ignored_channels: HashSet<serenity::ChannelId>,
    xp_vc: HashMap<serenity::UserId, u64>,
    xp_txt: HashMap<serenity::UserId, xp::TextXpInfo>,
    xp_txt_apply_milliseconds: u32,
    xp_txt_punish_seconds: u64,
}
impl From<HandlerSerializable> for Handler {
    fn from(value: HandlerSerializable) -> Self {
        let mark_delete_channels = value.created_channels.keys().map(|k|(*k, None)).collect::<HashMap<_, _>>();
        Self {
            guild_id: value.guild_id,
            creator_channel: value.creator_channel,
            create_category: value.create_category,
            creator_ignore_channels: value.creator_ignore_channels,
            creator_delete_non_created_channels: value.creator_delete_non_created_channels,
            created_channels: tokio::sync::RwLock::from(value.created_channels),
            creator_mark_delete_channels: tokio::sync::RwLock::from(mark_delete_channels),
            creator_delete_delay: value.delete_delay,
            incremental_check: Default::default(),
            xp_ignored_channels: value.xp_ignored_channels,
            xp_vc_tmp: Default::default(),
            xp_vc: tokio::sync::Mutex::new(value.xp_vc),
            xp_txt: tokio::sync::Mutex::new(value.xp_txt),
            xp_txt_apply_milliseconds: value.xp_txt_apply_milliseconds,
            xp_txt_punish_seconds: value.xp_txt_punish_seconds,
        }
    }
}
impl Handler {
    async fn to_serializable(&self) -> HandlerSerializable {
        HandlerSerializable {
            guild_id: self.guild_id,
            creator_channel: self.creator_channel,
            create_category: self.create_category,
            creator_ignore_channels: self.creator_ignore_channels.clone(),
            creator_delete_non_created_channels: self.creator_delete_non_created_channels,
            created_channels: self.created_channels.read().await.clone(),
            xp_ignored_channels: self.xp_ignored_channels.clone(),
            xp_vc: self.xp_vc.lock().await.clone(),
            xp_txt: self.xp_txt.lock().await.clone(),
            delete_delay: self.creator_delete_delay,
            xp_txt_apply_milliseconds: self.xp_txt_apply_milliseconds,
            xp_txt_punish_seconds: self.xp_txt_punish_seconds,
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
    fn cache_ready<'life0, 'async_trait>(&'life0 self, ctx: poise::serenity_prelude::Context, guilds: Vec<serenity::GuildId>)
    -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send + 'async_trait>>
    where Self: 'async_trait, 'life0: 'async_trait
    {
        Box::pin(async move {
            self.check_delete_channels(&ctx, None).await;
            //Populate all the voice states on startup
            if let Some(voice_states) = ctx.cache.guild(self.guild_id)
                .map(|v|v.voice_states.iter()
                    .filter(|(_,s)|s.guild_id == Some(self.guild_id) && s.channel_id.is_some())
                    .map(|(u,s)|(*u, s.clone()))
                    .collect::<Vec<_>>()) {
                for (user, state) in voice_states {
                    match state.channel_id {
                        Some(channel) => {
                            self.vc_join_xp(channel, user).await;
                        },
                        None => {}
                    }
                }
                tracing::warn!("Applied voice states for guild {}", self.guild_id);
            } else {
                tracing::warn!("Guild {} not found in cache. Cannot prefill voice states.", self.guild_id);
            }
        })
    }
    fn voice_state_update<'life0, 'async_trait>(&'life0 self, ctx: poise::serenity_prelude::Context, old_state: std::option::Option<serenity::all::VoiceState>, new_state: serenity::all::VoiceState)
    -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send + 'async_trait>>
    where Self: 'async_trait, 'life0: 'async_trait
    {
        Box::pin(async move {
            if new_state.guild_id != Some(self.guild_id) {
                return;
            }
            match new_state.channel_id {
                Some(channel) => {
                    tokio::join!(
                        self.vc_join_xp(channel, new_state.user_id),
                        self.vc_join_channel_temp_channel(&ctx, new_state.user_id, channel, old_state),
                    );
                },
                None => {
                    tokio::join!(
                        self.try_apply_vc_xp(new_state.user_id),
                        self.check_delete_channels(&ctx, old_state),
                    );
                },
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
                                    tracing::info!("Incremental Check: Temp Channel Check Done");
                                    slf.xp_incremental_check().await;
                                    //periodically apply text/voice xp here
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

    fn message<'life0, 'async_trait>(&'life0 self, _: poise::serenity_prelude::Context, message: serenity::model::channel::Message)
                                                -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send + 'async_trait>>
    where Self: 'async_trait, 'life0: 'async_trait
    {
        Box::pin(async move {
            self.message_xp(message).await;
        })
    }
}
impl Default for HandlerSerializable {
    fn default() -> Self {
        Self {
            guild_id: serenity::GuildId::new(695718765119275109),
            creator_channel: serenity::ChannelId::new(1241371878761824356),
            create_category: Some(serenity::ChannelId::new(966080144747933757)),
            creator_delete_non_created_channels: false,
            creator_ignore_channels: HashSet::from([serenity::ChannelId::new(695720756189069313)]),
            created_channels: Default::default(),
            xp_ignored_channels: HashSet::from([serenity::ChannelId::new(695720756189069313)]),
            xp_vc: Default::default(),
            xp_txt: Default::default(),
            delete_delay: core::time::Duration::from_secs(15),
            xp_txt_apply_milliseconds: 50,
            xp_txt_punish_seconds: 60*2,
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
    let handler_1 = handler.0.clone();
    
    
    let framework = poise::framework::FrameworkBuilder::default()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::ping(),
                commands::debug(),
            ],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    handler: handler_1,
                })
            })
        })
        .initialize_owners(true)
        .build();

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