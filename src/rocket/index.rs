use std::sync::Arc;

#[rocket::get("/")]
pub async fn index<'r>(
    twitch: &rocket::State<Arc<crate::twitch_client::Twitch>>,
    dc: &rocket::State<Arc<crate::discord_client::DiscordClient>>,
    session: super::twitch::oauth::session::Session
) -> rocket::response::content::RawHtml<String> {
    let mut channels = String::new();
    let channels_iter;
    let mut channels_iter = match twitch.auth.enabled_channels.get_async(&session.auth.user_id).await {
        None => None,
        Some(v) => {
            channels_iter = v;
            channels_iter.first_entry_async().await
        }
    };

    while let Some(channels_entry) = channels_iter {
        if let Some(v) = decorate_channel(&channels_entry.key(), &*channels_entry, &dc).await {
            channels.push_str(v.as_str());
        }
        channels_iter = channels_entry.next_async().await;
    }

    let csrf_token = twitch.get_new_csrf().await;

    rocket::response::content::RawHtml(format!(r#"
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <meta name="color-scheme" content="light dark">
        <title>Discord Channel Configuration</title>
    </head>
    <body>
        <h1>Discord Channel Configuration</h1>
        <form action="/twitch" method="post">
            <input type="hidden" name="csrf_token" value="{csrf_token}"></input>
            <input type="submit" value="Save"></input>
            <div>
                {channels}
            </div>
        </form>
    </body>
</html>
"#))
}

async fn decorate_channel(channel: &serenity::model::id::ChannelId, value: &(bool, chrono::DateTime<chrono::Utc>), dc: &crate::discord_client::DiscordClient) -> Option<String> {
    let input = {
        let checked = if value.0 {
            "checked"
        } else {
            ""
        };
        format!(r#"<input name="{channel}" type="checkbox" {checked}></input>"#)
    };
    match dc.get_channel(channel.clone()).await {
        Ok(serenity::model::channel::Channel::Guild(channel)) => {
            match dc.get_guild(channel.guild_id.clone()).await {
                Ok(guild) => {
                    let guild_name = &guild.name;
                    let channel_name = &channel.name;
                    let image = guild.icon_url().map_or_else(String::new, |url|format!(r#"<img src="{url}" alt="Icon of the Guild: {channel_name}"></img>"#));

                    Some(format!(r#"<div><label>{image}{guild_name} -> {channel_name}: {input}></label></div>"#))
                }
                Err(_) => {
                    let guild_id = &channel.guild_id;
                    let name = &channel.name;

                    Some(format!(r#"<div><label>(Raw Guild Id: {guild_id}) -> {name}: {input}</label></div>"#))
                }
            }
        }
        Ok(_) => {
            None
        }
        Err(_) => {
            Some(format!(r#"<div><label>Deleted, Unknown or Unaccessible Channel Id: {channel}</label></div>"#))
        }
    }
}