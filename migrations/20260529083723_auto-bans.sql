-- Add migration script here
DROP TABLE IF EXISTS AutoBanChannels;
CREATE TABLE IF NOT EXISTS AutoBanChannels(
    guild_id bigint not null references guilds(guild_id),
    channel bigint not null,
    dmd smallint not null,
    constraint AutoBanChannels_pk primary key (guild_id, channel)
);
ALTER TABLE AutoBanChannels OWNER TO meandtheboysbot;

DROP TABLE IF EXISTS AutoBanRoles;
CREATE TABLE IF NOT EXISTS AutoBanRoles(
    guild_id bigint not null references guilds(guild_id),
    roleid bigint not null,
    dmd smallint not null,
    constraint AutoBanRoles_pk primary key (guild_id, roleid)
);
ALTER TABLE AutoBanRoles OWNER TO meandtheboysbot;