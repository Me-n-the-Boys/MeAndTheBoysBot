mod rocket_callback;

use twitch_api::twitch_oauth2::{AccessToken, TwitchToken, UserToken};

pub(in super) static TWITCH_WS_SECRET:std::sync::LazyLock<[u8;64]>  = std::sync::LazyLock::new(||{
    rand::random()
});
pub(in super) static TWITCH_WS_SECRET_STRING:std::sync::LazyLock<String>  = std::sync::LazyLock::new(||{
    use base64::engine::Engine;
    base64::engine::general_purpose::URL_SAFE.encode(&*TWITCH_WS_SECRET)
});

#[non_exhaustive]
pub(crate) struct Twitch {
    pub(super) client_id: ::twitch_api::twitch_oauth2::types::ClientId,
    pub(super) client_secret: ::twitch_api::twitch_oauth2::types::ClientSecret,
    pub(super) client: twitch_api::HelixClient<'static, reqwest::Client>,
    pub(super) access_token: twitch_api::twitch_oauth2::AppAccessToken,
    conduit: twitch_api::eventsub::Conduit,
    pub(super) auth: TwitchAuthentications,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct TwitchAuthentications {
    pub(crate) authentications: scc::HashMap<twitch_api::types::UserId, TwitchAuthentication>,
    pub(crate) enabled_channels: scc::HashMap<twitch_api::types::UserId, scc::HashMap<serenity::model::id::ChannelId, (bool, chrono::DateTime<chrono::Utc>)>>,
    pub(crate) live_message: scc::HashMap<twitch_api::types::UserId, TwitchLiveMessage>,
    conduit: Option<twitch_api::eventsub::Conduit>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub(crate) struct TwitchAuthentication{
    pub access_token: AccessToken,
    pub client_id: twitch_api::twitch_oauth2::ClientId,
    /// Username of user associated with this token
    pub login: twitch_api::types::UserName,
    /// User ID of the user associated with this token
    pub user_id: twitch_api::types::UserId,
    /// The refresh token used to extend the life of this user token
    pub refresh_token: Option<twitch_api::twitch_oauth2::RefreshToken>,
    pub expiry: TwitchAuthenticationTime,
}

impl TwitchAuthentications {
    async fn load() -> Self {
        match tokio::fs::read_to_string(TWITCH_AUTH_PATH).await {
            Ok(v) => serde_json::from_str(v.as_str()).unwrap_or_else(|err| {
                tracing::info!("Failed to parse Twitch Authentications: {err}");
                Default::default()
            }),
            Err(err) => {
                tracing::info!("Failed to read Twitch Authentications file: {err}");
                Default::default()
            }
        }
    }
    pub(crate) async fn save(&self) -> ::anyhow::Result<()> {
        let auth = serde_json::to_string(&self)?;
        match tokio::fs::write(TWITCH_AUTH_PATH, auth).await {
            Ok(()) => {
                tracing::info!("Saved Twitch Authentications");
            },
            Err(err) => {
                tracing::error!("Failed to save Twitch Authentications: {err}");
            }
        }
        Ok(())
    }
}

#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub(crate) struct TwitchLiveMessage{
    pub live_message: String,
    pub last_changed: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) enum TwitchAuthenticationTime {
    /// Token will never expire
    ///
    /// This is only true for old client IDs, like <https://twitchapps.com/tmi> and others
    NeverExpiring,
    /// Expiration when the token will roughly expire.
    /// This is not exact, as twitch only returns the duration of the token, not the exact time it was created.
    Expiring(chrono::DateTime<chrono::Utc>),
}

impl TwitchAuthenticationTime {
    pub fn new_expiry(expiry: std::time::Duration) -> Self {
        if let Ok(seconds) = i64::try_from(expiry.as_secs()) {
            match chrono::Utc::now().checked_add_signed(chrono::TimeDelta::seconds(seconds)) {
                Some(v) => TwitchAuthenticationTime::Expiring(v),
                //Let's just treat a date-time, that is so far in the future that it cannot be represented as a never expiring token.
                None => TwitchAuthenticationTime::NeverExpiring,
            }
        } else {
            TwitchAuthenticationTime::NeverExpiring
        }
    }
}

impl TwitchAuthentication {
    pub async fn check_valid(&mut self, twitch: &Twitch) -> bool {
        match self.access_token.validate_token(&twitch.client).await {
            Ok(v) => {
                if let Some(expiry) = v.expires_in {
                    self.expiry = TwitchAuthenticationTime::new_expiry(expiry);
                }
                if let Some(login) = v.login {
                    self.login = login;
                }
                if let Some(user_id) = v.user_id {
                    self.user_id = user_id;
                }
                self.client_id = v.client_id;

                return true;
            }
            Err(err) => {
                let user_id = &self.user_id;
                let user_name = &self.login;
                tracing::warn!("Failed to validate token for {user_id}/{user_name}: {err}");
                //Don't return here, since we might be able to refresh the token
            },
        }
        match self.refresh_token.take() {
            Some(v) => {
                match v.refresh_token(&twitch.client, &twitch.client_id, Some(&twitch.client_secret)).await {
                    Ok((access_token, expiry, refresh_token)) => {
                        self.access_token = access_token;
                        self.expiry = TwitchAuthenticationTime::new_expiry(expiry);
                        self.refresh_token = refresh_token;
                        true
                    }
                    Err(err) => {
                        let user_id = &self.user_id;
                        let user_name = &self.login;
                        tracing::error!("Failed to refresh token for {user_id}/{user_name}: {err}");
                        false
                    }
                }
            },
            None => {
                let user_id = &self.user_id;
                let user_name = &self.login;
                tracing::warn!("Token is invalid and there is no refresh token for {user_id}/{user_name}");
                false
            }
        }
    }
}

impl From<twitch_api::twitch_oauth2::UserToken> for TwitchAuthentication {
    fn from(value: UserToken) -> Self {
        let expiry = if value.never_expiring {
            TwitchAuthenticationTime::NeverExpiring
        } else {
            let expires_at = chrono::Utc::now() + value.expires_in();
            TwitchAuthenticationTime::Expiring(expires_at)
        };
        let client_id = value.client_id().clone();
        Self {
            access_token: value.access_token,
            client_id,
            login: value.login,
            user_id: value.user_id,
            refresh_token: value.refresh_token,
            expiry,
        }
    }
}

pub(crate) const TWITCH_AUTH_PATH: &str = "twitch_authentications.json";
pub(in super) async fn create_twitch_client(mut rocket: rocket::Rocket<rocket::Build>) -> ::anyhow::Result<(rocket::Rocket<rocket::Build>, Twitch)> {
    let mut auth = TwitchAuthentications::load().await;
    //Get Twitch Client ID and Secret from .env
    let client_id = ::twitch_api::twitch_oauth2::types::ClientId::from(::std::env::var("TWITCH_CLIENT_ID")?);
    let client_secret = ::twitch_api::twitch_oauth2::types::ClientSecret::from(::std::env::var("TWITCH_CLIENT_SECRET")?);
    //Create Twitch Client
    let client = reqwest::Client::new();
    let client = twitch_api::HelixClient::with_client(client);
    //Utilize the Client credentials grant flow to get an app access token
    let access_token = twitch_api::twitch_oauth2::tokens::AppAccessToken::get_app_access_token(&client, client_id.clone(), client_secret.clone(), vec![]).await?;
    //Pre-Create a conduit for eventsub
    let created_conduit = auth.conduit.is_none();
    let conduit = match auth.conduit.clone() {
        Some(v) => v,
        None => {
            let conduit = client.create_conduit(1, &access_token).await?;
            auth.conduit = Some(conduit.clone());
            conduit
        },
    };
    let twitch = Twitch {
        client_id,
        client_secret,
        client,
        access_token,
        conduit,
        auth,
    };
    twitch.auth.save().await?;
    //Register the Twitch Rocket Callback to finish Conduit setup, once the Webserver is online
    if created_conduit {
        rocket = rocket.attach(rocket_callback::TwitchRocketCallback{
            client: twitch.client.clone(),
            access_token: twitch.access_token.clone(),
            conduit: twitch.conduit.clone(),
        });
    }

    Ok((rocket, twitch))
}