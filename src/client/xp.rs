use poise::{serenity_prelude as serenity};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Default, Copy, Clone, Deserialize, Serialize)]
pub struct TextXpInfo {
    xp: i128,
    #[serde(skip)]
    pending: Option<(u64, tokio::time::Instant)>,
}
impl super::Handler {
    /// Tries to apply the voice channel xp to the user, if applicable.
    pub(crate) async fn try_apply_vc_xp(&self, user_id: serenity::UserId) {
        let start = {
            let mut lock = self.xp_vc_tmp.lock().await;
            lock.remove(&user_id)
        };
        if let Some(start) = start {
            let instant = serenity::Timestamp::now();
            let duration = start.naive_utc() - instant.naive_utc();
            let seconds = duration.num_seconds();
            match u64::try_from(seconds) {
                Ok(seconds) => {
                    let mut lock = self.xp_vc.lock().await;
                    let v = lock.entry(user_id).or_default();
                    *v+=seconds;
                },
                Err(_) => {
                    tracing::warn!("User {} has joined a voice channel for a negative duration. User was in a voice channel for {}second(s). Refusing to delete xp.", user_id, seconds);
                }
            }
        }
    }
    
    /// Marks a user as being in a voice channel.
    pub(crate) async fn vc_join_xp(&self, channel: serenity::ChannelId, user_id: serenity::UserId) {
        match self.xp_ignored_channels.contains(&channel) {
            true => self.try_apply_vc_xp(user_id).await,
            false => {
                let mut lock = self.xp_vc_tmp.lock().await;
                lock.entry(user_id).or_insert(serenity::Timestamp::now());
            }
        }
    }

    pub(crate) async fn message_xp(&self, message: serenity::Message) {
        let time = tokio::time::Instant::now();
        //Ignore message, if sent in an ignored channel or by a bot
        if message.author.bot || message.author.system || self.xp_ignored_channels.contains(&message.channel_id) {
            return;
        }
        //Apply message xp
        {
            let mut lock = self.xp_txt.lock().await;
            let data = lock.entry(message.author.id).or_default();
            let xp = calculate_message_text_xp(BASE_TEXT_XP, &message);
            match data.pending.take() {
                None => {
                    data.pending = Some((xp, time));
                },
                Some((amount, instant)) => {
                    let duration = instant.elapsed();
                    let applyable_xp = duration.as_millis() / u128::max(1, u128::from(self.xp_txt_apply_milliseconds));
                    let applyable_xp = u64::try_from(applyable_xp).unwrap_or(u64::MAX);
                    if amount > applyable_xp {
                        let mut xp_apply_duration = std::time::Duration::from_millis(self.xp_txt_apply_milliseconds);
                        xp_apply_duration = xp_apply_duration.mul_f64(amount as f64);
                        let applyable_xp = i128::from(applyable_xp);
                        if xp_apply_duration > std::time::Duration::from_secs(self.xp_txt_punish_seconds) {
                            let amount = i128::from(amount);
                            let over_xp = amount - applyable_xp;
                            data.xp = data.xp.saturating_sub(over_xp);
                            tracing::warn!("User {} has triggered the xp spam limit. Queued are {amount} xp from {duration:?} ago. {applyable_xp} xp are applyable. {over_xp} xp are removed.", message.author.id);
                        } else {
                            let (tmp_xp, overflow) = amount.overflowing_add(xp);
                            if !overflow {
                                data.pending = Some((tmp_xp, instant));
                            } else {
                                data.xp = data.xp.saturating_sub(i128::from(tmp_xp));
                                let tmp_xp_1 = u64::MAX;
                                data.xp = data.xp.saturating_sub(i128::from(tmp_xp_1) - applyable_xp);
                                data.pending = Some((tmp_xp_1, instant));
                                tracing::warn!("User {} has EXTREME xp gain (2^64+{tmp_xp} text xp outstanding from the last {duration:?}). {tmp_xp}+{tmp_xp_1}-{applyable_xp} text xp have been removed.", message.author.id);
                            }
                        }
                    } else {
                        data.xp += i128::from(amount);
                    }
                }
            }
        };
    }
}

const BASE_TEXT_XP: u64 = 10;
fn calculate_message_text_xp(base_xp: u64, message: &serenity::Message) -> u64 {
    let mut xp = base_xp as f64;
    xp += message.content.len() as f64 / 1000. * xp;
    xp += message.attachments.len() as f64 * 0.25 * xp;
    xp += message.embeds.len() as f64 * 0.1 * xp;
    xp as u64
}