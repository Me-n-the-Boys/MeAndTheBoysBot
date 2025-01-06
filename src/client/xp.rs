use poise::{serenity_prelude as serenity};

impl super::Handler {
    /// Tries to apply the voice channel xp to the user, if applicable.
    pub(crate) async fn try_apply_vc_xp(&self, user_id: serenity::UserId) {
        let start = {
            let mut lock = self.xp_vc_tmp.lock().await;
            lock.remove(&user_id)
        };
        if let Some(start) = start {
            let instant = tokio::time::Instant::now();
            let duration = start - instant;
            let seconds = duration.as_secs();
            {
                let mut lock = self.xp_vc.lock().await;
                let v = lock.entry(user_id).or_default();
                *v+=seconds;
            }
        }
    }
    
    /// Marks a user as being in a voice channel.
    pub(crate) async fn vc_join_xp(&self, channel: serenity::ChannelId, user_id: serenity::UserId) {
        match self.xp_ignored_channels.contains(&channel) {
            true => self.try_apply_vc_xp(user_id).await,
            false => {
                let mut lock = self.xp_vc_tmp.lock().await;
                lock.entry(user_id).or_insert(tokio::time::Instant::now());
            }
        }
    }
}