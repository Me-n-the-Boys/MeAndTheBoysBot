use std::sync::Arc;
use askama::Template;

#[rocket::get("/", rank = 1)]
pub async fn index_none<'r>(
    _discord_session: Option<super::discord::oauth::session::Session>,
    twitch_session: Option<super::twitch::oauth::session::Session>,
) -> rocket::response::Redirect {
    if twitch_session.is_none() {
        rocket::response::Redirect::to(crate::rocket::auth::twitch::NEW_OAUTH_URL)
    } else {
        rocket::response::Redirect::to(crate::rocket::auth::discord::NEW_OAUTH_URL)
    }
}

// #[rocket::get("/", rank = 0)] //TODO: Fix error
pub async fn index<'r>(
    _auth: &rocket::State<Arc<crate::rocket::auth::Auth>>,
    discord_session: super::discord::oauth::session::Session,
    twitch_session: super::twitch::oauth::session::Session,
) -> Result<rocket::response::content::RawHtml<String>, askama::Error> {
    let discord_name = &discord_session.current_user.name;
    let twitch_name = &twitch_session.auth.login;
    let twitch_id = &twitch_session.auth.user_id;

    crate::template::dashboard::Dashboard{
        twitch_name,
        twitch_id,
        discord_name
    }.render()
        .map(rocket::response::content::RawHtml)
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