-- Add migration script here
BEGIN;

DO $$ BEGIN
    CREATE TYPE reaction_role_InnerBoolFormula AS (normal bigint[], negated bigint[]);
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;
--https://stackoverflow.com/a/48382296

DO $$ BEGIN
CREATE FUNCTION role_limit_predicate(roles bigint[], bind_roles reaction_role_InnerBoolFormula[] )
    RETURNS bool
    IMMUTABLE
--    LEAKPROOF
    STRICT
    PARALLEL SAFE
    LANGUAGE sql
RETURN (SELECT bool_or(br.normal <@ role_limit_predicate.roles AND NOT br.negated <@ role_limit_predicate.roles ) FROM unnest(role_limit_predicate.bind_roles) as br);
EXCEPTION
    WHEN duplicate_function THEN null;
END $$;

create table IF NOT EXISTS public.role_limiter
(
    guild_id        bigint                   not null
        references public.guilds,
    role_id    bigint                   not null,
    bind_roles      reaction_role_InnerBoolFormula[] default '{}' not null,
    constraint role_limiter_pk
        primary key (guild_id, role_id)
);

create table IF NOT EXISTS public.role_reactions
(
    guild_id        bigint                   not null
        references public.guilds,
    message_id      bigint                   not null,
    give_role_id    bigint                   not null,
    emoji           jsonb                    not null,
    channel_id      bigint                   not null,
    constraint role_reactions_pk
        primary key (guild_id, message_id, give_role_id)
);
COMMIT;