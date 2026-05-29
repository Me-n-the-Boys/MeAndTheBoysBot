create table if not exists twitch_user
(
    id       bigint generated always as identity
        constraint twitch_user_pk
            primary key,
    username text not null
);

create table if not exists guild_channel
(
    guild      bigint  not null
        constraint guild_fk
            references guilds on delete cascade,
    channel_id bigint  not null,
    name   text not null,
    constraint guild_channel_pk
        primary key (guild, channel_id)
)
    partition by HASH (guild);



create table if not exists twitch_channel
(
    guild      bigint  not null
        constraint guild_fk
            references guilds on delete cascade,
    channel_id bigint  not null,
    add_self   boolean not null,
    constraint twitch_channel_pk
        primary key (guild, channel_id),
    constraint twitch_channel_fk
        foreign key (guild, channel_id) REFERENCES guild_channel (guild, channel_id)
)
    partition by HASH (guild);

create table if not exists twitch_channel_user
(
    guild      bigint  not null
        constraint guild_fk
            references guilds on delete cascade,
    channel_id bigint  not null,
    twitch_user   bigint not null,
    constraint twitch_channel_user_pk
        primary key (guild, channel_id, twitch_user),
    constraint twitch_channel_user_fk_guild_chanel
        foreign key (guild, channel_id) REFERENCES guild_channel (guild, channel_id) on delete cascade,
    constraint twitch_channel_user_fk_twitch_user
        foreign key (twitch_user) REFERENCES twitch_user (id) on delete cascade
)
    partition by HASH (guild, twitch_user);

create index if not exists twitch_channel_user_twitch_user_index
    on twitch_channel_user (twitch_user);
