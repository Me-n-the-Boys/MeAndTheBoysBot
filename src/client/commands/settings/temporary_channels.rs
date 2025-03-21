use crate::client::commands::{Context, Error};
use poise::serenity_prelude as serenity;

///Various commands for changing some settings.
#[poise::command(
    slash_command,
    subcommands(
        "set_creator_channel",
        "set_category",
        "ignored_channels",
        "list_ignored_channels",
        "delete_delay",
    ),
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
    subcommand_required,
)]
pub async fn temporary_channels(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

///This command makes the channel function as a channel to create temporary channels.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
)]
async fn set_creator_channel(ctx: Context<'_>, channel: serenity::Channel, set_category: Option<bool>) -> Result<(), Error> {
    if let Some(guild) = ctx.guild_id() {
        let db = crate::get_db().await;
        let channel_id = crate::converti(channel.id().get());
        let set_category = set_category.unwrap_or(true);
        let category = if set_category {
            channel.category().map(|v|crate::converti(v.id.get()))
        } else {
            None
        };
        let guild = crate::converti(guild.get());
        let old_channel = sqlx::query!("
MERGE INTO temp_channels USING (SELECT $1::bigint, $2::bigint, $4::bigint) as input(guild_id, channel_id, category) ON temp_channels.guild_id = input.guild_id
WHEN MATCHED THEN UPDATE SET creator_channel = input.channel_id, create_category = CASE WHEN $3::bool THEN input.category ELSE temp_channels.create_category END
WHEN NOT MATCHED THEN INSERT (guild_id, creator_channel, create_category) VALUES (input.guild_id, input.channel_id, input.category)
", guild, channel_id, set_category, category).execute(&db).await?;
        if old_channel.rows_affected() == 0 {
            ctx.say("Nothing changed in the database? This shouldn't happen.").await?;
        }
        let mut out = format!("Set the temporary channel creator channel to <#{channel_id}>. ");
        if set_category {
            let category = category.map(|v|crate::convertu(v));
            match category {
                Some(category) => {
                    out.push_str(format!("And made channels spawn in <#{category}>.").as_str());
                },
                None => {
                    out.push_str(format!("And made channels no longer spawn in any category.").as_str());
                },
            }
        }
        ctx.say(out).await?;
    } else {
        ctx.say("This command can only be used in a server.").await?;
    }
    Ok(())
}


///Set's the category a channel will spawn in to the category the specified channel is in.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
)]
async fn set_category(ctx: Context<'_>, category: serenity::Channel) -> Result<(), Error> {
    if let Some(guild) = ctx.guild_id() {
        let category = category.category().map(|v|crate::converti(v.id.get()));
        let db = crate::get_db().await;
        let guild = crate::converti(guild.get());
        let old_channel = sqlx::query!("UPDATE temp_channels SET create_category = $2 WHERE guild_id = $1", guild, category).execute(&db).await?;
        if old_channel.rows_affected() == 0 {
            ctx.say("The Temporary Channel Spawn category did not change, because the guild does not have a creator channel set..").await?;
        } else {
            if let Some(category) = category.map(|v|crate::convertu(v)) {
                ctx.say(format!("Made channels spawn in the category: <#{category}>.")).await?;
            } else {
                ctx.say("Made channels spawn in no category.").await?;
            }
        }
    } else {
        ctx.say("This command can only be used in a server.").await?;
    }
    Ok(())
}

#[derive(poise::ChoiceParameter)]
enum Action {
    #[name_localized("de", "Hinzufügen")]
    Add,
    #[name_localized("de", "Entfernen")]
    Remove,
}

///Add or Remove Channels from the list of ignored channels for temporary channel deletion.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
)]
async fn ignored_channels(ctx: Context<'_>, action: Action, channel: serenity::ChannelId) -> Result<(), Error> {
    if let Some(guild) = ctx.guild_id() {
        let guild = crate::converti(guild.get());
        let channel = crate::converti(channel.get());
        let db = crate::get_db().await;
        match action {
            Action::Add => {
                let out = sqlx::query!("INSERT INTO temp_channels_ignore (guild_id, channel_id) VALUES ($1, $2) ON CONFLICT DO NOTHING", guild, channel).execute(&db).await?;
                let channel = crate::convertu(channel);
                match out.rows_affected() {
                    0 => {
                        ctx.say(format!("Channel <#{channel}> was already on the list of ignored channels for temporary channel deletion.")).await?;
                    }
                    _ => {
                        ctx.say(format!("Added <#{channel}> to the list of ignored channels for temporary channel deletion.")).await?;
                    },
                }
            },
            Action::Remove => {
                let out = sqlx::query!("DELETE FROM temp_channels_ignore WHERE guild_id = $1 AND channel_id = $2", guild, channel).execute(&db).await?;
                let channel = crate::convertu(channel);
                match out.rows_affected() {
                    0 => ctx.say(format!("Removed <#{channel}> from the list of ignored channels for temporary channel deletion.")).await?,
                    _ => ctx.say(format!("Channel <#{channel}> was not on the list of ignored channels for temporary channel deletion.")).await?,
                };
            },
        }
    } else {
        ctx.say("This command can only be used in a server.").await?;
    }
    Ok(())
}

///List ignored channels for temporary channel deletion.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
)]
async fn list_ignored_channels(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(guild) = ctx.guild_id() {
        let guild = crate::converti(guild.get());
        let db = crate::get_db().await;
        let channels = sqlx::query!("SELECT channel_id FROM temp_channels_ignore WHERE guild_id = $1", guild).fetch_all(&db).await?;
        if channels.is_empty() {
            ctx.say("No channels are currently ignored for temporary channel deletion.").await?;
        } else {
            let channel_list = channels.into_iter().map(|v|format!("<#{}>", v.channel_id)).reduce(|a,b|format!("{a}, {b}")).unwrap_or_default();
            let out = format!("Channels ignored for temporary channel deletion: {channel_list}\n");
            ctx.say(out).await?;
        }
    } else {
        ctx.say("This command can only be used in a server.").await?;
    }
    Ok(())
}


///Sets the time between the last person leaving a created channel and the channel being deleted.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
)]
async fn delete_delay(ctx: Context<'_>, microseconds: i64) -> Result<(), Error> {
    if let Some(guild) = ctx.guild_id() {
        let value = sqlx::postgres::types::PgInterval{
            microseconds,
            days: 0,
            months: 0,
        };
        let guild = crate::converti(guild.get());
        let db = crate::get_db().await;
        let out = sqlx::query!("UPDATE temp_channels SET delete_delay = $2 WHERE guild_id = $1", guild, value).execute(&db).await?;
        match out.rows_affected() {
            0 => {
                ctx.say(format!("No creator channel is configured.")).await?;
            },
            _ => {
                ctx.say(format!("Changed the delay for deleting temporary channels to {microseconds} microseconds. (1000microseconds = 1 millisecond)")).await?;
            },
        }
    } else {
        ctx.say("This command can only be used in a server.").await?;
    }
    Ok(())
}

