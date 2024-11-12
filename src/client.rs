use std::collections::HashSet;
use poise::{CreateReply, serenity_prelude as serenity};
use serenity::gateway::ShardManager;
use serenity::utils::{validate_token};
use serenity::prelude::TypeMapKey;
use serenity::Client;
use std::default::Default;
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
struct Handler {
    guild_id: serenity::GuildId,
    creator_channel: serenity::ChannelId,
    create_category: Option<serenity::ChannelId>,
    ignore_channels: HashSet<serenity::ChannelId>,
    created_channels: tokio::sync::Mutex<std::collections::HashMap<serenity::ChannelId, serenity::GuildChannel>>,
    delete_delay: core::time::Duration,
}

impl serenity::client::EventHandler for Handler {
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
            if new_state.channel_id == Some(self.creator_channel) {
                self.create_channel(ctx, new_state.user_id).await;
                return;
            } else if new_state.channel_id == None {
                self.check_delete_channels(ctx, _old_state).await;
                return;
            }
        })
    }
}

impl Handler {
    async fn log_error(&self, ctx: &poise::serenity_prelude::Context, channel: serenity::ChannelId, error: String) {
        let send_message = serenity::CreateMessage::new()
            .content(error)
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
        if self.ignore_channels.contains(&channel) { return; }
        if channel == self.creator_channel { return; }
        //For now only worry about channels created by this bot instance.
        if self.created_channels.lock().await.contains_key(&channel) {
            //TODO: This is technically not 100% what is intended. If everyone leaves the channel this function will fire. If then someone rejoins and exits before self.delete_delay is up, the channel deletion will not be moved further back.
            let channel = match self.created_channels.lock().await.remove(&channel) {
                None => {
                    //Channel was already deleted?
                    return;
                }
                Some(channel) => channel,
            };
            tokio::time::sleep(self.delete_delay).await;
            match channel.members(&ctx) {
                Ok(members) => {
                    if members.is_empty() {
                        match channel.delete(&ctx).await {
                            Ok(_) => {},
                            Err(err) => {
                                self.log_error(&ctx, channel.id, format!("There was an error deleting the Channel: {error}")).await;
                                tracing::error!("Error deleting channel: {err}");
                                return;
                            }
                        }
                    } else {
                        self.created_channels.lock().await.insert(channel.id, channel);
                    }
                },
                Err(err) => {
                    self.log_error(&ctx, channel.id, format!("Error getting members of channel: {err}")).await;
                    self.created_channels.lock().await.insert(channel.id, channel);
                    return;
                }
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
                    for channel in v.values().filter(|channel|channel.parent_id == Some(category)) {
                        self.check_delete_channel(&ctx, channel.id).await;
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
                self.created_channels.lock().await.insert(v.id, v);
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

    let mut client = serenity::Client::builder(&token, GatewayIntents::default())
        .framework(framework)
        .event_handler(Handler{
            guild_id: serenity::GuildId::new(695718765119275109),
            creator_channel: serenity::ChannelId::new(1241371878761824356),
            create_category: Some(serenity::ChannelId::new(966080144747933757)),
            ignore_channels: HashSet::from([serenity::ChannelId::new(695720756189069313)]),
            created_channels: Default::default(),
            delete_delay: core::time::Duration::from_secs(15),
        })
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
        shard_manager.shutdown_all().await;
    });

    if let Err(why) = client.start_autosharded().await {
        tracing::error!("Client error: {:?}", why);
    }

    client
}