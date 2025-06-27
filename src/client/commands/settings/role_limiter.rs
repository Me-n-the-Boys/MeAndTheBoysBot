use std::sync::LazyLock;
use serenity::all::{ChunkGuildFilter, RoleId};
use crate::client::commands::{Context, Error};
use crate::client::role_limiter::{BindRoles, BindRolesOrs};

///Various commands for changing some settings.
#[poise::command(
    slash_command,
    subcommands(
        "add",
        "list",
        "refresh",
        "remove",
    ),
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
    subcommand_required,
)]
pub async fn role_limiter(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(
    slash_command,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
)]
///Add a limitation about who can have a certain role (expressed in a Disjunctive normal form formula)
pub async fn add(ctx: Context<'_>, role: RoleId, #[description = "! = NOT, & = AND, | = OR"]bind_roles: String) -> Result<(), Error> {
    static REMOVE_WHITESPACE:LazyLock<regex::Regex> =  LazyLock::new(||regex::Regex::new(r#"[\s<@&>]*"#).expect("Invalid Regex"));
    let bind_roles = REMOVE_WHITESPACE.replace_all(&bind_roles, "");
    let mut bind_roles_parsed = BindRoles::default();
    for and_items in bind_roles.split("|") {
        let mut items = BindRolesOrs::default();
        for or_item in and_items.split("&") {
            let (inverted, role) = or_item.strip_prefix("!").map_or_else(||(false, or_item), |v|(true, v));
            log::debug!("Parsing role: {role}");
            let role:RoleId = role.parse()?;
            let role = role.get().cast_signed();
            if inverted {
                items.negated.push(role);
            } else {
                items.normal.push(role);
            }
        }
        bind_roles_parsed.bind.push(items);
    }
    let db = crate::get_db().await;
    let guild_id = match ctx.guild_id() {
        Some(v) => v,
        None => return Err("This command needs to be run from a guild".into())
    };
    let bound_roles = serde_json::to_value(&bind_roles_parsed).map_err(|v|anyhow::format_err!("Could not serialize bind roles: {v}"))?;
    sqlx::query!(r#"INSERT INTO public.role_limiter (guild_id, role_id, bind_roles) VALUES ($1, $2, (SELECT bind from jsonb_to_record($3) as t(bind reaction_role_InnerBoolFormula[]) ))"#, guild_id.get().cast_signed(), role.get().cast_signed(), bound_roles)
        .execute(&db)
        .await?;
    ctx.say("Added the new Role-Reaction. It has not yet been applied to guild members.").await?;

    Ok(())
}
#[poise::command(
    slash_command,
    ephemeral,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
)]
///Add a limitation about who can have a certain role (expressed in a Disjunctive normal form formula)
pub async fn list(ctx: Context<'_>, role_id: Vec<RoleId>) -> Result<(), Error> {
    let db = crate::get_db().await;
    let guild_id = match ctx.guild_id() {
        Some(v) => v,
        None => return Err("This command needs to be run from a guild".into())
    };
    let role_id = role_id.into_iter().map(|v|v.get().cast_signed()).collect::<Vec<_>>();
    let roles = sqlx::query!(r#"
SELECT role_id, array_to_string(
        array(SELECT
                  '( ' || array_to_string(array_cat(
                          array((SELECT '<@&'||v||'>' FROM unnest(t.normal) as v)),
                          array((SELECT '!<@&'||v||'>' FROM unnest(t.negated) as v))
                  ), ' & ') || ' )'
              FROM unnest(bind_roles) as t
        ), ' | ') as "bind_roles!"
FROM role_limiter
WHERE guild_id = $1 AND (cardinality($2::bigint[]) = 0  OR role_id = ANY($2))"#, guild_id.get().cast_signed(), role_id.as_slice())
        .fetch_all(&db)
        .await?
        .into_iter()
        .fold(String::new(), |mut init, item|{
            let role_id = item.role_id;
            let bind_roles = item.bind_roles;
            init.push_str(&format!("- <@&{role_id}> is bound to: {bind_roles}\n"));
            init
        });
    ctx.say(format!("The following Role-Limits exist: \n{roles}")).await?;

    Ok(())
}
#[poise::command(
    slash_command,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
    guild_cooldown = 900,
)]
/// Removes a Limits about who can acquire what Roles
pub async fn refresh(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = match ctx.guild_id() {
        Some(v) => v,
        None => return Err("This command needs to be run from a guild".into())
    };
    ctx.serenity_context().shard.chunk_guild(guild_id, None, false, ChunkGuildFilter::None, None);
    ctx.say("Started a Refresh of the Role Limiter").await?;
    Ok(())
}

#[poise::command(
    slash_command,
    default_member_permissions = "MANAGE_GUILD",
    required_permissions = "MANAGE_GUILD",
)]
/// Removes a Limits about who can acquire what Roles
pub async fn remove(ctx: Context<'_>, #[min_length = 1] roles: Vec<RoleId>) -> Result<(), Error> {
    let db = crate::get_db().await;
    let guild_id = match ctx.guild_id() {
        Some(v) => v,
        None => return Err("This command needs to be run from a guild".into())
    };
    let roles = roles.into_iter().map(|v|v.get().cast_signed()).collect::<Vec<_>>();
    sqlx::query!(
        r#"DELETE FROM public.role_limiter WHERE guild_id = $1 AND role_id = ANY($2)"#,
        guild_id.get().cast_signed(), roles.as_slice(),
    )
        .execute(&db)
        .await?;
    let roles = roles.iter().fold(String::new(), |mut init, role|{
        init.push_str(&format!("<@&{role}>"));
        init
    });
    ctx.say(format!("Removed all Role Limits for the roles: {roles}")).await?;
    Ok(())
}