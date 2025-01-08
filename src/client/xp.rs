use poise::{serenity_prelude as serenity};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Default, Copy, Clone, Deserialize, Serialize)]
pub struct TextXpInfo {
    xp: i128,
    pending: Option<(u64, serenity::Timestamp)>,
}

impl super::Handler {
    async fn try_apply_vc_xp_timestamp(&self, start: serenity::Timestamp, instant:serenity::Timestamp, user_id: serenity::UserId) {
        let duration = start.naive_utc() - instant.naive_utc();
        let seconds = duration.num_seconds().abs();
        match u64::try_from(seconds) {
            Ok(seconds) => {
                let mut lock = self.xp_vc.lock().await;
                let v = lock.entry(user_id).or_default();
                *v = v.saturating_add(seconds);
            },
            Err(_) => {
                tracing::warn!("User {} has joined a voice channel for a negative duration. User was in a voice channel for {}second(s). Refusing to delete xp.", user_id, seconds);
            }
        }
    }
    /// Tries to apply the voice channel xp to the user, if applicable.
    pub(in super) async fn try_apply_vc_xp(&self, user_id: serenity::UserId) {
        let start = {
            let mut lock = self.xp_vc_tmp.lock().await;
            lock.remove(&user_id)
        };
        if let Some(start) = start {
            self.try_apply_vc_xp_timestamp(start, serenity::Timestamp::now(), user_id).await;
        }
    }
    
    /// Marks a user as being in a voice channel.
    pub(in super) async fn vc_join_xp(&self, channel: serenity::ChannelId, user_id: serenity::UserId) {
        match self.xp_ignored_channels.contains(&channel) {
            true => self.try_apply_vc_xp(user_id).await,
            false => {
                let mut lock = self.xp_vc_tmp.lock().await;
                lock.entry(user_id).or_insert_with(serenity::Timestamp::now);
            }
        }
    }

    fn apply_previous_message_xp(&self, user_id: serenity::UserId, data: &mut TextXpInfo, now: serenity::Timestamp) {
        match data.pending.take() {
            None => {},
            Some((amount, instant)) => {
                let duration = instant.naive_utc() - now.naive_utc();
                let applyable_xp = duration.num_milliseconds().abs() / i64::from(self.xp_txt_apply_milliseconds);
                if i128::from(amount) > i128::from(applyable_xp) {
                    let mut xp_apply_duration = std::time::Duration::from_millis(u64::from(self.xp_txt_apply_milliseconds));
                    xp_apply_duration = xp_apply_duration.mul_f64(amount as f64);
                    if xp_apply_duration > std::time::Duration::from_secs(self.xp_txt_punish_seconds) {
                        let applyable_xp = i128::from(applyable_xp);
                        let amount = i128::from(amount);
                        let over_xp = amount - applyable_xp;
                        data.xp = data.xp.saturating_add(over_xp);
                        tracing::warn!("User {user_id} has triggered the xp spam limit. Queued are {amount} xp from {duration:?} ago. {applyable_xp} xp are applyable. {over_xp} xp are removed.");
                    } else {
                        let amount = amount.saturating_add_signed(-applyable_xp);
                        let applyable_xp = i128::from(applyable_xp);
                        data.xp = data.xp.saturating_add(applyable_xp);
                        data.pending = Some((amount, now));
                        tracing::info!("User {user_id} has gotten an unusual amount of xp. Queued outstanding xp for application. Queued are {amount} xp. {applyable_xp} xp were already applied");
                    }
                } else {
                    data.xp = data.xp.saturating_add(i128::from(amount));
                }
            }
        }
    }

    pub(in super) async fn message_xp(&self, message: serenity::Message) {
        let time = serenity::Timestamp::now();
        //Ignore message, if sent in an ignored channel or by a bot
        if message.author.bot || message.author.system || self.xp_ignored_channels.contains(&message.channel_id) {
            return;
        }
        //Apply message xp
        {
            let mut lock = self.xp_txt.lock().await;
            let xp = calculate_message_text_xp(BASE_TEXT_XP, &message);
            let data = lock.entry(message.author.id).or_default();
            self.apply_previous_message_xp(message.author.id, data, time);
            data.pending = Some(data.pending.take().map_or_else(||(xp, time), |(v, time)|(v.saturating_add(xp), time)));
        };
    }
    pub(crate) async fn xp_incremental_check(&self) {
        let time = serenity::Timestamp::now();
        //apply voice xp
        {
            let mut lock = self.xp_vc_tmp.lock().await;
            for (user_id, start) in lock.iter_mut() {
                let start = core::mem::replace(start, time);
                self.try_apply_vc_xp_timestamp(start, time, *user_id).await;
            }
        }
        tracing::info!("Incremental Check: Voice Xp applied");
        //apply text xp
        {
            let mut lock = self.xp_txt.lock().await;
            for (user_id, start) in lock.iter_mut() {
                self.apply_previous_message_xp(*user_id, start, time);
            }
        }
        tracing::info!("Incremental Check: Text Xp applied");
    }
}

const BASE_TEXT_XP: u64 = 10;
fn calculate_message_text_xp(base_xp: u64, message: &serenity::Message) -> u64 {
    let mut xp = base_xp as f64;
    xp *=
        1. +
        message.content.len() as f64 / 1000. +
        message.attachments.len() as f64 * 0.25 +
        message.embeds.len() as f64 * 0.1;

    xp as u64
}