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
        "delete_non_created_channels",
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
        let handler = ctx.data().handler.guilds.entry_async(guild).await.or_default();
        let channel_id = channel.id();
        let old_channel = handler.creator_channel.swap(channel.id().get(), std::sync::atomic::Ordering::AcqRel);
        let mut out = match old_channel {
            0 => {
                format!("Set the temporary channel creator channel to <#{channel_id}>. ")
            }
            old_channel => {
                format!("Changed the temporary channel creator channel from <#{old_channel}> to <#{channel_id}>. ")
            }
        };
        if set_category.unwrap_or(true) {
            let category = channel.guild().map(|c| c.parent_id).flatten();
            let old_category = handler.create_category.swap(category.map_or_else(||0, |v|v.get()), std::sync::atomic::Ordering::AcqRel);
            match (old_category, category) {
                (0, Some(category)) => {
                    out.push_str(format!("And made channels spawn in <#{category}>.").as_str());
                },
                (0, None) => {},
                (old_category, Some(category)) => {
                    out.push_str(format!("Also changed the category from <#{old_category}> to <#{category}>.").as_str());
                },
                (old_category, None) => {
                    out.push_str(format!("And made channels no longer spawn in <#{old_category}>.").as_str());
                },
            }
        }
        drop(handler);
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
        let handler = ctx.data().handler.guilds.entry_async(guild).await.or_default();
        let category = category.guild().map(|c| c.parent_id).flatten();
        let old_category = handler.create_category.swap(category.map_or_else(||0, |v|v.get()), std::sync::atomic::Ordering::AcqRel);
        drop(handler);
        match (old_category, category) {
            (0, Some(category)) => {
                ctx.say(format!("Made channels spawn in the category: <#{category}>.")).await?;
            },
            (0, None) => {
                ctx.say("The Temporary Channel Spawn category did not change, because the specified channel is not in a category and no category was set previously.").await?;
            },
            (old_category, Some(category)) => {
                ctx.say(format!("Changed the category from <#{old_category}> to <#{category}>.")).await?;
            },
            (old_category, None) => {
                ctx.say(format!("Made channels no longer spawn in the category: <#{old_category}>.")).await?;
            },
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
        let handler = ctx.data().handler.guilds.entry_async(guild).await.or_default();
        match action {
            Action::Add => {
                let out = handler.creator_ignore_channels.insert_async(channel, ()).await;
                drop(handler);
                match out{
                    Ok(()) => {
                        ctx.say(format!("Added <#{channel}> to the list of ignored channels for temporary channel deletion.")).await?;
                    },
                    Err(_) => {
                        ctx.say(format!("Channel <#{channel}> was already on the list of ignored channels for temporary channel deletion.")).await?;
                    }
                }
            },
            Action::Remove => {
                let out = handler.creator_ignore_channels.remove(&channel);
                drop(handler);
                if out {
                    ctx.say(format!("Removed <#{channel}> from the list of ignored channels for temporary channel deletion.")).await?;
                } else {
                    ctx.say(format!("Channel <#{channel}> was not on the list of ignored channels for temporary channel deletion.")).await?;
                }
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
        if let Some(handler) = ctx.data().handler.guilds.get_async(&guild).await {
            if handler.creator_ignore_channels.is_empty() {
                ctx.say("No channels are currently ignored for temporary channel deletion.").await?;
            } else {
                let mut out = format!("Channels ignored for temporary channel deletion:\n");
                let mut is_first = true;
                for (channel, ()) in handler.creator_ignore_channels.iter(&sdd::Guard::new()) {
                    if !is_first {
                        out.push_str(format!(", <#{channel}>\n").as_str());
                    } else {
                        is_first = false;
                        out.push_str(format!("<#{channel}>\n").as_str());
                    }
                }
                drop(handler);
                ctx.say(out).await?;
            }
        } else {
            ctx.say("No channels are currently ignored for temporary channel deletion.").await?;
        }
    } else {
        ctx.say("This command can only be used in a server.").await?;
    }
    Ok(())
}


///Chooses to dis-/allow deletion of other channels in the same category as the creation category.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
)]
async fn delete_non_created_channels(ctx: Context<'_>, value: bool) -> Result<(), Error> {
    if let Some(guild) = ctx.guild_id() {
        let handler = ctx.data().handler.guilds.entry_async(guild).await.or_default();
        let old_value = handler.creator_delete_non_created_channels.swap(value, std::sync::atomic::Ordering::AcqRel);
        drop(handler);
        ctx.say(format!("Changed the value from {old_value} to {value}.")).await?;
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
async fn delete_delay(ctx: Context<'_>, nanoseconds: u64) -> Result<(), Error> {
    if let Some(guild) = ctx.guild_id() {
        let handler = ctx.data().handler.guilds.entry_async(guild).await.or_default();
        let old_delay = handler.creator_delete_delay_nanoseconds.swap(nanoseconds, std::sync::atomic::Ordering::AcqRel);
        drop(handler);
        ctx.say(format!("Changed the delay for deleting temporary channels from {old_delay}ns to {nanoseconds}ns.")).await?;
    } else {
        ctx.say("This command can only be used in a server.").await?;
    }
    Ok(())
}

