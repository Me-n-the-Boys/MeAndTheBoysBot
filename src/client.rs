mod temp_channels;
mod xp;
mod commands;

use poise::serenity_prelude as serenity;
use serenity::utils::validate_token;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use serenity::all::{Context, Event, GatewayIntents, Guild, PartialGuild};
use tokio::io::AsyncReadExt;

/// User data, which is stored and accessible in all command invocations
struct Data;
#[derive(Clone)]
struct Handler{
    pool: sqlx::PgPool,
}
#[poise::async_trait]
impl serenity::client::RawEventHandler for Handler {
    async fn raw_event(&self, ctx: Context, ev: Event) {
        match ev {
            Event::Ready(_) => {}
            Event::Resumed(_) => {}
            Event::UserUpdate(_) => {}

            Event::ChannelCreate(channel) => {
                let channel_name = channel.channel.name;
                let channel_id = crate::converti(channel.channel.id.get());
                let category_id = channel.channel.parent_id.map(serenity::ChannelId::get).map(crate::converti);
                let guild_id = crate::converti(channel.channel.guild_id.get());
                match sqlx::query!(
r#"INSERT INTO temp_channels_created SELECT guild_id, $2::bigint as channel_id, NULL as mark_delete, $4::text as name FROM temp_channels
WHERE guild_id = $1 AND temp_channels.create_category = $3"#, guild_id, channel_id, category_id, channel_name).execute(&self.pool).await
                {
                    Ok(_) => {},
                    Err(err) => {
                        tracing::error!("Error creating temp channel: {err}");
                    }
                }
            }
            Event::ChannelUpdate(channel) => {
                let channel_name = channel.channel.name;
                let channel_id = crate::converti(channel.channel.id.get());
                let guild_id = crate::converti(channel.channel.guild_id.get());
                match sqlx::query!(
                    r#"UPDATE temp_channels_created SET name = $1 WHERE guild_id = $2 AND channel_id = $3"#,
                    channel_name, guild_id, channel_id
                ).execute(&self.pool).await
                {
                    Ok(_) => {},
                    Err(err) => {
                        tracing::error!("Error updating temp channel: {err}");
                    }
                }
            }
            Event::ChannelDelete(channel) => {
                let channel_id = crate::converti(channel.channel.id.get());
                let guild_id = crate::converti(channel.channel.guild_id.get());
                match sqlx::query!(r#"DELETE FROM temp_channels_created WHERE guild_id = $1 AND channel_id = $2"#, guild_id, channel_id).execute(&self.pool).await {
                    Ok(_) => {},
                    Err(err) => {
                        tracing::error!("Error deleting temp channel: {err}");
                    }
                }
            }

            Event::GuildCreate(create) => {
                self.guild_info(create.guild.into()).await
            }
            Event::GuildUpdate(update) => {
                self.guild_info(update.guild).await
            }
            Event::GuildDelete(_) => {}

            Event::GuildMemberAdd(_) => {}
            Event::GuildMemberRemove(_) => {}
            Event::GuildMemberUpdate(_) => {}
            Event::GuildMembersChunk(_) => {}

            Event::VoiceStateUpdate(state) => {
                let new_state = state.voice_state;
                let guild_id = match new_state.guild_id{
                    Some(v) => v,
                    None => return,
                };
                let db = crate::get_db().await;
                let apply_vc = async {
                    match new_state.channel_id {
                        None => {
                            match sqlx::query!("WITH vc_xp_apply AS (
    DELETE FROM xp_vc_tmp WHERE guild_id = $1 AND user_id = $2 RETURNING *
) MERGE INTO xp_user USING vc_xp_apply ON xp_user.guild_id = vc_xp_apply.guild_id AND xp_user.user_id = vc_xp_apply.user_id
WHEN MATCHED THEN UPDATE SET vc = xp_user.vc + (now() - vc_xp_apply.time)
WHEN NOT MATCHED THEN INSERT (guild_id, user_id, vc) VALUES (vc_xp_apply.guild_id, vc_xp_apply.user_id, (now() - vc_xp_apply.time))
", crate::converti(guild_id.get()), crate::converti(new_state.user_id.get())).execute(&db).await {
                                Ok(v) => match v.rows_affected(){
                                    0 => {
                                        tracing::error!("Error applying voice xp: No rows affected");
                                    },
                                    _ => {},
                                },
                                Err(err) => {
                                    tracing::error!("Error applying voice xp: {err}");
                                }
                            }
                        },
                        Some(_) => {
                            match sqlx::query!(r#"MERGE INTO xp_vc_tmp
USING (SELECT $1::bigint as guild_id, $2::bigint as user_id) as input ON xp_vc_tmp.guild_id = input.guild_id AND xp_vc_tmp.user_id = input.user_id
WHEN MATCHED THEN DO NOTHING
WHEN NOT MATCHED THEN INSERT (guild_id, user_id, time) VALUES (input.guild_id, input.user_id, now())
"#, crate::converti(guild_id.get()), crate::converti(new_state.user_id.get())).execute(&db).await {
                                Ok(_) => {},
                                Err(err) => {
                                    tracing::error!("Error adding user to voice xp tmp: {err}");
                                }
                            }
                        }
                    }
                };
                let temp_channel = async {
                    match new_state.channel_id {
                        Some(channel) => {
                            tracing::debug!("User {} is joining channel {channel}", new_state.user_id);
                            self.vc_join_channel_temp_channel(ctx, guild_id, new_state.user_id, channel).await
                        },
                        None => {
                            tracing::debug!("User {} left a channel", new_state.user_id);
                            match sqlx::query!(r#"DELETE FROM temp_channels_created_users WHERE guild_id = $1 AND user_id = $2"#, crate::converti(guild_id.get()), crate::converti(user_id.get())).execute(&self.pool).await {
                                Ok(_) => {},
                                Err(err) => {
                                    tracing::error!("Error getting created channels: {err}");
                                    return;
                                }
                            };
                            self.check_delete_channels(ctx, new_state.user_id, guild_id).await
                        }
                    }
                };
                tokio::join!(
                    temp_channel,
                    apply_vc,
                );
            }
            Event::VoiceChannelStatusUpdate(_) => {}

            Event::MessageCreate(create) => {
                self.message_xp(create.message).await;
            }
            Event::MessageUpdate(_) => {}
            Event::MessageDelete(_) => {}
            Event::MessageDeleteBulk(_) => {}
            Event::ReactionAdd(add) => {
                self.message_xp_react(add.reaction).await;
            }
            Event::ReactionRemove(_) => {}
            Event::ReactionRemoveAll(_) => {}
            Event::ReactionRemoveEmoji(_) => {}

            //Unneeded events
            Event::VoiceServerUpdate(_) => {}
            Event::CommandPermissionsUpdate(_) => {}
            Event::AutoModRuleCreate(_) => {}
            Event::AutoModRuleUpdate(_) => {}
            Event::AutoModRuleDelete(_) => {}
            Event::AutoModActionExecution(_) => {}
            Event::ChannelPinsUpdate(_) => {}
            Event::GuildAuditLogEntryCreate(_) => {}
            Event::GuildBanAdd(_) => {}
            Event::GuildBanRemove(_) => {}
            Event::GuildEmojisUpdate(_) => {}
            Event::GuildIntegrationsUpdate(_) => {}
            Event::GuildRoleCreate(_) => {}
            Event::GuildRoleDelete(_) => {}
            Event::GuildRoleUpdate(_) => {}
            Event::GuildStickersUpdate(_) => {}
            Event::InviteCreate(_) => {}
            Event::InviteDelete(_) => {}
            Event::PresenceUpdate(_) => {}
            // Event::PresencesReplace(_) => {}
            Event::TypingStart(_) => {}
            Event::WebhookUpdate(_) => {}
            Event::InteractionCreate(_) => {}
            Event::IntegrationCreate(_) => {}
            Event::IntegrationUpdate(_) => {}
            Event::IntegrationDelete(_) => {}
            Event::StageInstanceCreate(_) => {}
            Event::StageInstanceUpdate(_) => {}
            Event::StageInstanceDelete(_) => {}
            Event::ThreadCreate(_) => {}
            Event::ThreadUpdate(_) => {}
            Event::ThreadDelete(_) => {}
            Event::ThreadListSync(_) => {}
            Event::ThreadMemberUpdate(_) => {}
            Event::ThreadMembersUpdate(_) => {}
            Event::GuildScheduledEventCreate(_) => {}
            Event::GuildScheduledEventUpdate(_) => {}
            Event::GuildScheduledEventDelete(_) => {}
            Event::GuildScheduledEventUserAdd(_) => {}
            Event::GuildScheduledEventUserRemove(_) => {}
            Event::EntitlementCreate(_) => {}
            Event::EntitlementUpdate(_) => {}
            Event::EntitlementDelete(_) => {}
            Event::MessagePollVoteAdd(_) => {}
            Event::MessagePollVoteRemove(_) => {}
            Event::Unknown(_) => {}
            _ => {}
        }
    }
}

