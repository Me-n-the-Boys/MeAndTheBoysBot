use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct DiscordClient {
    cache: Arc<serenity::cache::Cache>,
    http: Arc<serenity::http::Http>,
}
impl DiscordClient {
    pub fn new(client: &serenity::client::Client) -> Self {
        Self {
            cache: client.cache.clone(),
            http: client.http.clone()
        }
    }
}
impl serenity::http::CacheHttp for DiscordClient {
    fn http(&self) -> &serenity::http::Http {
        &self.http
    }

    fn cache(&self) -> Option<&Arc<serenity::cache::Cache>> {
        Some(&self.cache)
    }
}

impl DiscordClient {
    pub async fn get_channel(&self, channel_id: serenity::model::id::ChannelId) -> Result<serenity::model::channel::Channel, serenity::Error> {
        channel_id.to_channel(&self).await
    }
    pub async fn get_guild(&self, guild_id: serenity::model::id::GuildId) -> Result<serenity::model::guild::PartialGuild, serenity::Error> {
        guild_id.to_partial_guild(&self).await
    }
}