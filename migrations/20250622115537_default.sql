-- Add migration script here
--
-- PostgreSQL database dump
--

-- Dumped from database version 17.5
-- Dumped by pg_dump version 17.5
SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
-- SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: meandtheboysbot; Type: DATABASE; Schema: -; Owner: postgres
--

-- CREATE DATABASE meandtheboysbot WITH TEMPLATE = template0 ENCODING = 'UTF8' LOCALE_PROVIDER = libc LOCALE = 'en_US.UTF-8';


-- ALTER DATABASE meandtheboysbot OWNER TO postgres;

-- USE meandtheboysbot;

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
-- SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: public; Type: SCHEMA; Schema: -; Owner: pg_database_owner
--

--CREATE SCHEMA IF NOT EXISTS public;


--ALTER SCHEMA public OWNER TO pg_database_owner;

--
-- Name: SCHEMA public; Type: COMMENT; Schema: -; Owner: pg_database_owner
--

--COMMENT ON SCHEMA public IS 'standard public schema';


--
-- Name: apply_previous_message_xp(bigint, bigint); Type: FUNCTION; Schema: public; Owner: postgres
--
DO $$ BEGIN
    CREATE FUNCTION public.apply_previous_message_xp(guild_id bigint, user_id bigint) RETURNS TABLE(guild_id bigint, user_id bigint, txt_apply_interval interval, txt_punish_interval interval, total_xp bigint, applyable_xp bigint, xp_change bigint, xp_punish boolean, duration interval)
        LANGUAGE sql
        --LEAKPROOF
        AS '
    WITH xp AS (
        WITH xp AS (WITH xp AS (
            SELECT
                xp.*,
                xp_txt_tmp.user_id,
                xp_txt_tmp.xp as total_xp,
                GREATEST(xp_txt_tmp.xp, EXTRACT(EPOCH FROM (now() - xp_txt_tmp.time))/EXTRACT(EPOCH FROM xp.txt_apply_interval)) as applyable_xp,
                xp_txt_tmp.time,
                xp_txt_tmp.xp * xp.txt_apply_interval > xp.txt_punish_interval as xp_punish
            FROM xp
                     JOIN xp_txt_tmp ON xp_txt_tmp.guild_id = xp.guild_id
            WHERE CASE WHEN apply_previous_message_xp.guild_id IS NOT NULL THEN xp.guild_id = apply_previous_message_xp.guild_id ELSE TRUE END
        ) SELECT
              xp.guild_id,
              xp.user_id,
              xp.txt_apply_interval,
              xp.txt_punish_interval,
              xp.total_xp,
              xp.applyable_xp,
              xp.applyable_xp - CASE WHEN xp.xp_punish THEN (xp.total_xp - xp.applyable_xp) ELSE 0 END as xp_change,
              xp.xp_punish,
              now() - xp.time as duration
        FROM xp
        )

            UPDATE xp_user SET txt = xp_user.txt + xp.xp_change FROM xp WHERE xp_user.guild_id = xp.guild_id AND xp_user.user_id = xp.user_id
                AND CASE WHEN apply_previous_message_xp.user_id IS NOT NULL THEN xp_user.user_id = apply_previous_message_xp.user_id ELSE TRUE END
                RETURNING xp.*
        ) MERGE INTO xp_txt_tmp USING xp ON xp_txt_tmp.guild_id = xp.guild_id AND xp_txt_tmp.user_id = xp.user_id
    WHEN MATCHED AND xp.xp_punish OR xp.xp_change = xp.total_xp THEN DELETE
    WHEN MATCHED THEN UPDATE SET xp = xp.total_xp - xp.xp_change, time = now()
                          WHEN NOT MATCHED THEN INSERT (guild_id, user_id, xp, time) VALUES (xp.guild_id, xp.user_id, xp.total_xp - xp.xp_change, now())
                          RETURNING xp.*
                          ';
EXCEPTION
    WHEN duplicate_function THEN null;
END $$;



--ALTER FUNCTION public.apply_previous_message_xp(guild_id bigint, user_id bigint) OWNER TO postgres;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: guild_user; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.guild_user (
                                   guild_id bigint NOT NULL,
                                   user_id bigint NOT NULL,
                                   nickname text,
                                   avatar text,
                                   CONSTRAINT guild_user_pk PRIMARY KEY (guild_id, user_id),
                                   CONSTRAINT guild_user_guilds_guild_id_fk FOREIGN KEY (guild_id) REFERENCES public.guilds(guild_id),
                                   CONSTRAINT guild_user_users_id_fk FOREIGN KEY (user_id) REFERENCES public.users(id)
);


--ALTER TABLE public.guild_user OWNER TO postgres;

--
-- Name: guilds; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.guilds (
                               guild_id bigint NOT NULL,
                               name text,
                               icon text,
                               CONSTRAINT guilds_pk PRIMARY KEY (guild_id)
);


--ALTER TABLE public.guilds OWNER TO postgres;

--
-- Name: temp_channels; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.temp_channels (
                                      guild_id bigint NOT NULL,
                                      creator_channel bigint NOT NULL,
                                      create_category bigint,
                                      delete_delay interval DEFAULT '00:00:15'::interval NOT NULL,
                                      delete_non_created_channels boolean DEFAULT false NOT NULL,
                                      CONSTRAINT temp_channels_pkey PRIMARY KEY (guild_id),
                                      CONSTRAINT temp_channels_guilds_guild_id_fk FOREIGN KEY (guild_id) REFERENCES public.guilds(guild_id)
);


--ALTER TABLE public.temp_channels OWNER TO postgres;

