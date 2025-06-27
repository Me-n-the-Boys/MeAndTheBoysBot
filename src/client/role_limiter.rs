use serde_derive::{Serialize, Deserialize};
use serenity::all::Member;
use serenity::client::Context;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BindRoles {
    pub bind: Vec<BindRolesOrs>
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BindRolesOrs {
    pub normal: Vec<i64>,
    pub negated: Vec<i64>,
}

pub async fn handle_role_change(ctx: &Context, member: &Member) {
    let roles = member.roles.iter().map(|v|v.get().cast_signed()).collect::<Vec<_>>();
    let db = crate::get_db().await;
    match sqlx::query!(
        "SELECT role_id FROM role_limiter WHERE guild_id = $1 AND role_id = ANY($2) AND NOT role_limit_predicate($2, bind_roles)",
        member.guild_id.get().cast_signed(), roles.as_slice()
    )
        .fetch_all(&db)
        .await 
    {
        Ok(v) => {
            let roles:Vec<_> = v.iter().map(|v|v.role_id.cast_unsigned().into()).collect();
            match member.remove_roles(ctx, roles.as_slice()).await {
                Ok(()) => {},
                Err(err) => {
                    log::error!("Failed to remove member roles: {err}");
                }
            }
        },
        Err(err) => {
            log::error!("Could not handle role change for guild {} and user {member}: {err}", member.guild_id);
            return;
        }
    }
    
}