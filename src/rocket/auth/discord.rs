pub mod token;

pub const OAUTH_URL: &'static str = const_format::concatcp!(super::super::BASE_SCHEME, "://", super::super::BASE_URL, "/discord/oauth");
pub const NEW_OAUTH_URL: &'static str = const_format::concatcp!(super::super::BASE_SCHEME, "://", super::super::BASE_URL, "/discord/new_oauth");
#[derive(Debug)]
#[non_exhaustive]
pub struct Discord {
    pub client_id: String,
    client_secret: String,
    client: reqwest::Client,
    pub auth: DiscordAuthentications,
}

impl Discord {
    pub async fn new() -> ::anyhow::Result<Self> {
        let auth = DiscordAuthentications::load().await;
        //Get Twitch Client ID and Secret from .env
        let client_id = ::dotenv::var("DISCORD_CLIENT_ID")?;
        let client_secret = ::dotenv::var("DISCORD_CLIENT_SECRET")?;

        Ok(Self{
            client_id,
            client_secret,
            client: reqwest::Client::new(),
            auth,
        })
    }
}
const TOKEN_LENGTH: usize = 32;
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct DiscordAuthentications {
    pub(in super::super) tokens: scc::HashMap<[u8;TOKEN_LENGTH], token::Token>
}
pub(crate) const DISCORD_AUTH_PATH: &str = "discord_authentications.json";
impl DiscordAuthentications{

    async fn load() -> Self {
        match tokio::fs::read_to_string(DISCORD_AUTH_PATH).await {
            Ok(v) => serde_json::from_str(v.as_str()).unwrap_or_else(|err| {
                tracing::info!("Failed to parse Discord Authentications: {err}");
                Default::default()
            }),
            Err(err) => {
                tracing::info!("Failed to read Discord Authentications file: {err}");
                Default::default()
            }
        }
    }
    pub(crate) async fn save(&self) -> ::anyhow::Result<()> {
        let auth = serde_json::to_string(&self)?;
        match tokio::fs::write(DISCORD_AUTH_PATH, auth).await {
            Ok(()) => {
                tracing::info!("Saved Discord Authentications");
            },
            Err(err) => {
                tracing::error!("Failed to save Discord Authentications: {err}");
            }
        }
        Ok(())
    }

    pub(crate) async fn insert_new_token(&self, mut token: token::Token) -> [u8;TOKEN_LENGTH] {
        let mut key:[u8;TOKEN_LENGTH] = rand::random();
        loop {
            match self.tokens.insert_async(key, token).await {
                Ok(()) => return key,
                Err((_, token1)) => {
                    token = token1;
                    key = rand::random();
                }
            }
        }
    }
}