--
-- Name: temp_channels_created; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.temp_channels_created (
                                              guild_id bigint NOT NULL,
                                              channel_id bigint NOT NULL,
                                              mark_delete timestamp without time zone,
                                              name text NOT NULL,
                                              CONSTRAINT temp_channels_created_pk PRIMARY KEY (guild_id, channel_id),
                                              CONSTRAINT temp_channels_created_guilds_guild_id_fk FOREIGN KEY (guild_id) REFERENCES public.guilds(guild_id)
);


--ALTER TABLE public.temp_channels_created OWNER TO postgres;

--
-- Name: temp_channels_created_users; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.temp_channels_created_users (
                                                    guild_id bigint NOT NULL,
                                                    channel_id bigint NOT NULL,
                                                    user_id bigint NOT NULL,
                                                    CONSTRAINT temp_channels_created_users_pk PRIMARY KEY (guild_id, channel_id, user_id),
                                                    CONSTRAINT temp_channels_created_users_pk_2 UNIQUE (guild_id, user_id),
                                                    CONSTRAINT temp_channels_created_users_temp_channels_created_guild_id_chan FOREIGN KEY (guild_id, channel_id) REFERENCES public.temp_channels_created(guild_id, channel_id) ON UPDATE CASCADE ON DELETE CASCADE
);


--ALTER TABLE public.temp_channels_created_users OWNER TO postgres;

--
-- Name: temp_channels_ignore; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.temp_channels_ignore (
                                             guild_id bigint NOT NULL,
                                             channel_id bigint NOT NULL,
                                             CONSTRAINT temp_channels_ignore_pk PRIMARY KEY (guild_id, channel_id),
                                             CONSTRAINT temp_channels_ignore_guilds_guild_id_fk FOREIGN KEY (guild_id) REFERENCES public.guilds(guild_id)
);


--ALTER TABLE public.temp_channels_ignore OWNER TO postgres;

--
-- Name: users; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.users (
                              id bigint NOT NULL,
                              display_name text,
                              avatar text,
                              CONSTRAINT users_pk PRIMARY KEY (id),
                              CONSTRAINT users_users_username_id_fk FOREIGN KEY (id) REFERENCES public.users_username(id)
);


--ALTER TABLE public.users OWNER TO postgres;

--
-- Name: users_username; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.users_username (
                                       id bigint NOT NULL,
                                       username text NOT NULL,
                                       discriminator smallint,
                                       CONSTRAINT check_username_valid CHECK (((discriminator IS NULL) OR (discriminator > 0))),
                                       CONSTRAINT users_username_pk PRIMARY KEY (id),
                                       CONSTRAINT users_username_pk_2 UNIQUE (username, discriminator),
                                       CONSTRAINT users_username_users_id_fk FOREIGN KEY (id) REFERENCES public.users(id) ON UPDATE CASCADE ON DELETE CASCADE
);


--ALTER TABLE public.users_username OWNER TO postgres;

--
-- Name: xp; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.xp (
                           txt_apply_interval interval DEFAULT '00:00:00.05'::interval NOT NULL,
                           txt_punish_interval interval DEFAULT '00:02:00'::interval NOT NULL,
                           guild_id bigint NOT NULL,
                           CONSTRAINT xp_pk PRIMARY KEY (guild_id),
                           CONSTRAINT xp_guilds_guild_id_fk FOREIGN KEY (guild_id) REFERENCES public.guilds(guild_id)
);


--ALTER TABLE public.xp OWNER TO postgres;

--
-- Name: xp_channels_ignored; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.xp_channels_ignored (
                                            guild_id bigint NOT NULL,
                                            channel_id bigint NOT NULL,
                                            CONSTRAINT xp_channels_ignored_pk PRIMARY KEY (guild_id, channel_id),
                                            CONSTRAINT xp_channels_ignored_guilds_guild_id_fk FOREIGN KEY (guild_id) REFERENCES public.guilds(guild_id)
);


--ALTER TABLE public.xp_channels_ignored OWNER TO postgres;

--
-- Name: xp_txt_tmp; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.xp_txt_tmp (
                                   guild_id bigint NOT NULL,
                                   user_id bigint NOT NULL,
                                   xp bigint DEFAULT 0 NOT NULL,
                                   "time" timestamp without time zone DEFAULT now() NOT NULL,
                                   CONSTRAINT xp_txt_tmp_pk PRIMARY KEY (guild_id, user_id),
                                   CONSTRAINT xp_txt_tmp_guilds_guild_id_fk FOREIGN KEY (guild_id) REFERENCES public.guilds(guild_id),
                                   CONSTRAINT xp_vc_tmp_guilds_guild_id_fk FOREIGN KEY (guild_id) REFERENCES public.guilds(guild_id)
);


--ALTER TABLE public.xp_txt_tmp OWNER TO postgres;

--
-- Name: xp_user; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.xp_user (
                                guild_id bigint NOT NULL,
                                user_id bigint NOT NULL,
                                txt bigint DEFAULT 0 NOT NULL,
                                vc interval DEFAULT '00:00:00'::interval NOT NULL,
                                CONSTRAINT xp_user_pk PRIMARY KEY (guild_id, user_id),
                                CONSTRAINT xp_user_guilds_guild_id_fk FOREIGN KEY (guild_id) REFERENCES public.guilds(guild_id)
);


--ALTER TABLE public.xp_user OWNER TO postgres;

--
-- Name: xp_vc_tmp; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE IF NOT EXISTS public.xp_vc_tmp (
                                  guild_id bigint NOT NULL,
                                  user_id bigint NOT NULL,
                                  "time" timestamp without time zone NOT NULL,
                                  CONSTRAINT xp_vc_tmp_pk PRIMARY KEY (guild_id, user_id)
);
