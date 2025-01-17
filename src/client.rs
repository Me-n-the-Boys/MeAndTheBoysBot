mod temp_channels;
mod xp;
mod commands;

use poise::serenity_prelude as serenity;
use serenity::utils::validate_token;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use serenity::all::GatewayIntents;

/// User data, which is stored and accessible in all command invocations
struct Data {
    handler: Arc<OuterHandler>,
    auth: Arc<crate::rocket::auth::Auth>,
}
#[derive(Debug, serde::Deserialize, serde::Serialize, Default)]
struct OuterHandler{
    guilds: scc::HashIndex<serenity::GuildId, Handler>
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct Handler {
    guild_id: serenity::GuildId,
    creator_channel: std::sync::atomic::AtomicU64,                                                                      //ChannelId
    create_category: std::sync::atomic::AtomicU64,                                                                      //ChannelId
    creator_ignore_channels: scc::HashIndex<serenity::ChannelId, ()>,
    creator_delete_non_created_channels: std::sync::atomic::AtomicBool,
    created_channels: scc::HashIndex<serenity::ChannelId, serenity::GuildChannel>,
    #[serde(skip)]
    creator_mark_delete_channels: scc::HashMap<serenity::ChannelId, Option<tokio::time::Instant>>,
    //2^64 nanoseconds is 584.5421 years, which is more than enough of a deletion delay.
    creator_delete_delay_nanoseconds: std::sync::atomic::AtomicU64,                                                     //Nanoseconds
    xp_ignored_channels: scc::HashIndex<serenity::ChannelId, ()>,
    #[serde(skip)]
    xp_vc_tmp: scc::HashMap<serenity::UserId, serenity::Timestamp>,
    xp_vc: scc::HashMap<serenity::UserId, u64>,
    xp_txt: scc::HashMap<serenity::UserId, xp::TextXpInfo>,
    xp_txt_apply_milliseconds: std::sync::atomic::AtomicU32,                                                            //Milliseconds
    xp_txt_punish_seconds: std::sync::atomic::AtomicU64,                                                                //Seconds
    twitch_live_notification: Option<TwitchLiveNotifications>,
}

impl Clone for Handler {
    fn clone(&self) -> Self {
        Self{
            guild_id: self.guild_id,
            creator_channel: std::sync::atomic::AtomicU64::new(self.creator_channel.load(std::sync::atomic::Ordering::Acquire)),
            create_category: std::sync::atomic::AtomicU64::new(self.create_category.load(std::sync::atomic::Ordering::Acquire)),
            creator_ignore_channels: self.creator_ignore_channels.clone(),
            creator_delete_non_created_channels: std::sync::atomic::AtomicBool::new(self.creator_delete_non_created_channels.load(std::sync::atomic::Ordering::Acquire)),
            created_channels: self.created_channels.clone(),
            creator_mark_delete_channels: self.creator_mark_delete_channels.clone(),
            creator_delete_delay_nanoseconds: std::sync::atomic::AtomicU64::new(self.creator_delete_delay_nanoseconds.load(std::sync::atomic::Ordering::Acquire)),
            xp_ignored_channels: self.xp_ignored_channels.clone(),
            xp_vc_tmp: self.xp_vc_tmp.clone(),
            xp_vc: self.xp_vc.clone(),
            xp_txt: self.xp_txt.clone(),
            xp_txt_apply_milliseconds: std::sync::atomic::AtomicU32::new(self.xp_txt_apply_milliseconds.load(std::sync::atomic::Ordering::Acquire)),
            xp_txt_punish_seconds: std::sync::atomic::AtomicU64::new(self.xp_txt_punish_seconds.load(std::sync::atomic::Ordering::Acquire)),
            twitch_live_notification: self.twitch_live_notification.clone(),
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct TwitchLiveNotifications {
    discord_channel: std::sync::atomic::AtomicU64,                                                                      //ChannelId
    twitch_channels: std::collections::HashSet<u64>,
}

impl Clone for TwitchLiveNotifications {
    fn clone(&self) -> Self {
        Self {
            discord_channel: std::sync::atomic::AtomicU64::new(self.discord_channel.load(std::sync::atomic::Ordering::Acquire)),
            twitch_channels: self.twitch_channels.clone(),
        }
    }
}


#[derive(Clone, Default, Debug, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
struct HandlerWrapper(Arc<OuterHandler>);
impl Deref for HandlerWrapper {
    type Target = Arc<OuterHandler>;
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
            //init all guilds
            {
                for guild in guilds {
                    let handler = self.guilds.entry_async(guild).await.or_insert(Handler{
                        guild_id: guild,
                        ..Default::default()
                    });
                    let handler = &*handler;
                    handler.check_delete_channels(ctx.clone(), &None).await;
                    //Populate all the voice states on startup
                    if let Some(voice_states) = ctx.cache.guild(guild)
                        .map(|v|v.voice_states.iter()
                            .filter(|(_,s)|s.guild_id == Some(guild) && s.channel_id.is_some())
                            .map(|(u,s)|(*u, s.clone()))
                            .collect::<Vec<_>>()) {
                        for (user, state) in voice_states {
                            match state.channel_id {
                                Some(channel) => {
                                    handler.vc_join_xp(channel, user).await;
                                },
                                None => {}
                            }
                        }
                        tracing::info!("Applied voice states for guild {guild}");
                    } else {
                        tracing::warn!("Guild {guild} not found in cache. Cannot prefill voice states.");
                    }
                }
                //if any guilds has not been processed at this point, then this bot is not in that guild.
                //In that case, we don't actually need to process it.
            }
        })
    }
    fn voice_state_update<'life0, 'async_trait>(&'life0 self, ctx: poise::serenity_prelude::Context, old_state: std::option::Option<serenity::all::VoiceState>, new_state: serenity::all::VoiceState)
    -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send + 'async_trait>>
    where Self: 'async_trait, 'life0: 'async_trait
    {
        Box::pin(async move {
            if let Some(guild) = new_state.guild_id {
                let handler = self.guilds.entry_async(guild).await.or_insert(Handler{
                    guild_id: guild,
                    ..Default::default()
                });
                let handler = &*handler;
                match new_state.channel_id {
                    Some(channel) => {
                        let old_state = &old_state;
                        let apply_vc_other_guild = async {
                            if Some(guild) != old_state.as_ref().map(|v|v.guild_id).flatten() {
                                if let Some(old_state) = old_state {
                                    if let Some(old_guild) = old_state.guild_id {
                                        let old_handler = self.guilds.entry_async(old_guild).await.or_insert(Handler{
                                            guild_id: old_guild,
                                            ..Default::default()
                                        });
                                        let old_handler = &*old_handler;
                                        match old_state.channel_id {
                                            Some(_) => {
                                                old_handler.try_apply_vc_xp(old_state.user_id).await;
                                            },
                                            None => {}
                                        }
                                    }
                                }
                            }
                        };
                        tokio::join!(
                            apply_vc_other_guild,
                            handler.vc_join_xp(channel, new_state.user_id),
                            handler.vc_join_channel_temp_channel(ctx, new_state.user_id, channel, old_state),
                        );
                    },
                    None => {
                        tokio::join!(
                            handler.try_apply_vc_xp(new_state.user_id),
                            handler.check_delete_channels(ctx, &old_state),
                        );
                    },
                }
            }
        })
    }

    fn message<'life0, 'async_trait>(&'life0 self, _: poise::serenity_prelude::Context, message: serenity::model::channel::Message)
                                                -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send + 'async_trait>>
    where Self: 'async_trait, 'life0: 'async_trait
    {
        Box::pin(async move {
            if let Some(guild) = message.guild_id {
                let handler = self.guilds.entry_async(guild).await.or_insert(Handler{
                    guild_id: guild,
                    ..Default::default()
                });
                let handler = &*handler;
                handler.message_xp(message).await;
            }
        })
    }
    fn reaction_add<'life0, 'async_trait>(&'life0 self, _: poise::serenity_prelude::Context, add_reaction: serenity::Reaction)
                                                -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send + 'async_trait>>
    where Self: 'async_trait, 'life0: 'async_trait
    {
        Box::pin(async move {
            if let Some(guild) = add_reaction.guild_id {
                let handler = self.guilds.entry_async(guild).await.or_insert(Handler{
                    guild_id: guild,
                    ..Default::default()
                });
                let handler = &*handler;
                handler.message_xp_react(add_reaction).await;
            }
        })
    }
}
impl Default for Handler {
    fn default() -> Self {
        Self {
            guild_id: serenity::GuildId::default(),
            creator_channel: std::sync::atomic::AtomicU64::new(0),
            create_category: std::sync::atomic::AtomicU64::new(0),
            creator_delete_non_created_channels: std::sync::atomic::AtomicBool::new(false),
            creator_ignore_channels: Default::default(),
            created_channels: Default::default(),
            creator_mark_delete_channels: Default::default(),
            creator_delete_delay_nanoseconds: std::sync::atomic::AtomicU64::new(15*1_000*1_000*1_000), //15s * 1_000ms/s * 1_000us/ms * 1_000ns/us
            xp_ignored_channels: Default::default(),
            xp_vc_tmp: Default::default(),
            xp_vc: Default::default(),
            xp_txt: Default::default(),
            xp_txt_apply_milliseconds: std::sync::atomic::AtomicU32::new(50),
            xp_txt_punish_seconds: std::sync::atomic::AtomicU64::new(60*2),
            twitch_live_notification: None,
        }
    }
}

struct Cache {
    handler: HandlerWrapper,
    auth: Arc<crate::rocket::auth::Auth>,
}

impl Cache {
    async fn save(&self) {
        tracing::info!("Serializing handler");
        let serialized = match serde_json::to_vec(&self.handler) {
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
        tracing::info!("Serializing twitch");
        match self.auth.twitch.auth.save().await {
            Ok(()) => {},
            Err(err) => {
                tracing::error!("Error saving twitch: {err}");
            }
        }
        tracing::info!("Serializing discord");
        match self.auth.discord.auth.save().await {
            Ok(()) => {},
            Err(err) => {
                tracing::error!("Error saving discord: {err}");
            }
        }
        //TODO: This is a bit hacky, but has to be done this way, to keep this send.
        let guilds = {
            let mut guilds = Vec::new();
            let guard = sdd::Guard::new();
            for (guild, _) in self.handler.guilds.iter(&guard) {
                guilds.push(*guild);
            }
            guilds
        };
        for guild in guilds {
            let handler = self.handler.guilds.get_async(&guild).await;
            if let Some(handler) = handler {
                handler.xp_incremental_check().await;
            }
        }
    }
}
const CACHE_PATH: &str = "cache.json";
pub async fn init_client(auth: Arc<crate::rocket::auth::Auth>) -> ::anyhow::Result<serenity::Client> {
    tracing::info!("Client Startup");

    tracing::debug!("Getting Client Token");
    let token = std::env::var("DISCORD_TOKEN")?;
    validate_token(&token)?;

    let cache = {
        let handler =
        tokio::fs::read(CACHE_PATH).await
            .map(|v|
                serde_json::from_slice::<HandlerWrapper>(v.as_slice())
                    .inspect_err(|err|tracing::error!("Error deserializing cache. Falling back to Defaults. Error: {err}"))
                    .unwrap_or_default()
            ).inspect_err(|err|tracing::info!("Error reading cache. Falling back to Defaults. Error: {err}"))
            .unwrap_or_default();
        let auth = auth.clone();
        Cache {
            handler,
            auth,
        }
    };

    tracing::info!("Got Cache: {:?}", cache.handler);
    let handler = cache.handler.clone();
    let handler_1 = handler.0.clone();
    
    
    let framework = poise::framework::FrameworkBuilder::default()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::ping(),
                commands::debug(),
                commands::settings(),
            ],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    handler: handler_1,
                    auth,
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
            let cache = cache;
            loop {
                tokio::select!(
                    biased;
                    _ = &mut receiver => return cache,
                    _ = interval.tick() => cache.save().await,
                )
            }
        }))
    };

    let client = serenity::Client::builder(&token, GatewayIntents::default().union(GatewayIntents::MESSAGE_CONTENT))
        .framework(framework)
        .event_handler(handler.clone())
        .await?;

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        super::SHUTDOWN.notified().await;
        let _ = sender.send(());
        let mut new_js = tokio::task::JoinSet::new();
        new_js.spawn(async {
            match saver.await {
                Ok(v) => v.save().await,
                Err(err) => {
                    tracing::error!("Cache saver Thread Panicked: {err}");
                }
            }
        });
        new_js.spawn(async move {shard_manager.shutdown_all().await});
        new_js.join_all().await;
    });

    Ok(client)
}