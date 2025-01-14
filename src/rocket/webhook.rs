use hmac::Mac;
use rocket::{Data, Request};
use crate::twitch_client::TWITCH_WS_SECRET;

macro_rules! twitch_header {
    ($ident: ident, $request: ident,  $name: literal) => {
        twitch_header!($ident, $request, $name, {$ident});
    };
    ($ident: ident, $request: ident, $name: literal, $extra_valiation:expr) => {
        let $ident = {
            let mut header = $request.headers()
                .get($name);
            let $ident = match header.next() {
                None => return rocket::data::Outcome::Error((rocket::http::Status::BadRequest, concat!("Missing ", $name, " Header"))),
                Some(value) => value
            };
            if header.count() > 0 {
                return rocket::data::Outcome::Error((rocket::http::Status::BadRequest, concat!("Multiple ", $name, " Headers")));
            }
            let $ident = $extra_valiation;
            $ident
        };
    };
}
#[derive(Debug)]
pub(super) struct TwitchEventsubMessage<'r> {
    id: &'r str,
    timestamp: chrono::DateTime<chrono::FixedOffset>,
    body: twitch_api::eventsub::Event,
}
#[rocket::async_trait]
impl<'r> rocket::data::FromData<'r> for TwitchEventsubMessage<'r> {
    type Error = &'r str;

    async fn from_data(request: &'r Request<'_>, data: Data<'r>) -> rocket::data::Outcome<'r, Self> {
        const SHA_HEADER: &str = "sha256=";
        twitch_header!(id, request, "Twitch-Eventsub-Message-Id");
        twitch_header!(timestamp, request, "Twitch-Eventsub-Message-Timestamp");
        twitch_header!(signature, request, "Twitch-Eventsub-Message-Signature", {
            match signature.strip_prefix(SHA_HEADER) {
                None => return rocket::data::Outcome::Error((rocket::http::Status::Unauthorized, "Invalid Signature")),
                Some(signature) => signature
            }
        });

        let key = hmac::digest::Key::<hmac::Hmac::<sha2::Sha256>>::from(*TWITCH_WS_SECRET);
        let mut hmac = hmac::Hmac::<sha2::Sha256>::new(&key);
        hmac.update(id.as_bytes());
        hmac.update(timestamp.as_bytes());
        let timestamp: chrono::DateTime::<chrono::FixedOffset> = match chrono::DateTime::parse_from_rfc3339(timestamp) {
            Ok(timestamp) => timestamp,
            Err(_) => return rocket::data::Outcome::Error((rocket::http::Status::BadRequest, "Twitch-Eventsub-Message-Timestamp is not in RFC3339 format")),
        };
        let limit = request.limits().get("json").unwrap_or(rocket::data::Limits::JSON);
        let string = match data.open(limit).into_string().await {
            Ok(s) if s.is_complete() => s.into_inner(),
            Ok(_) => return rocket::data::Outcome::Error((rocket::http::Status::PayloadTooLarge, "data limit exceeded")),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof || e.kind() == std::io::ErrorKind::OutOfMemory {
                    return rocket::data::Outcome::Error((rocket::http::Status::PayloadTooLarge, "data limit exceeded"));
                }
                tracing::error!("Failed to read data: Error: {e:?}, Request: {request:?}");
                return rocket::data::Outcome::Error((rocket::http::Status::InternalServerError, "Failed to read data"));
            },
        };
        hmac.update(string.as_bytes());
        if let Err(_) = hmac.verify_slice(signature.as_bytes()) {
            return rocket::data::Outcome::Error((rocket::http::Status::Unauthorized, "Invalid Signature"));
        }

        let body: twitch_api::eventsub::Event = match serde_json::from_str(&string) {
            Ok(body) => body,
            Err(_) => return rocket::data::Outcome::Error((rocket::http::Status::InternalServerError, "Failed to parse JSON")),
        };

        rocket::data::Outcome::Success(TwitchEventsubMessage {
            id,
            timestamp,
            body,
        })
    }
}

#[rocket::post("/twitch/eventsub", data="<twitch_event>")]
pub(in super) async fn webhook<'r>(twitch_event: TwitchEventsubMessage<'r>) -> (rocket::http::Status, String) {
    if let Some(verification) = twitch_event.body.get_verification_request() {
        return (rocket::http::Status::Ok, verification.challenge.clone());
    }

    match twitch_event.body {
        twitch_api::eventsub::Event::StreamOnlineV1(event) => {
            match event.message{
                twitch_api::eventsub::Message::VerificationRequest(event) => {
                    (rocket::http::Status::Ok, event.challenge.clone())
                },
                twitch_api::eventsub::Message::Notification(event) => {
                    //TODO: Handle
                    (rocket::http::Status::NoContent, String::new())
                },
                twitch_api::eventsub::Message::Revocation() => {
                    //TODO: Handle
                    (rocket::http::Status::NoContent, String::new())
                },
                other => {
                    tracing::info!("Received unhandled twitch Webhook Message: {other:?}");
                    (rocket::http::Status::NoContent, String::new())
                }
            }
        }
        event => {
            tracing::info!("Received unhandled twitch Webhook Event: {event:?}");
            (rocket::http::Status::NoContent, String::new())
        }
    }
}