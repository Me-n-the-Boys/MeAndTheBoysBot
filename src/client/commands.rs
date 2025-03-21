mod settings;

use poise::CreateReply;
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, super::Data, Error>;

///Gets the latency of the current shard to discord's gateway.
#[poise::command(
    slash_command,
)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    const CONVERSION_STEP:u32 = 1000;
    let time = ctx.ping().await;
    let ns = time.subsec_nanos();
    //get submicro_nanos
    let us = ns/CONVERSION_STEP;
    let ns = ns - us *CONVERSION_STEP;
    //get submilli_micros
    let ms = us /CONVERSION_STEP;
    let us = us - ms*CONVERSION_STEP;
    //get subsec_milli
    ctx.send(CreateReply::default().content(format!("Pong! Shard {}'s Latency to Gateway: {}s{ms}ms{us}µs{ns}ns", ctx.serenity_context().shard_id, time.as_secs(), )).ephemeral(true).reply(true)).await?;
    Ok(())
}

pub use settings::settings;


///Copies one emoji to the guild the command was run in (5 min cooldown).
#[poise::command(
    slash_command,
    guild_only,
    user_cooldown = 300,
    default_member_permissions = "MANAGE_GUILD_EXPRESSIONS",
    required_bot_permissions = "MANAGE_GUILD_EXPRESSIONS",
)]
pub async fn copy_emoji(ctx: Context<'_>, #[description = "Exactly ONE Discord emoji"] #[min_length = 1] emoji: String, #[description = "Name of the emoji in the guild"] #[min_length = 2] name: Option<String>) -> Result<(), Error> {
    let emoji_p = match serenity::utils::parse_emoji(emoji) {
        Some(v) => v,
        None => {
            ctx.send(CreateReply::default().content("Expected the emoji argument to contain exactly one discord emoji. Please remove any other content, such as extra spaces in-front or behind the emoji.").ephemeral(true).reply(true)).await?;
            return Ok(());
        }
    };
    let name = name.unwrap_or_else(||emoji_p.name.clone());

    if emoji_p.name.chars().any(|c| !c.is_alphanumeric() && c != '_') {
        ctx.send(CreateReply::default().content("The name of the emoji must be at least 2 characters long and can only contain alphanumeric characters and underscores.").ephemeral(true).reply(true)).await?;
        return Ok(());
    }

    let guild = match ctx.guild().map_or(Err("This command must be run in a guild."), |guild|{
        if guild.emojis.get(&emoji_p.id).is_some() {
            return Err("This emoji comes from the guild, in which this command was ran.");
        }

        if guild.emojis.values().filter(|v|v.name == name).any(|_|true) {
            return Err("This emoji comes from the guild, in which this command was ran.");
        }

        Ok(guild.id)
    }) {
        Err(err) => {
            ctx.send(CreateReply::default().content(format!("Error getting emojis: {err}")).ephemeral(true).reply(true)).await?;
            return Ok(());
        }
        Ok(v) => v
    };

    let image = match reqwest::Client::new().get(emoji_p.url()).send().await {
        Ok(v) => match v.bytes().await {
            Ok(v) => v,
            Err(err) => {
                ctx.send(CreateReply::default().content(format!("Error getting emoji (request content): {err}")).ephemeral(true).reply(true)).await?;
                return Ok(());
            }
        },
        Err(err) => {
            ctx.send(CreateReply::default().content(format!("Error getting emoji (request): {err}")).ephemeral(true).reply(true)).await?;
            return Ok(());
        }
    };
    let image = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, image.as_ref());
    let image = if emoji_p.animated {
        format!("data:image/gif;base64,{image}")
    } else {
        format!("data:image/png;base64,{image}")
    };
    match guild.create_emoji(&ctx, &name, image.as_str()).await{
        Ok(emoji) => {
            ctx.send(CreateReply::default().content(
                format!(
                    "Copied emoji: <{}:{}:{}>",
                    if emoji.animated {"a"} else {""},
                    emoji.name,
                    emoji.id.get()
                )
            ).reply(true)).await?;
        }
        Err(err) => {
            ctx.send(CreateReply::default().content(format!("Error creating emoji: {err}")).ephemeral(true).reply(true)).await?;
        }
    }
    Ok(())
}