impl Handler{
    async fn guild_info(&self, new_data: serenity::PartialGuild) {
        let id = crate::converti(new_data.id.get());
        let name = new_data.name.as_str();
        let icon = new_data.icon.map(|v| v.to_string());

        let db = crate::get_db().await;
        match sqlx::query!(r#"INSERT INTO guilds (guild_id, name, icon) VALUES ($1, $2, $3) ON CONFLICT (guild_id) DO UPDATE SET name = $2, icon = $3"#, id, name, icon)
        .execute(&db).await {
            Ok(_) => {},
            Err(err) => {
                tracing::error!("Error updating guild: {err}");
            }
        }
    }
}

pub async fn migrate() -> ::anyhow::Result<()> {
    //<editor-fold desc="Old Struct definitions">
    const CACHE_PATH: &str = "cache.json";
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
    //</editor-fold>
    match tokio::fs::File::open(CACHE_PATH).await {
        Ok(mut file) => {
            let mut v = Vec::new();
            let _ = file.read_to_end(&mut v).await.expect("Failed to read cache file");

            let handler = serde_json::from_slice::<HandlerWrapper>(v.as_slice())
                .expect("Error deserializing cache.");

            {
                let db = crate::get_db().await;
                let mut transaction = db.begin().await.expect("Failed to start transaction");
                for (guild, handler) in handler.guilds.iter(&sdd::Guard::new()).map(|(guild, handler)|(*guild, handler.clone())).collect::<Vec<_>>().into_iter() {
                    let guild = crate::converti(guild.get());
                    sqlx::query!("INSERT INTO guilds (guild_id) VALUES ($1) ON CONFLICT DO NOTHING", guild).execute(&mut *transaction).await?;
                    let creator_channel = handler.creator_channel.load(std::sync::atomic::Ordering::Acquire);
                    if creator_channel != 0 {
                        let creator_channel = crate::converti(creator_channel);
                        let create_category = handler.create_category.load(std::sync::atomic::Ordering::Acquire);
                        let create_category = if create_category != 0 { Some(crate::converti(create_category)) } else { None };

                        let delete_delay = {
                            let delete_delay = std::time::Duration::from_nanos(handler.creator_delete_delay_nanoseconds.load(std::sync::atomic::Ordering::Acquire));
                            const SECS_PER_DAY: u32 = 24*60*60;
                            let delete_days = delete_delay.as_secs() / 24 / 60 / 60;
                            let delete_days = (delete_days % 30) + delete_delay.as_secs()/u64::from(SECS_PER_DAY);
                            //This assumes 30 days per month, which is not correct, but good enough for this purpose.
                            sqlx::postgres::types::PgInterval{
                                months: (delete_days / 30) as i32,
                                days: (delete_days%30) as i32,
                                microseconds: i64::from(delete_delay.subsec_micros()) + ((delete_delay.as_secs()%u64::from(SECS_PER_DAY))*1_000*1_000) as i64,
                            }
                        };
                        sqlx::query!("INSERT INTO temp_channels (guild_id, creator_channel, create_category, delete_delay) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
                    guild, creator_channel, create_category, delete_delay).execute(&mut *transaction).await?;
                    }
                    for (channel, ()) in handler.created_channels.iter(&sdd::Guard::new()).map(|(channel, _)|(*channel, ())).collect::<Vec<_>>().into_iter() {
                        let channel = crate::converti(channel.get());
                        sqlx::query!("INSERT INTO temp_channels_created (guild_id, channel_id) VALUES ($1, $2) ON CONFLICT DO NOTHING", guild, channel).execute(&mut *transaction).await?;
                    }
                    for (channel, _) in handler.creator_ignore_channels.iter(&sdd::Guard::new()).map(|(channel, ())|(*channel, ())).collect::<Vec<_>>().into_iter() {
                        let channel = crate::converti(channel.get());
                        sqlx::query!("INSERT INTO temp_channels_ignore (guild_id, channel_id) VALUES ($1, $2) ON CONFLICT DO NOTHING", guild, channel).execute(&mut *transaction).await?;
                    }
                    for (channel, _) in handler.xp_ignored_channels.iter(&sdd::Guard::new()).map(|(channel, ())|(*channel, ())).collect::<Vec<_>>().into_iter() {
                        let channel = crate::converti(channel.get());
                        sqlx::query!("INSERT INTO xp_channels_ignored (guild_id, channel_id) VALUES ($1, $2) ON CONFLICT DO NOTHING", guild, channel).execute(&mut *transaction).await?;
                    }
                    let xp_txt_apply_milliseconds = sqlx::postgres::types::PgInterval{
                        months: 0,
                        days: 0,
                        microseconds: i64::from(handler.xp_txt_apply_milliseconds.load(std::sync::atomic::Ordering::Acquire)*1000),
                    };
                    let xp_txt_punish_seconds = {
                        let xp_txt_punish_seconds = handler.xp_txt_punish_seconds.load(std::sync::atomic::Ordering::Acquire);

                        let days = xp_txt_punish_seconds/24/60/60;

                        sqlx::postgres::types::PgInterval{
                            months: (days/30) as i32,
                            days: (days%30) as i32,
                            microseconds: (xp_txt_punish_seconds%(24*60*60)) as i64 * 1000 * 1000,
                        }
                    };
                    sqlx::query!("INSERT INTO xp (guild_id, txt_apply_interval, txt_punish_interval) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING", guild, xp_txt_apply_milliseconds, xp_txt_punish_seconds).execute(&mut *transaction).await?;

                    let mut xp = std::collections::HashMap::<_, (Option<u64>, Option<i64>)>::new();
                    let mut xp_txt_tmp = std::collections::HashMap::new();
                    {
                        let mut next_entry = handler.xp_vc.first_entry_async().await;
                        while let Some(entry) = next_entry {
                            let user = *entry.key();
                            let vc_xp = *entry;
                            xp.entry(user).or_default().0 = Some(vc_xp);
                            next_entry = entry.next_async().await;
                        }
                        let mut next_entry = handler.xp_txt.first_entry_async().await;
                        while let Some(entry) = next_entry {
                            let user = *entry.key();
                            let xp_txt = *entry;
                            xp.entry(user).or_default().1 = Some(xp_txt.xp as i64);
                            if let Some(pending) = xp_txt.pending {
                                *xp_txt_tmp.entry(user).or_default() = pending;
                            }
                            next_entry = entry.next_async().await;
                        }
                    }
                }
                transaction.commit().await.expect("Failed to commit transaction");
            }
            drop(file);
            tokio::fs::remove_file(CACHE_PATH).await.expect("Failed to remove cache file");
        }
        Err(v) => {
            tracing::info!("Failed to open cache file: {v}");
        }
    }
    Ok(())
}
pub async fn init_client(auth: Arc<crate::rocket::auth::Auth>) -> ::anyhow::Result<serenity::Client> {
    tracing::info!("Client Startup");
    migrate().await.expect("Failed to migrate");

    tracing::debug!("Getting Client Token");
    let token = std::env::var("DISCORD_TOKEN")?;
    validate_token(&token)?;
    
    let framework = poise::framework::FrameworkBuilder::default()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::ping(),
                commands::copy_emoji(),
                commands::settings(),
            ],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data)
            })
        })
        .initialize_owners(true)
        .build();

    let handler = Handler{
        pool: crate::get_db().await,
    };
    let client = serenity::Client::builder(&token, GatewayIntents::default().union(GatewayIntents::MESSAGE_CONTENT))
        .framework(framework)
        .raw_event_handler(handler.clone())
        .await?;

    let shard_manager = client.shard_manager.clone();

    {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(core::time::Duration::from_secs(60));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let db = crate::get_db().await;
            loop{
                tokio::select! {
                    biased;
                    _ = super::SHUTDOWN.notified() => {
                        shard_manager.shutdown_all().await;
                    },
                    _ = interval.tick() => {
                        match sqlx::query!(r#"WITH vc_xp_apply AS (
UPDATE xp_user SET vc = xp_user.vc + (now() - xp_vc_tmp.time) FROM xp_vc_tmp WHERE xp_user.guild_id = xp_vc_tmp.guild_id AND xp_user.user_id = xp_vc_tmp.user_id RETURNING xp_user.guild_id, xp_user.user_id
) UPDATE xp_vc_tmp SET time = now() FROM vc_xp_apply WHERE xp_vc_tmp.guild_id = vc_xp_apply.guild_id AND xp_vc_tmp.user_id = vc_xp_apply.user_id
"#)
                        .execute(&db).await {
                            Ok(result) => {
                                match result.rows_affected() {
                                    0 => {},
                                    v => {
                                        tracing::info!("Applied voice xp of {v} users", );
                                    }
                                }
                            },
                            Err(err) => {
                                tracing::error!("Error applying voice xp: {err}");
                            }
                        }
                        handler.apply_previous_message_xp(None, None).await;
                    },
                }
            }
        });
    }

    Ok(client)
}