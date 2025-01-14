mod rocket_callback;

use base64::Engine;
use twitch_api::twitch_oauth2::{TwitchToken, UserToken};

pub(in super) static TWITCH_WS_SECRET:std::sync::LazyLock<[u8;64]>  = std::sync::LazyLock::new(||{
    rand::random()
});
pub(in super) static TWITCH_WS_SECRET_STRING:std::sync::LazyLock<String>  = std::sync::LazyLock::new(||{
    use base64::engine::Engine;
    base64::engine::general_purpose::URL_SAFE.encode(&*TWITCH_WS_SECRET)
});

#[non_exhaustive]
#[derive(Clone)]
pub(crate) struct UserTokenBuilder{
    pub client_id: ::twitch_api::twitch_oauth2::types::ClientId,
    pub client_secret: ::twitch_api::twitch_oauth2::types::ClientSecret,
    pub url: String,
}

#[rocket::async_trait]
impl<'a> rocket::request::FromRequest<'a> for UserTokenBuilder {
    type Error = String;

    async fn from_request(_: &'a rocket::Request<'_>) -> rocket::request::Outcome<Self, Self::Error> {
        match &*TWITCH_OAUTH {
            Ok(v) => rocket::request::Outcome::Success(v.clone()),
            Err(e) => {
                tracing::error!("Failed to get Twitch OAuth: {e}");
                rocket::request::Outcome::Error((rocket::http::Status::InternalServerError, format!("Failed to get Twitch OAuth: {e}")))
            }
        }
    }
}

pub(crate) static TWITCH_OAUTH: std::sync::LazyLock<Result<UserTokenBuilder, String>> = std::sync::LazyLock::new(||{
    let client_id = ::twitch_api::twitch_oauth2::types::ClientId::from(match ::dotenv::var("TWITCH_CLIENT_ID"){
        Ok(client_id) => client_id,
        Err(_) => return Err("Invalid Server configuration. Missing TWITCH_CLIENT_ID.".to_string()),
    });
    let client_secret = ::twitch_api::twitch_oauth2::types::ClientSecret::from(match ::dotenv::var("TWITCH_CLIENT_SECRET"){
        Ok(client_secret) => client_secret,
        Err(_) => return Err("Invalid Server configuration. Missing TWITCH_CLIENT_SECRET.".to_string()),
    });
    Ok(UserTokenBuilder {
        client_id,
        client_secret,
        url: format!("{}twitch/oauth", crate::twitch_client::get_base_url())
    })
});

const CSRF_TOKEN_LENGTH: usize = 32;
#[non_exhaustive]
pub(crate) struct Twitch {
    pub(super) client: twitch_api::HelixClient<'static, reqwest::Client>,
    pub(super) access_token: twitch_api::twitch_oauth2::AppAccessToken,
    conduit: twitch_api::eventsub::Conduit,
    pub(super) csrf_tokens: scc::HashIndex<[u8; CSRF_TOKEN_LENGTH], std::time::Instant>,
    pub(super) auth: TwitchAuthentications,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct TwitchAuthentications{
    pub(crate) authentications: scc::HashMap<twitch_api::types::UserId, TwitchAuthentication>,
    pub(crate) live_message: scc::HashMap<twitch_api::types::UserId, TwitchLiveMessage>,
    conduit: Option<twitch_api::eventsub::Conduit>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub(crate) struct TwitchAuthentication{
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
            client_id,
            login: value.login,
            user_id: value.user_id,
            refresh_token: value.refresh_token,
            expiry,
        }
    }
}

pub(crate) fn get_base_url() -> String {
    let debug = ::dotenv::var("DEBUG_URL").unwrap_or_default();
    format!("https://{debug}twitch.meandtheboys.c0d3m4513r.com/")
}

pub(crate) const TWITCH_AUTH_PATH: &str = "twitch_authentications.json";
pub(in super) async fn create_twitch_client(rocket: rocket::Rocket<rocket::Build>) -> ::anyhow::Result<(rocket::Rocket<rocket::Build>, std::sync::Arc<Twitch>)> {
    let mut auth = TwitchAuthentications::load().await;
    //Get Twitch Client ID and Secret from .env
    let client_id = ::twitch_api::twitch_oauth2::types::ClientId::from(::dotenv::var("TWITCH_CLIENT_ID")?);
    let client_secret = ::twitch_api::twitch_oauth2::types::ClientSecret::from(::dotenv::var("TWITCH_CLIENT_SECRET")?);
    //Create Twitch Client
    let client = reqwest::Client::new();
    let client = twitch_api::HelixClient::with_client(client);
    //Utilize the Client credentials grant flow to get an app access token
    let access_token = twitch_api::twitch_oauth2::tokens::AppAccessToken::get_app_access_token(&client, client_id, client_secret, vec![]).await?;
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
    auth.save().await?;
    let twitch = std::sync::Arc::new(Twitch {
        client,
        access_token,
        conduit,
        csrf_tokens: scc::HashIndex::new(),
        auth,
    });
    //Attach Twitch Client to Rocket as a managed state and Register the Twitch Rocket Callback to finish Conduit setup, once the Webserver is online
    let rocket = rocket
        .manage(twitch.clone())
    ;
    let rocket = if created_conduit {
        rocket.attach(rocket_callback::TwitchRocketCallback{client: twitch.clone()})
    } else {
        rocket
    };

    Ok((rocket, twitch))
}
impl Twitch {
    pub(crate) async fn get_new_oath(&self) -> Result<String, String> {
        match &*TWITCH_OAUTH {
            Ok(v) => {
                let time = std::time::Instant::now();
                let mut csrf:[u8; CSRF_TOKEN_LENGTH] = rand::random();
                loop {
                    match self.csrf_tokens.insert_async(csrf, time).await{
                        Ok(_) => break,
                        Err(_) => {
                            //Collision?, generate a new token
                            csrf = rand::random();
                        }
                    }
                }
                let token = base64::engine::general_purpose::URL_SAFE.encode(&csrf);
                Ok(format!("https://id.twitch.tv/oauth2/authorize?redirect_uri={}&response_type=code&client_id={}&scope=&state={token}", v.url, v.client_id))
            }
            Err(e) => {
                Err(e.clone())
            }
        }
    }
}