use std::sync::Arc;

#[rocket::get("/")]
pub async fn index<'r>(
    auth: &rocket::State<crate::rocket::auth::Auth>,
    dc: &rocket::State<Arc<crate::discord_client::DiscordClient>>,
    twitch_session: super::twitch::oauth::session::Session,
    discord_session: super::discord::oauth::session::Session,
) -> rocket::response::content::RawHtml<String> {
    let discord_name = &discord_session.current_user.name;
    let twitch_name = &twitch_session.auth.login;
    let twitch_id = &twitch_session.auth.user_id;
    rocket::response::content::RawHtml(format!(r#"
<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <meta name="color-scheme" content="light dark">
        <title>Hello World</title>
    </head>
    <body>
        <h1>Hello World</h1>
        <p>You are successfully logged in as:</p>
        <ul>
            <li>Twitch: {twitch_name} ({twitch_id})</li>
            <li>Discord: {discord_name}</li>
        </ul>
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