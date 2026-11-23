SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: natural_sort_order; Type: COLLATION; Schema: public; Owner: -
--

CREATE COLLATION public.natural_sort_order (provider = icu, locale = 'und-u-kn');


--
-- Name: btree_gist; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS btree_gist WITH SCHEMA pg_catalog;


--
-- Name: EXTENSION btree_gist; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION btree_gist IS 'support for indexing common datatypes in GiST';


--
-- Name: pg_trgm; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA pg_catalog;


--
-- Name: EXTENSION pg_trgm; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION pg_trgm IS 'text similarity measurement and index searching based on trigrams';


--
-- Name: unaccent; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS unaccent WITH SCHEMA pg_catalog;


--
-- Name: EXTENSION unaccent; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION unaccent IS 'text search dictionary that removes accents';


--
-- Name: work_package_version_kind; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.work_package_version_kind AS ENUM (
    'target',
    'observed_in'
);


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: ai_text_action_types; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ai_text_action_types (
    id bigint NOT NULL,
    ai_text_action_id bigint NOT NULL,
    type_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: ai_text_action_types_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ai_text_action_types_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ai_text_action_types_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ai_text_action_types_id_seq OWNED BY public.ai_text_action_types.id;


--
-- Name: ai_text_actions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ai_text_actions (
    id bigint NOT NULL,
    label character varying NOT NULL,
    prompt text NOT NULL,
    usage_scope character varying NOT NULL,
    active boolean DEFAULT true NOT NULL,
    "position" integer,
    injects_type_template boolean DEFAULT false NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: ai_text_actions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ai_text_actions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ai_text_actions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ai_text_actions_id_seq OWNED BY public.ai_text_actions.id;


--
-- Name: ai_text_transform_run_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ai_text_transform_run_events (
    id bigint NOT NULL,
    run_id bigint NOT NULL,
    seq integer NOT NULL,
    kind character varying NOT NULL,
    payload jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp(6) with time zone NOT NULL
);


--
-- Name: ai_text_transform_run_events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ai_text_transform_run_events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ai_text_transform_run_events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ai_text_transform_run_events_id_seq OWNED BY public.ai_text_transform_run_events.id;


--
-- Name: ai_text_transform_runs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ai_text_transform_runs (
    id bigint NOT NULL,
    uuid uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id bigint NOT NULL,
    ai_text_action_id bigint NOT NULL,
    status character varying DEFAULT 'queued'::character varying NOT NULL,
    system_prompt text NOT NULL,
    input text NOT NULL,
    cancel_requested boolean DEFAULT false NOT NULL,
    error_message character varying,
    finished_at timestamp(6) with time zone,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: ai_text_transform_runs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ai_text_transform_runs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ai_text_transform_runs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ai_text_transform_runs_id_seq OWNED BY public.ai_text_transform_runs.id;


--
-- Name: announcements; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.announcements (
    id bigint NOT NULL,
    text text,
    show_until date,
    active boolean DEFAULT false,
    created_at timestamp with time zone,
    updated_at timestamp with time zone
);


--
-- Name: announcements_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.announcements_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: announcements_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.announcements_id_seq OWNED BY public.announcements.id;


--
-- Name: ar_internal_metadata; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ar_internal_metadata (
    key character varying NOT NULL,
    value character varying,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: attachable_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.attachable_journals (
    id bigint NOT NULL,
    journal_id bigint NOT NULL,
    attachment_id bigint NOT NULL,
    filename character varying NOT NULL
);


--
-- Name: attachable_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.attachable_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: attachable_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.attachable_journals_id_seq OWNED BY public.attachable_journals.id;


--
-- Name: attachment_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.attachment_journals (
    id bigint NOT NULL,
    container_id bigint,
    container_type character varying(30),
    filename character varying NOT NULL,
    disk_filename character varying NOT NULL,
    filesize bigint NOT NULL,
    content_type character varying,
    digest character varying(40) NOT NULL,
    downloads integer NOT NULL,
    author_id bigint NOT NULL,
    description text
);


--
-- Name: attachment_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.attachment_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: attachment_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.attachment_journals_id_seq OWNED BY public.attachment_journals.id;


--
-- Name: attachments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.attachments (
    id bigint NOT NULL,
    container_id bigint,
    container_type character varying(30),
    filename character varying DEFAULT ''::character varying NOT NULL,
    disk_filename character varying DEFAULT ''::character varying NOT NULL,
    filesize bigint DEFAULT 0 NOT NULL,
    content_type character varying DEFAULT ''::character varying,
    digest character varying(40) DEFAULT ''::character varying NOT NULL,
    downloads integer DEFAULT 0 NOT NULL,
    author_id bigint NOT NULL,
    created_at timestamp with time zone,
    description character varying,
    file character varying,
    fulltext text,
    fulltext_tsv tsvector,
    file_tsv tsvector,
    updated_at timestamp with time zone,
    status integer DEFAULT 0 NOT NULL,
    charset text
);


--
-- Name: attachments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.attachments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: attachments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.attachments_id_seq OWNED BY public.attachments.id;


--
-- Name: attribute_help_texts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.attribute_help_texts (
    id bigint NOT NULL,
    help_text text NOT NULL,
    type character varying NOT NULL,
    attribute_name character varying NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    caption text
);


--
-- Name: attribute_help_texts_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.attribute_help_texts_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: attribute_help_texts_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.attribute_help_texts_id_seq OWNED BY public.attribute_help_texts.id;


--
-- Name: auth_providers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.auth_providers (
    id bigint NOT NULL,
    type character varying NOT NULL,
    display_name character varying NOT NULL,
    slug character varying NOT NULL,
    available boolean DEFAULT true NOT NULL,
    limit_self_registration boolean DEFAULT false NOT NULL,
    options jsonb DEFAULT '{}'::jsonb NOT NULL,
    creator_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: auth_providers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.auth_providers_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: auth_providers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.auth_providers_id_seq OWNED BY public.auth_providers.id;


--
-- Name: autologin_session_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.autologin_session_links (
    id bigint NOT NULL,
    token_id bigint NOT NULL,
    session_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: autologin_session_links_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.autologin_session_links_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: autologin_session_links_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.autologin_session_links_id_seq OWNED BY public.autologin_session_links.id;


--
-- Name: automation_actions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.automation_actions (
    id bigint NOT NULL,
    automation_id bigint NOT NULL,
    custom_field_id bigint,
    type character varying NOT NULL,
    options jsonb DEFAULT '{}'::jsonb NOT NULL,
    "position" integer,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: automation_actions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.automation_actions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: automation_actions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.automation_actions_id_seq OWNED BY public.automation_actions.id;


--
-- Name: automation_triggers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.automation_triggers (
    id bigint NOT NULL,
    automation_id bigint NOT NULL,
    type character varying NOT NULL,
    options jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: automation_triggers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.automation_triggers_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: automation_triggers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.automation_triggers_id_seq OWNED BY public.automation_triggers.id;


--
-- Name: automations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.automations (
    id bigint NOT NULL,
    name character varying,
    description text,
    "position" integer
);


--
-- Name: automations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.automations_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: automations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.automations_id_seq OWNED BY public.automations.id;


--
-- Name: automations_projects; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.automations_projects (
    id bigint NOT NULL,
    project_id bigint,
    automation_id bigint
);


--
-- Name: automations_projects_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.automations_projects_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: automations_projects_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.automations_projects_id_seq OWNED BY public.automations_projects.id;


--
-- Name: automations_roles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.automations_roles (
    id bigint NOT NULL,
    role_id bigint,
    automation_id bigint
);


--
-- Name: automations_roles_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.automations_roles_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: automations_roles_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.automations_roles_id_seq OWNED BY public.automations_roles.id;


--
-- Name: automations_statuses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.automations_statuses (
    id bigint NOT NULL,
    status_id bigint,
    automation_id bigint
);


--
-- Name: automations_statuses_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.automations_statuses_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: automations_statuses_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.automations_statuses_id_seq OWNED BY public.automations_statuses.id;


--
-- Name: automations_types; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.automations_types (
    id bigint NOT NULL,
    type_id bigint,
    automation_id bigint
);


--
-- Name: automations_types_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.automations_types_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: automations_types_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.automations_types_id_seq OWNED BY public.automations_types.id;


--
-- Name: backlog_buckets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.backlog_buckets (
    id bigint NOT NULL,
    name character varying NOT NULL COLLATE public.natural_sort_order,
    project_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: backlog_buckets_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.backlog_buckets_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: backlog_buckets_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.backlog_buckets_id_seq OWNED BY public.backlog_buckets.id;


--
-- Name: backlog_excluded_types; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.backlog_excluded_types (
    project_id bigint NOT NULL,
    type_id bigint NOT NULL
);


--
-- Name: bcf_comments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.bcf_comments (
    id bigint NOT NULL,
    uuid text,
    journal_id bigint,
    issue_id bigint,
    viewpoint_id bigint,
    reply_to bigint
);


--
-- Name: bcf_comments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.bcf_comments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: bcf_comments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.bcf_comments_id_seq OWNED BY public.bcf_comments.id;


--
-- Name: bcf_issues; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.bcf_issues (
    id bigint NOT NULL,
    uuid text,
    markup xml,
    work_package_id bigint,
    stage character varying,
    index integer,
    labels text[] DEFAULT '{}'::text[],
    created_at timestamp(6) with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp(6) with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: bcf_issues_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.bcf_issues_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: bcf_issues_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.bcf_issues_id_seq OWNED BY public.bcf_issues.id;


--
-- Name: bcf_viewpoints; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.bcf_viewpoints (
    id bigint NOT NULL,
    uuid text,
    viewpoint_name text,
    issue_id bigint,
    json_viewpoint jsonb,
    created_at timestamp(6) with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp(6) with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: bcf_viewpoints_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.bcf_viewpoints_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: bcf_viewpoints_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.bcf_viewpoints_id_seq OWNED BY public.bcf_viewpoints.id;


--
-- Name: budget_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.budget_journals (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    author_id bigint NOT NULL,
    subject character varying NOT NULL,
    description text,
    fixed_date date NOT NULL,
    base_amount numeric(20,2) DEFAULT 0.0 NOT NULL
);


--
-- Name: budget_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.budget_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: budget_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.budget_journals_id_seq OWNED BY public.budget_journals.id;


--
-- Name: budgets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.budgets (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    author_id bigint NOT NULL,
    subject character varying NOT NULL,
    description text NOT NULL,
    fixed_date date NOT NULL,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    base_amount numeric(20,2) DEFAULT 0.0 NOT NULL
);


--
-- Name: budgets_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.budgets_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: budgets_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.budgets_id_seq OWNED BY public.budgets.id;


--
-- Name: calculated_value_errors; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calculated_value_errors (
    id bigint NOT NULL,
    customized_type character varying NOT NULL,
    customized_id bigint NOT NULL,
    custom_field_id bigint,
    error_code character varying NOT NULL,
    offending_custom_field_ids integer[] DEFAULT '{}'::integer[] NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: calculated_value_errors_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.calculated_value_errors_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: calculated_value_errors_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.calculated_value_errors_id_seq OWNED BY public.calculated_value_errors.id;


--
-- Name: categories; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.categories (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    name character varying DEFAULT ''::character varying NOT NULL,
    assigned_to_id bigint,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: categories_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.categories_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: categories_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.categories_id_seq OWNED BY public.categories.id;


--
-- Name: changes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.changes (
    id bigint NOT NULL,
    changeset_id bigint NOT NULL,
    action character varying(1) DEFAULT ''::character varying NOT NULL,
    path text NOT NULL,
    from_path text,
    from_revision character varying,
    revision character varying,
    branch character varying
);


--
-- Name: changes_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.changes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: changes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.changes_id_seq OWNED BY public.changes.id;


--
-- Name: changeset_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.changeset_journals (
    id bigint NOT NULL,
    repository_id bigint NOT NULL,
    revision character varying NOT NULL,
    committer character varying,
    committed_on timestamp with time zone NOT NULL,
    comments text,
    commit_date date,
    scmid character varying,
    user_id bigint
);


--
-- Name: changeset_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.changeset_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: changeset_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.changeset_journals_id_seq OWNED BY public.changeset_journals.id;


--
-- Name: changesets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.changesets (
    id bigint NOT NULL,
    repository_id bigint NOT NULL,
    revision character varying NOT NULL,
    committer character varying,
    committed_on timestamp with time zone NOT NULL,
    comments text,
    commit_date date,
    scmid character varying,
    user_id bigint
);


--
-- Name: changesets_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.changesets_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: changesets_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.changesets_id_seq OWNED BY public.changesets.id;


--
-- Name: changesets_work_packages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.changesets_work_packages (
    changeset_id bigint NOT NULL,
    work_package_id bigint NOT NULL
);


--
-- Name: colors; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.colors (
    id bigint NOT NULL,
    name character varying NOT NULL,
    hexcode character varying NOT NULL,
    created_at timestamp with time zone,
    updated_at timestamp with time zone
);


--
-- Name: colors_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.colors_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: colors_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.colors_id_seq OWNED BY public.colors.id;


--
-- Name: comments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.comments (
    id bigint NOT NULL,
    commented_type character varying(30) DEFAULT ''::character varying NOT NULL,
    commented_id bigint NOT NULL,
    author_id bigint NOT NULL,
    comments text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: comments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.comments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: comments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.comments_id_seq OWNED BY public.comments.id;


--
-- Name: cost_entries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.cost_entries (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    project_id bigint NOT NULL,
    cost_type_id bigint NOT NULL,
    units double precision NOT NULL,
    spent_on date NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    comments character varying NOT NULL,
    blocked boolean DEFAULT false NOT NULL,
    overridden_costs numeric(15,4),
    costs numeric(15,4),
    rate_id bigint,
    tyear integer NOT NULL,
    tmonth integer NOT NULL,
    tweek integer NOT NULL,
    logged_by_id bigint,
    entity_type character varying,
    entity_id bigint
);


--
-- Name: cost_entries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.cost_entries_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: cost_entries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.cost_entries_id_seq OWNED BY public.cost_entries.id;


--
-- Name: cost_queries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.cost_queries (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    project_id bigint,
    name character varying NOT NULL,
    is_public boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    serialized character varying(2000) NOT NULL
);


--
-- Name: cost_queries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.cost_queries_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: cost_queries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.cost_queries_id_seq OWNED BY public.cost_queries.id;


--
-- Name: cost_types; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.cost_types (
    id bigint NOT NULL,
    name character varying NOT NULL,
    unit character varying NOT NULL,
    unit_plural character varying NOT NULL,
    "default" boolean DEFAULT false NOT NULL,
    deleted_at timestamp with time zone,
    for_all_projects boolean DEFAULT true NOT NULL
);


--
-- Name: cost_types_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.cost_types_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: cost_types_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.cost_types_id_seq OWNED BY public.cost_types.id;


--
-- Name: cost_types_projects; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.cost_types_projects (
    id bigint NOT NULL,
    cost_type_id bigint NOT NULL,
    project_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: cost_types_projects_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.cost_types_projects_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: cost_types_projects_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.cost_types_projects_id_seq OWNED BY public.cost_types_projects.id;


--
-- Name: custom_comment_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.custom_comment_journals (
    id bigint NOT NULL,
    custom_field_id bigint NOT NULL,
    journal_id bigint NOT NULL,
    text text NOT NULL
);


--
-- Name: custom_comment_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.custom_comment_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: custom_comment_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.custom_comment_journals_id_seq OWNED BY public.custom_comment_journals.id;


--
-- Name: custom_comments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.custom_comments (
    id bigint NOT NULL,
    custom_field_id bigint NOT NULL,
    customized_type character varying NOT NULL,
    customized_id bigint NOT NULL,
    text text NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: custom_comments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.custom_comments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: custom_comments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.custom_comments_id_seq OWNED BY public.custom_comments.id;


--
-- Name: custom_field_project_specific_value_restrictions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.custom_field_project_specific_value_restrictions (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    custom_field_id bigint NOT NULL,
    restriction character varying NOT NULL,
    custom_option_id bigint,
    hierarchy_item_id bigint,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    CONSTRAINT cf_ps_value_restrictions_one_value CHECK ((num_nonnulls(custom_option_id, hierarchy_item_id) = 1))
);


--
-- Name: custom_field_project_specific_value_restrictions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.custom_field_project_specific_value_restrictions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: custom_field_project_specific_value_restrictions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.custom_field_project_specific_value_restrictions_id_seq OWNED BY public.custom_field_project_specific_value_restrictions.id;


--
-- Name: custom_field_sections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.custom_field_sections (
    id bigint NOT NULL,
    "position" integer,
    name character varying,
    type character varying,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    display_representation jsonb DEFAULT '{"overview": "sidebar"}'::jsonb NOT NULL,
    attribute_order character varying[] DEFAULT '{}'::character varying[] NOT NULL
);


--
-- Name: custom_field_sections_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.custom_field_sections_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: custom_field_sections_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.custom_field_sections_id_seq OWNED BY public.custom_field_sections.id;


--
-- Name: custom_fields; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.custom_fields (
    id bigint NOT NULL,
    type character varying(30) DEFAULT ''::character varying NOT NULL,
    field_format character varying(30) DEFAULT ''::character varying NOT NULL,
    is_required boolean DEFAULT false NOT NULL,
    is_for_all boolean DEFAULT false NOT NULL,
    is_filter boolean DEFAULT true NOT NULL,
    "position" integer DEFAULT 1,
    searchable boolean DEFAULT false,
    admin_only boolean DEFAULT false NOT NULL,
    multi_value boolean DEFAULT false,
    name character varying,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    custom_field_section_id bigint,
    formula jsonb,
    has_comment boolean DEFAULT false NOT NULL,
    visible_on_user_card boolean DEFAULT false NOT NULL,
    semantic_key character varying,
    project_specific_value_restriction character varying,
    settings jsonb DEFAULT '{}'::jsonb NOT NULL
);


--
-- Name: custom_fields_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.custom_fields_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: custom_fields_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.custom_fields_id_seq OWNED BY public.custom_fields.id;


--
-- Name: custom_fields_projects; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.custom_fields_projects (
    id bigint NOT NULL,
    custom_field_id bigint NOT NULL,
    project_id bigint NOT NULL
);


--
-- Name: custom_fields_projects_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.custom_fields_projects_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: custom_fields_projects_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.custom_fields_projects_id_seq OWNED BY public.custom_fields_projects.id;


--
-- Name: custom_fields_roles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.custom_fields_roles (
    id bigint NOT NULL,
    custom_field_id bigint NOT NULL,
    role_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: custom_fields_roles_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.custom_fields_roles_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: custom_fields_roles_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.custom_fields_roles_id_seq OWNED BY public.custom_fields_roles.id;


--
-- Name: custom_options; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.custom_options (
    id bigint NOT NULL,
    custom_field_id bigint,
    "position" integer,
    default_value boolean,
    value text,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    project_id bigint
);


--
-- Name: custom_options_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.custom_options_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: custom_options_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.custom_options_id_seq OWNED BY public.custom_options.id;


--
-- Name: custom_styles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.custom_styles (
    id bigint NOT NULL,
    logo character varying,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    favicon character varying,
    touch_icon character varying,
    theme character varying DEFAULT 'OpenProject (default)'::character varying,
    theme_logo character varying,
    export_logo character varying,
    export_cover character varying,
    export_cover_text_color character varying,
    export_footer character varying,
    export_font_regular character varying,
    export_font_bold character varying,
    export_font_italic character varying,
    export_font_bold_italic character varying,
    logo_mobile character varying,
    logo_dark character varying,
    logo_light_high_contrast character varying,
    logo_mobile_dark character varying,
    logo_mobile_light_high_contrast character varying
);


--
-- Name: custom_styles_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.custom_styles_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: custom_styles_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.custom_styles_id_seq OWNED BY public.custom_styles.id;


--
-- Name: custom_values; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.custom_values (
    id bigint NOT NULL,
    customized_type character varying(30) DEFAULT ''::character varying NOT NULL,
    customized_id bigint NOT NULL,
    custom_field_id bigint NOT NULL,
    value text
);


--
-- Name: custom_values_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.custom_values_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: custom_values_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.custom_values_id_seq OWNED BY public.custom_values.id;


--
-- Name: customizable_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.customizable_journals (
    id bigint NOT NULL,
    journal_id bigint NOT NULL,
    custom_field_id bigint NOT NULL,
    value text
);


--
-- Name: customizable_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.customizable_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: customizable_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.customizable_journals_id_seq OWNED BY public.customizable_journals.id;


--
-- Name: deploy_status_checks; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.deploy_status_checks (
    id bigint NOT NULL,
    deploy_target_id bigint,
    github_pull_request_id bigint,
    core_sha text NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: deploy_status_checks_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.deploy_status_checks_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: deploy_status_checks_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.deploy_status_checks_id_seq OWNED BY public.deploy_status_checks.id;


--
-- Name: deploy_targets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.deploy_targets (
    id bigint NOT NULL,
    type text NOT NULL,
    host text NOT NULL,
    options jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: deploy_targets_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.deploy_targets_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: deploy_targets_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.deploy_targets_id_seq OWNED BY public.deploy_targets.id;


--
-- Name: design_colors; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.design_colors (
    id bigint NOT NULL,
    variable character varying,
    hexcode character varying,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: design_colors_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.design_colors_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: design_colors_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.design_colors_id_seq OWNED BY public.design_colors.id;


--
-- Name: document_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.document_journals (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    category_id bigint,
    title character varying(255) NOT NULL,
    description text,
    content_binary text
);


--
-- Name: document_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.document_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: document_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.document_journals_id_seq OWNED BY public.document_journals.id;


--
-- Name: document_types; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.document_types (
    id bigint NOT NULL,
    name character varying,
    "position" integer,
    is_default boolean DEFAULT false NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    active boolean DEFAULT true NOT NULL,
    documents_count integer DEFAULT 0 NOT NULL
);


--
-- Name: document_types_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.document_types_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: document_types_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.document_types_id_seq OWNED BY public.document_types.id;


--
-- Name: documents; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.documents (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    category_id bigint,
    title character varying(255) DEFAULT ''::character varying NOT NULL,
    description text,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    type_id bigint,
    kind character varying DEFAULT 'collaborative'::character varying,
    content_binary text
);


--
-- Name: documents_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.documents_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: documents_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.documents_id_seq OWNED BY public.documents.id;


--
-- Name: done_statuses_for_project; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.done_statuses_for_project (
    project_id bigint,
    status_id bigint
);


--
-- Name: emoji_reactions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.emoji_reactions (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    reactable_type character varying NOT NULL,
    reactable_id bigint NOT NULL,
    reaction character varying NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    CONSTRAINT emoji_reactions_agenda_item_votes CHECK ((((reactable_type)::text <> 'MeetingAgendaItem'::text) OR ((reaction)::text = ANY ((ARRAY['thumbs_up'::character varying, 'thumbs_down'::character varying])::text[]))))
);


--
-- Name: emoji_reactions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.emoji_reactions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: emoji_reactions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.emoji_reactions_id_seq OWNED BY public.emoji_reactions.id;


--
-- Name: enabled_modules; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.enabled_modules (
    id bigint NOT NULL,
    project_id bigint,
    name character varying NOT NULL
);


--
-- Name: enabled_modules_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.enabled_modules_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: enabled_modules_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.enabled_modules_id_seq OWNED BY public.enabled_modules.id;


--
-- Name: enterprise_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.enterprise_tokens (
    id bigint NOT NULL,
    encoded_token text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    valid_from date,
    valid_until date
);


--
-- Name: enterprise_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.enterprise_tokens_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: enterprise_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.enterprise_tokens_id_seq OWNED BY public.enterprise_tokens.id;


--
-- Name: enumerations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.enumerations (
    id bigint NOT NULL,
    name character varying DEFAULT ''::character varying NOT NULL,
    "position" integer DEFAULT 1,
    is_default boolean DEFAULT false NOT NULL,
    type character varying,
    active boolean DEFAULT true NOT NULL,
    project_id bigint,
    parent_id bigint,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    color_id bigint
);


--
-- Name: enumerations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.enumerations_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: enumerations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.enumerations_id_seq OWNED BY public.enumerations.id;


--
-- Name: export_settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.export_settings (
    id bigint NOT NULL,
    query_id bigint NOT NULL,
    format character varying NOT NULL,
    settings jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: export_settings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.export_settings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: export_settings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.export_settings_id_seq OWNED BY public.export_settings.id;


--
-- Name: exports; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.exports (
    id bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    type character varying
);


--
-- Name: exports_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.exports_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: exports_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.exports_id_seq OWNED BY public.exports.id;


--
-- Name: favorites; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.favorites (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    favorited_type character varying NOT NULL,
    favorited_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: favorites_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.favorites_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: favorites_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.favorites_id_seq OWNED BY public.favorites.id;


--
-- Name: file_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.file_links (
    id bigint NOT NULL,
    storage_id bigint,
    creator_id bigint NOT NULL,
    container_id bigint,
    container_type character varying,
    origin_id character varying,
    origin_name character varying,
    origin_created_by_name character varying,
    origin_last_modified_by_name character varying,
    origin_mime_type character varying,
    origin_created_at timestamp with time zone,
    origin_updated_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: file_links_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.file_links_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: file_links_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.file_links_id_seq OWNED BY public.file_links.id;


--
-- Name: form_configuration_attributes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.form_configuration_attributes (
    id bigint NOT NULL,
    form_configuration_id bigint NOT NULL,
    form_configuration_group_id bigint,
    "position" integer,
    custom_field_id bigint,
    attribute_key character varying,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    CONSTRAINT form_configuration_attributes_one_reference CHECK (((custom_field_id IS NULL) <> (attribute_key IS NULL))),
    CONSTRAINT form_configuration_attributes_placement CHECK (((form_configuration_group_id IS NULL) = ("position" IS NULL)))
);


--
-- Name: form_configuration_attributes_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.form_configuration_attributes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: form_configuration_attributes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.form_configuration_attributes_id_seq OWNED BY public.form_configuration_attributes.id;


--
-- Name: form_configuration_groups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.form_configuration_groups (
    id bigint NOT NULL,
    form_configuration_id bigint NOT NULL,
    "position" integer NOT NULL,
    kind character varying NOT NULL,
    default_key character varying,
    label character varying,
    query_id bigint,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    CONSTRAINT form_configuration_groups_kind CHECK (((kind)::text = ANY ((ARRAY['attribute'::character varying, 'query'::character varying])::text[]))),
    CONSTRAINT form_configuration_groups_named CHECK (((default_key IS NOT NULL) OR (label IS NOT NULL))),
    CONSTRAINT form_configuration_groups_query_matches_kind CHECK ((((kind)::text = 'query'::text) = (query_id IS NOT NULL)))
);


--
-- Name: form_configuration_groups_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.form_configuration_groups_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: form_configuration_groups_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.form_configuration_groups_id_seq OWNED BY public.form_configuration_groups.id;


--
-- Name: form_configurations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.form_configurations (
    id bigint NOT NULL,
    name character varying NOT NULL,
    description text,
    is_default boolean DEFAULT false NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: form_configurations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.form_configurations_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: form_configurations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.form_configurations_id_seq OWNED BY public.form_configurations.id;


--
-- Name: forums; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.forums (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    name character varying DEFAULT ''::character varying NOT NULL,
    description character varying,
    "position" integer DEFAULT 1,
    topics_count integer DEFAULT 0 NOT NULL,
    messages_count integer DEFAULT 0 NOT NULL,
    last_message_id bigint
);


--
-- Name: forums_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.forums_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: forums_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.forums_id_seq OWNED BY public.forums.id;


--
-- Name: friendly_id_slugs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.friendly_id_slugs (
    id bigint NOT NULL,
    slug character varying NOT NULL,
    sluggable_id bigint NOT NULL,
    sluggable_type character varying(50) NOT NULL,
    scope character varying,
    created_at timestamp(6) with time zone
);


--
-- Name: friendly_id_slugs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.friendly_id_slugs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: friendly_id_slugs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.friendly_id_slugs_id_seq OWNED BY public.friendly_id_slugs.id;


--
-- Name: github_check_runs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.github_check_runs (
    id bigint NOT NULL,
    github_pull_request_id bigint NOT NULL,
    github_id bigint NOT NULL,
    github_html_url character varying NOT NULL,
    app_id bigint NOT NULL,
    github_app_owner_avatar_url character varying NOT NULL,
    status character varying NOT NULL,
    name character varying NOT NULL,
    conclusion character varying,
    output_title character varying,
    output_summary character varying,
    details_url character varying,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: github_check_runs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.github_check_runs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: github_check_runs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.github_check_runs_id_seq OWNED BY public.github_check_runs.id;


--
-- Name: github_pull_requests; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.github_pull_requests (
    id bigint NOT NULL,
    github_user_id bigint,
    merged_by_id bigint,
    github_id bigint,
    number integer NOT NULL,
    github_html_url character varying NOT NULL,
    state character varying NOT NULL,
    repository character varying NOT NULL,
    github_updated_at timestamp with time zone,
    title character varying,
    body text,
    draft boolean,
    merged boolean,
    merged_at timestamp with time zone,
    comments_count integer,
    review_comments_count integer,
    additions_count integer,
    deletions_count integer,
    changed_files_count integer,
    labels json,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    repository_html_url character varying,
    merge_commit_sha text
);


--
-- Name: github_pull_requests_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.github_pull_requests_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: github_pull_requests_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.github_pull_requests_id_seq OWNED BY public.github_pull_requests.id;


--
-- Name: github_pull_requests_work_packages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.github_pull_requests_work_packages (
    github_pull_request_id bigint NOT NULL,
    work_package_id bigint NOT NULL
);


--
-- Name: github_users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.github_users (
    id bigint NOT NULL,
    github_id bigint NOT NULL,
    github_login character varying NOT NULL,
    github_html_url character varying NOT NULL,
    github_avatar_url character varying NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: github_users_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.github_users_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: github_users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.github_users_id_seq OWNED BY public.github_users.id;


--
-- Name: gitlab_branches; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.gitlab_branches (
    id bigint NOT NULL,
    work_package_id bigint NOT NULL,
    gitlab_project_id bigint NOT NULL,
    namespace character varying NOT NULL,
    project_html_url character varying NOT NULL,
    name character varying NOT NULL,
    repository character varying NOT NULL,
    gitlab_user_id bigint,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: gitlab_branches_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.gitlab_branches_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: gitlab_branches_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.gitlab_branches_id_seq OWNED BY public.gitlab_branches.id;


--
-- Name: gitlab_commits; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.gitlab_commits (
    id bigint NOT NULL,
    gitlab_user_id bigint,
    sha character varying NOT NULL,
    message text DEFAULT ''::text NOT NULL,
    author_name character varying NOT NULL,
    author_email character varying NOT NULL,
    authored_at timestamp with time zone NOT NULL,
    repository character varying NOT NULL,
    gitlab_html_url character varying NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: gitlab_commits_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.gitlab_commits_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: gitlab_commits_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.gitlab_commits_id_seq OWNED BY public.gitlab_commits.id;


--
-- Name: gitlab_commits_work_packages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.gitlab_commits_work_packages (
    id bigint NOT NULL,
    gitlab_commit_id bigint,
    work_package_id bigint,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: gitlab_commits_work_packages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.gitlab_commits_work_packages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: gitlab_commits_work_packages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.gitlab_commits_work_packages_id_seq OWNED BY public.gitlab_commits_work_packages.id;


--
-- Name: gitlab_issues; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.gitlab_issues (
    id bigint NOT NULL,
    gitlab_user_id bigint,
    gitlab_id bigint,
    number integer NOT NULL,
    gitlab_html_url character varying NOT NULL,
    state character varying NOT NULL,
    repository character varying NOT NULL,
    gitlab_updated_at timestamp(6) with time zone,
    title character varying,
    body text,
    labels json,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: gitlab_issues_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.gitlab_issues_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: gitlab_issues_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.gitlab_issues_id_seq OWNED BY public.gitlab_issues.id;


--
-- Name: gitlab_issues_work_packages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.gitlab_issues_work_packages (
    gitlab_issue_id bigint NOT NULL,
    work_package_id bigint NOT NULL
);


--
-- Name: gitlab_merge_requests; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.gitlab_merge_requests (
    id bigint NOT NULL,
    gitlab_user_id bigint,
    merged_by_id bigint,
    gitlab_id bigint,
    number integer NOT NULL,
    gitlab_html_url character varying NOT NULL,
    state character varying NOT NULL,
    repository character varying NOT NULL,
    gitlab_updated_at timestamp with time zone,
    title character varying,
    body text,
    draft boolean,
    merged boolean,
    merged_at timestamp with time zone,
    labels json,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: gitlab_merge_requests_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.gitlab_merge_requests_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: gitlab_merge_requests_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.gitlab_merge_requests_id_seq OWNED BY public.gitlab_merge_requests.id;


--
-- Name: gitlab_merge_requests_work_packages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.gitlab_merge_requests_work_packages (
    gitlab_merge_request_id bigint NOT NULL,
    work_package_id bigint NOT NULL
);


--
-- Name: gitlab_pipelines; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.gitlab_pipelines (
    id bigint NOT NULL,
    gitlab_merge_request_id bigint NOT NULL,
    gitlab_id bigint NOT NULL,
    gitlab_html_url character varying NOT NULL,
    project_id bigint NOT NULL,
    gitlab_user_avatar_url character varying NOT NULL,
    status character varying NOT NULL,
    name character varying NOT NULL,
    details_url character varying,
    ci_details json,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    username text,
    commit_id text
);


--
-- Name: gitlab_pipelines_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.gitlab_pipelines_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: gitlab_pipelines_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.gitlab_pipelines_id_seq OWNED BY public.gitlab_pipelines.id;


--
-- Name: gitlab_users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.gitlab_users (
    id bigint NOT NULL,
    gitlab_id bigint NOT NULL,
    name character varying NOT NULL,
    username character varying NOT NULL,
    avatar_url character varying NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: gitlab_users_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.gitlab_users_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: gitlab_users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.gitlab_users_id_seq OWNED BY public.gitlab_users.id;


--
-- Name: good_job_batches; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.good_job_batches (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    description text,
    serialized_properties jsonb,
    on_finish text,
    on_success text,
    on_discard text,
    callback_queue_name text,
    callback_priority integer,
    enqueued_at timestamp(6) with time zone,
    discarded_at timestamp(6) with time zone,
    finished_at timestamp(6) with time zone,
    jobs_finished_at timestamp(6) with time zone
);


--
-- Name: good_job_executions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.good_job_executions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    active_job_id uuid NOT NULL,
    job_class text,
    queue_name text,
    serialized_params jsonb,
    scheduled_at timestamp(6) with time zone,
    finished_at timestamp(6) with time zone,
    error text,
    error_event smallint,
    error_backtrace text[],
    process_id uuid,
    duration interval
);


--
-- Name: good_job_processes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.good_job_processes (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    state jsonb,
    lock_type smallint
);


--
-- Name: good_job_settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.good_job_settings (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    key text,
    value jsonb
);


--
-- Name: good_jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.good_jobs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    queue_name text,
    priority integer,
    serialized_params jsonb,
    scheduled_at timestamp(6) with time zone,
    performed_at timestamp(6) with time zone,
    finished_at timestamp(6) with time zone,
    error text,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    active_job_id uuid,
    concurrency_key text,
    cron_key text,
    retried_good_job_id uuid,
    cron_at timestamp(6) with time zone,
    batch_id uuid,
    batch_callback_id uuid,
    is_discrete boolean,
    executions_count integer,
    job_class text,
    error_event smallint,
    labels text[],
    locked_by_id uuid,
    locked_at timestamp(6) with time zone,
    lock_type smallint
);


--
-- Name: grid_widgets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.grid_widgets (
    id bigint NOT NULL,
    start_row integer NOT NULL,
    end_row integer NOT NULL,
    start_column integer NOT NULL,
    end_column integer NOT NULL,
    identifier character varying,
    options text,
    grid_id bigint
);


--
-- Name: grid_widgets_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.grid_widgets_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: grid_widgets_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.grid_widgets_id_seq OWNED BY public.grid_widgets.id;


--
-- Name: grids; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.grids (
    id bigint NOT NULL,
    row_count integer NOT NULL,
    column_count integer NOT NULL,
    type character varying,
    user_id bigint,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    project_id bigint,
    name text COLLATE public.natural_sort_order,
    options text,
    linked_type character varying,
    linked_id bigint
);


--
-- Name: grids_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.grids_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: grids_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.grids_id_seq OWNED BY public.grids.id;


--
-- Name: group_details; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.group_details (
    id bigint NOT NULL,
    principal_id bigint NOT NULL,
    organizational_unit boolean DEFAULT false NOT NULL,
    parent_id bigint,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    avatar_color character varying,
    avatar_text character varying,
    avatar_type character varying
);


--
-- Name: group_details_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.group_details_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: group_details_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.group_details_id_seq OWNED BY public.group_details.id;


--
-- Name: group_users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.group_users (
    id bigint NOT NULL,
    group_id bigint NOT NULL,
    user_id bigint NOT NULL
);


--
-- Name: group_users_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.group_users_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: group_users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.group_users_id_seq OWNED BY public.group_users.id;


--
-- Name: health_reports; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.health_reports (
    id bigint NOT NULL,
    subject_type character varying NOT NULL,
    subject_id bigint NOT NULL,
    results jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: health_reports_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.health_reports_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: health_reports_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.health_reports_id_seq OWNED BY public.health_reports.id;


--
-- Name: hierarchical_item_hierarchies; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.hierarchical_item_hierarchies (
    ancestor_id integer NOT NULL,
    descendant_id integer NOT NULL,
    generations integer NOT NULL
);


--
-- Name: hierarchical_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.hierarchical_items (
    id bigint NOT NULL,
    parent_id integer,
    sort_order integer,
    label character varying,
    short character varying,
    is_deleted boolean DEFAULT false NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    custom_field_id bigint,
    position_cache bigint,
    weight numeric,
    children_count integer DEFAULT 0,
    project_id bigint
);


--
-- Name: hierarchical_items_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.hierarchical_items_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: hierarchical_items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.hierarchical_items_id_seq OWNED BY public.hierarchical_items.id;


--
-- Name: ical_token_query_assignments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ical_token_query_assignments (
    id bigint NOT NULL,
    ical_token_id bigint,
    query_id bigint,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    name character varying NOT NULL
);


--
-- Name: ical_token_query_assignments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ical_token_query_assignments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ical_token_query_assignments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ical_token_query_assignments_id_seq OWNED BY public.ical_token_query_assignments.id;


--
-- Name: ifc_models; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ifc_models (
    id bigint NOT NULL,
    title character varying,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    project_id bigint,
    uploader_id bigint,
    is_default boolean DEFAULT false NOT NULL,
    conversion_status integer DEFAULT 0,
    conversion_error_message text
);


--
-- Name: ifc_models_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ifc_models_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ifc_models_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ifc_models_id_seq OWNED BY public.ifc_models.id;


--
-- Name: itsm_email_messages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.itsm_email_messages (
    id bigint NOT NULL,
    thread_id bigint NOT NULL,
    journal_id bigint,
    direction character varying NOT NULL,
    status character varying NOT NULL,
    message_id character varying NOT NULL,
    in_reply_to character varying,
    referenced_message_ids text[] DEFAULT '{}'::text[] NOT NULL,
    from_address character varying NOT NULL,
    from_name character varying,
    to_addresses text[] DEFAULT '{}'::text[] NOT NULL,
    cc_addresses text[] DEFAULT '{}'::text[] NOT NULL,
    subject character varying,
    error_message text,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: itsm_email_messages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.itsm_email_messages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: itsm_email_messages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.itsm_email_messages_id_seq OWNED BY public.itsm_email_messages.id;


--
-- Name: itsm_email_threads; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.itsm_email_threads (
    id bigint NOT NULL,
    work_package_id bigint NOT NULL,
    requester_email character varying NOT NULL,
    requester_name character varying,
    recipients text[] DEFAULT '{}'::text[] NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: itsm_email_threads_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.itsm_email_threads_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: itsm_email_threads_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.itsm_email_threads_id_seq OWNED BY public.itsm_email_threads.id;


--
-- Name: itsm_incoming_mailboxes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.itsm_incoming_mailboxes (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    mail_server_id bigint NOT NULL,
    username character varying NOT NULL,
    password text NOT NULL,
    email character varying NOT NULL,
    aliases text[] DEFAULT '{}'::text[] NOT NULL,
    active boolean DEFAULT true NOT NULL,
    last_fetched_at timestamp(6) with time zone,
    next_fetch_at timestamp(6) with time zone,
    consecutive_failures integer DEFAULT 0 NOT NULL,
    last_error text,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: itsm_incoming_mailboxes_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.itsm_incoming_mailboxes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: itsm_incoming_mailboxes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.itsm_incoming_mailboxes_id_seq OWNED BY public.itsm_incoming_mailboxes.id;


--
-- Name: itsm_mail_servers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.itsm_mail_servers (
    id bigint NOT NULL,
    name character varying NOT NULL,
    protocol character varying NOT NULL,
    host character varying NOT NULL,
    port integer NOT NULL,
    encryption character varying NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: itsm_mail_servers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.itsm_mail_servers_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: itsm_mail_servers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.itsm_mail_servers_id_seq OWNED BY public.itsm_mail_servers.id;


--
-- Name: itsm_outgoing_mailboxes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.itsm_outgoing_mailboxes (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    mail_server_id bigint NOT NULL,
    username character varying NOT NULL,
    password text NOT NULL,
    sender_email character varying NOT NULL,
    sender_name character varying,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: itsm_outgoing_mailboxes_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.itsm_outgoing_mailboxes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: itsm_outgoing_mailboxes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.itsm_outgoing_mailboxes_id_seq OWNED BY public.itsm_outgoing_mailboxes.id;


--
-- Name: jira_attachments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_attachments (
    id bigint NOT NULL,
    payload jsonb,
    origin_id character varying NOT NULL,
    jira_import_id bigint NOT NULL,
    jira_project_id bigint NOT NULL,
    jira_issue_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_attachments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_attachments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_attachments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_attachments_id_seq OWNED BY public.jira_attachments.id;


--
-- Name: jira_boards; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_boards (
    id bigint NOT NULL,
    payload jsonb,
    origin_id character varying NOT NULL,
    jira_import_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_boards_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_boards_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_boards_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_boards_id_seq OWNED BY public.jira_boards.id;


--
-- Name: jira_fields; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_fields (
    id bigint NOT NULL,
    payload jsonb,
    origin_id character varying NOT NULL,
    jira_import_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    issue_values jsonb
);


--
-- Name: jira_fields_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_fields_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_fields_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_fields_id_seq OWNED BY public.jira_fields.id;


--
-- Name: jira_import_job_cursors; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_import_job_cursors (
    id bigint NOT NULL,
    jira_import_id bigint NOT NULL,
    job_class character varying NOT NULL,
    arguments jsonb NOT NULL,
    cursor jsonb NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_import_job_cursors_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_import_job_cursors_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_import_job_cursors_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_import_job_cursors_id_seq OWNED BY public.jira_import_job_cursors.id;


--
-- Name: jira_import_transitions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_import_transitions (
    id bigint NOT NULL,
    from_state character varying NOT NULL,
    to_state character varying NOT NULL,
    metadata jsonb DEFAULT '{}'::jsonb,
    sort_key integer NOT NULL,
    jira_import_id integer NOT NULL,
    most_recent boolean DEFAULT false NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_import_transitions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_import_transitions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_import_transitions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_import_transitions_id_seq OWNED BY public.jira_import_transitions.id;


--
-- Name: jira_imports; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_imports (
    id bigint NOT NULL,
    author_id bigint NOT NULL,
    projects jsonb DEFAULT '[]'::jsonb,
    selected jsonb DEFAULT '{}'::jsonb,
    available jsonb DEFAULT '{}'::jsonb,
    created_at timestamp(6) with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp(6) with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    jira_id bigint,
    system_field_custom_fields jsonb DEFAULT '{}'::jsonb NOT NULL
);


--
-- Name: jira_imports_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_imports_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_imports_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_imports_id_seq OWNED BY public.jira_imports.id;


--
-- Name: jira_issue_types; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_issue_types (
    id bigint NOT NULL,
    payload jsonb,
    origin_id character varying NOT NULL,
    jira_import_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_issue_types_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_issue_types_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_issue_types_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_issue_types_id_seq OWNED BY public.jira_issue_types.id;


--
-- Name: jira_issues; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_issues (
    id bigint NOT NULL,
    payload jsonb,
    origin_id character varying NOT NULL,
    jira_import_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    jira_project_id bigint NOT NULL
);


--
-- Name: jira_issues_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_issues_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_issues_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_issues_id_seq OWNED BY public.jira_issues.id;


--
-- Name: jira_open_project_references; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_open_project_references (
    id bigint NOT NULL,
    op_entity_id character varying,
    op_entity_class character varying,
    jira_entity_id character varying,
    jira_entity_class character varying,
    uses_existing boolean DEFAULT false NOT NULL,
    jira_import_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_open_project_references_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_open_project_references_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_open_project_references_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_open_project_references_id_seq OWNED BY public.jira_open_project_references.id;


--
-- Name: jira_priorities; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_priorities (
    id bigint NOT NULL,
    payload jsonb,
    origin_id character varying NOT NULL,
    jira_import_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_priorities_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_priorities_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_priorities_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_priorities_id_seq OWNED BY public.jira_priorities.id;


--
-- Name: jira_projects; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_projects (
    id bigint NOT NULL,
    payload jsonb,
    origin_id character varying NOT NULL,
    jira_import_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_projects_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_projects_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_projects_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_projects_id_seq OWNED BY public.jira_projects.id;


--
-- Name: jira_sprints; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_sprints (
    id bigint NOT NULL,
    payload jsonb,
    origin_id character varying NOT NULL,
    jira_import_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_sprints_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_sprints_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_sprints_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_sprints_id_seq OWNED BY public.jira_sprints.id;


--
-- Name: jira_statuses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_statuses (
    id bigint NOT NULL,
    payload jsonb,
    origin_id character varying NOT NULL,
    jira_import_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_statuses_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_statuses_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_statuses_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_statuses_id_seq OWNED BY public.jira_statuses.id;


--
-- Name: jira_users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_users (
    id bigint NOT NULL,
    payload jsonb,
    origin_id character varying NOT NULL,
    jira_import_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_users_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_users_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_users_id_seq OWNED BY public.jira_users.id;


--
-- Name: jira_versions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jira_versions (
    id bigint NOT NULL,
    payload jsonb,
    origin_id character varying NOT NULL,
    jira_import_id bigint NOT NULL,
    jira_project_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: jira_versions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jira_versions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jira_versions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jira_versions_id_seq OWNED BY public.jira_versions.id;


--
-- Name: jiras; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jiras (
    id bigint NOT NULL,
    url character varying,
    name character varying,
    personal_access_token character varying,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    basic_auth_username character varying,
    basic_auth_password character varying,
    auth_method character varying DEFAULT 'bearer'::character varying NOT NULL,
    options jsonb DEFAULT '{}'::jsonb NOT NULL
);


--
-- Name: jiras_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.jiras_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: jiras_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.jiras_id_seq OWNED BY public.jiras.id;


--
-- Name: job_statuses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.job_statuses (
    id bigint NOT NULL,
    reference_type character varying,
    reference_id bigint,
    message character varying,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    status character varying DEFAULT 'in_queue'::character varying,
    user_id bigint,
    job_id character varying,
    payload jsonb,
    CONSTRAINT delayed_job_statuses_status_check CHECK (((status IS NULL) OR ((status)::text = ANY ((ARRAY['in_queue'::character varying, 'error'::character varying, 'in_process'::character varying, 'success'::character varying, 'failure'::character varying, 'cancelled'::character varying])::text[]))))
);


--
-- Name: job_statuses_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.job_statuses_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: job_statuses_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.job_statuses_id_seq OWNED BY public.job_statuses.id;


--
-- Name: journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.journals (
    id bigint NOT NULL,
    journable_type character varying,
    journable_id bigint,
    user_id bigint NOT NULL,
    notes text,
    created_at timestamp with time zone NOT NULL,
    version integer DEFAULT 0 NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    data_type character varying NOT NULL,
    data_id bigint NOT NULL,
    cause jsonb DEFAULT '{}'::jsonb,
    validity_period tstzrange,
    restricted boolean DEFAULT false NOT NULL,
    reactions_changed_at timestamp(6) with time zone,
    CONSTRAINT journals_validity_period_not_empty CHECK (((NOT isempty(validity_period)) AND (validity_period IS NOT NULL)))
);


--
-- Name: journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.journals_id_seq OWNED BY public.journals.id;


--
-- Name: label_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.label_journals (
    id bigint NOT NULL,
    journal_id bigint NOT NULL,
    label_id bigint NOT NULL
);


--
-- Name: label_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.label_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: label_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.label_journals_id_seq OWNED BY public.label_journals.id;


--
-- Name: labelings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.labelings (
    id bigint NOT NULL,
    label_id bigint NOT NULL,
    labelable_type character varying NOT NULL,
    labelable_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: labelings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.labelings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: labelings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.labelings_id_seq OWNED BY public.labelings.id;


--
-- Name: labels; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.labels (
    id bigint NOT NULL,
    name character varying NOT NULL,
    author_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: labels_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.labels_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: labels_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.labels_id_seq OWNED BY public.labels.id;


--
-- Name: labor_budget_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.labor_budget_items (
    id bigint NOT NULL,
    budget_id bigint NOT NULL,
    hours double precision NOT NULL,
    user_id bigint,
    comments character varying DEFAULT ''::character varying NOT NULL,
    amount numeric(15,4)
);


--
-- Name: labor_budget_items_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.labor_budget_items_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: labor_budget_items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.labor_budget_items_id_seq OWNED BY public.labor_budget_items.id;


--
-- Name: last_project_folders; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.last_project_folders (
    id bigint NOT NULL,
    project_storage_id bigint NOT NULL,
    origin_folder_id character varying,
    mode character varying DEFAULT 'inactive'::character varying NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    CONSTRAINT last_project_folders_mode_check CHECK (((mode)::text = ANY ((ARRAY['inactive'::character varying, 'manual'::character varying, 'automatic'::character varying])::text[])))
);


--
-- Name: TABLE last_project_folders; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.last_project_folders IS 'This table contains the last used project folder IDs for a project storage per mode.';


--
-- Name: last_project_folders_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.last_project_folders_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: last_project_folders_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.last_project_folders_id_seq OWNED BY public.last_project_folders.id;


--
-- Name: ldap_auth_sources; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ldap_auth_sources (
    id bigint NOT NULL,
    name character varying(60) DEFAULT ''::character varying NOT NULL,
    host character varying(60),
    port integer,
    account character varying,
    account_password character varying DEFAULT ''::character varying,
    base_dn character varying,
    attr_login character varying(30),
    attr_firstname character varying(30),
    attr_lastname character varying(30),
    attr_mail character varying(30),
    onthefly_register boolean DEFAULT false NOT NULL,
    attr_admin character varying,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    tls_mode integer DEFAULT 0 NOT NULL,
    filter_string text,
    verify_peer boolean DEFAULT true NOT NULL,
    tls_certificate_string text
);


--
-- Name: ldap_auth_sources_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ldap_auth_sources_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ldap_auth_sources_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ldap_auth_sources_id_seq OWNED BY public.ldap_auth_sources.id;


--
-- Name: ldap_departments_memberships; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ldap_departments_memberships (
    id bigint NOT NULL,
    user_id bigint,
    synchronized_department_id bigint,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: ldap_departments_memberships_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ldap_departments_memberships_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ldap_departments_memberships_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ldap_departments_memberships_id_seq OWNED BY public.ldap_departments_memberships.id;


--
-- Name: ldap_departments_synchronized_departments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ldap_departments_synchronized_departments (
    id bigint NOT NULL,
    synchronized_tree_id bigint,
    ldap_auth_source_id bigint,
    group_id bigint,
    dn text,
    ldap_entry_uuid character varying,
    users_count integer DEFAULT 0 NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: ldap_departments_synchronized_departments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ldap_departments_synchronized_departments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ldap_departments_synchronized_departments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ldap_departments_synchronized_departments_id_seq OWNED BY public.ldap_departments_synchronized_departments.id;


--
-- Name: ldap_departments_synchronized_trees; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ldap_departments_synchronized_trees (
    id bigint NOT NULL,
    name character varying,
    ldap_auth_source_id bigint,
    base_dn text,
    structure_filter_string character varying DEFAULT '(objectClass=organizationalUnit)'::character varying NOT NULL,
    ou_name_attribute character varying DEFAULT 'ou'::character varying NOT NULL,
    guid_attribute character varying,
    user_filter_string text,
    sync_users boolean DEFAULT false NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: ldap_departments_synchronized_trees_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ldap_departments_synchronized_trees_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ldap_departments_synchronized_trees_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ldap_departments_synchronized_trees_id_seq OWNED BY public.ldap_departments_synchronized_trees.id;


--
-- Name: ldap_groups_memberships; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ldap_groups_memberships (
    id bigint NOT NULL,
    user_id bigint,
    group_id bigint,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: ldap_groups_memberships_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ldap_groups_memberships_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ldap_groups_memberships_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ldap_groups_memberships_id_seq OWNED BY public.ldap_groups_memberships.id;


--
-- Name: ldap_groups_synchronized_filters; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ldap_groups_synchronized_filters (
    id bigint NOT NULL,
    name character varying,
    group_name_attribute character varying,
    filter_string character varying,
    ldap_auth_source_id bigint,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    base_dn text,
    sync_users boolean DEFAULT false NOT NULL,
    member_lookup_attribute character varying
);


--
-- Name: ldap_groups_synchronized_filters_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ldap_groups_synchronized_filters_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ldap_groups_synchronized_filters_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ldap_groups_synchronized_filters_id_seq OWNED BY public.ldap_groups_synchronized_filters.id;


--
-- Name: ldap_groups_synchronized_groups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ldap_groups_synchronized_groups (
    id bigint NOT NULL,
    group_id bigint,
    ldap_auth_source_id bigint,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    dn text,
    users_count integer DEFAULT 0 NOT NULL,
    filter_id bigint,
    sync_users boolean DEFAULT false NOT NULL
);


--
-- Name: ldap_groups_synchronized_groups_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ldap_groups_synchronized_groups_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ldap_groups_synchronized_groups_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ldap_groups_synchronized_groups_id_seq OWNED BY public.ldap_groups_synchronized_groups.id;


--
-- Name: llm_capability_verdicts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.llm_capability_verdicts (
    id bigint NOT NULL,
    llm_connection_id bigint NOT NULL,
    model_id character varying NOT NULL,
    capability character varying NOT NULL,
    state character varying NOT NULL,
    source character varying NOT NULL,
    detail jsonb DEFAULT '{}'::jsonb NOT NULL,
    checked_at timestamp(6) with time zone NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: llm_capability_verdicts_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.llm_capability_verdicts_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: llm_capability_verdicts_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.llm_capability_verdicts_id_seq OWNED BY public.llm_capability_verdicts.id;


--
-- Name: llm_connections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.llm_connections (
    id bigint NOT NULL,
    identifier character varying NOT NULL,
    active boolean DEFAULT true NOT NULL,
    base_url character varying NOT NULL,
    api_key character varying,
    api_format character varying DEFAULT 'openai'::character varying NOT NULL,
    custom_headers jsonb DEFAULT '{}'::jsonb NOT NULL,
    options jsonb DEFAULT '{}'::jsonb NOT NULL,
    last_synced_at timestamp(6) with time zone,
    connection_fingerprint character varying,
    last_connected_at timestamp(6) with time zone,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    default_chat_model_id bigint,
    default_embedding_model_id bigint,
    env_provisioned_at timestamp(6) with time zone,
    env_fingerprint character varying
);


--
-- Name: llm_connections_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.llm_connections_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: llm_connections_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.llm_connections_id_seq OWNED BY public.llm_connections.id;


--
-- Name: llm_feature_bindings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.llm_feature_bindings (
    id bigint NOT NULL,
    llm_connection_id bigint NOT NULL,
    feature_key character varying NOT NULL,
    model_id character varying,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    semantic_index_id bigint
);


--
-- Name: llm_feature_bindings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.llm_feature_bindings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: llm_feature_bindings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.llm_feature_bindings_id_seq OWNED BY public.llm_feature_bindings.id;


--
-- Name: llm_models; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.llm_models (
    id bigint NOT NULL,
    llm_connection_id bigint NOT NULL,
    external_id character varying NOT NULL,
    display_name character varying,
    active boolean DEFAULT true NOT NULL,
    manual boolean DEFAULT false NOT NULL,
    last_seen_at timestamp(6) with time zone,
    raw_metadata jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: llm_models_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.llm_models_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: llm_models_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.llm_models_id_seq OWNED BY public.llm_models.id;


--
-- Name: material_budget_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.material_budget_items (
    id bigint NOT NULL,
    budget_id bigint NOT NULL,
    units double precision NOT NULL,
    cost_type_id bigint,
    comments character varying DEFAULT ''::character varying NOT NULL,
    amount numeric(15,4)
);


--
-- Name: material_budget_items_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.material_budget_items_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: material_budget_items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.material_budget_items_id_seq OWNED BY public.material_budget_items.id;


--
-- Name: mcp_configurations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.mcp_configurations (
    id bigint NOT NULL,
    identifier character varying NOT NULL,
    title character varying NOT NULL,
    description character varying NOT NULL,
    enabled boolean DEFAULT false NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: mcp_configurations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.mcp_configurations_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: mcp_configurations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.mcp_configurations_id_seq OWNED BY public.mcp_configurations.id;


--
-- Name: meeting_agenda_item_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_agenda_item_journals (
    id bigint NOT NULL,
    journal_id integer,
    agenda_item_id integer,
    author_id integer,
    title character varying,
    notes text,
    "position" integer,
    duration_in_minutes integer,
    work_package_id integer,
    item_type smallint,
    presenter_id bigint,
    meeting_section_id bigint
);


--
-- Name: meeting_agenda_item_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.meeting_agenda_item_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: meeting_agenda_item_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.meeting_agenda_item_journals_id_seq OWNED BY public.meeting_agenda_item_journals.id;


--
-- Name: meeting_agenda_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_agenda_items (
    id bigint NOT NULL,
    meeting_id bigint,
    author_id bigint,
    title character varying,
    notes text,
    "position" integer,
    duration_in_minutes integer,
    created_at timestamp without time zone NOT NULL,
    updated_at timestamp without time zone NOT NULL,
    work_package_id bigint,
    item_type smallint DEFAULT 0,
    lock_version integer DEFAULT 0 NOT NULL,
    presenter_id bigint,
    meeting_section_id bigint,
    reactions_changed_at timestamp(6) with time zone
);


--
-- Name: meeting_agenda_items_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.meeting_agenda_items_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: meeting_agenda_items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.meeting_agenda_items_id_seq OWNED BY public.meeting_agenda_items.id;


--
-- Name: meeting_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_journals (
    id bigint NOT NULL,
    title character varying,
    author_id bigint,
    project_id bigint,
    location character varying,
    start_time timestamp with time zone,
    duration double precision,
    state integer DEFAULT 0 NOT NULL
);


--
-- Name: meeting_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.meeting_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: meeting_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.meeting_journals_id_seq OWNED BY public.meeting_journals.id;


--
-- Name: meeting_outcome_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_outcome_journals (
    id bigint NOT NULL,
    journal_id integer NOT NULL,
    outcome_id bigint NOT NULL,
    meeting_agenda_item_id bigint,
    work_package_id bigint,
    kind integer NOT NULL,
    notes text
);


--
-- Name: meeting_outcome_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.meeting_outcome_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: meeting_outcome_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.meeting_outcome_journals_id_seq OWNED BY public.meeting_outcome_journals.id;


--
-- Name: meeting_outcomes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_outcomes (
    id bigint NOT NULL,
    notes text,
    meeting_agenda_item_id bigint,
    work_package_id bigint,
    kind integer DEFAULT 0 NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    author_id bigint
);


--
-- Name: meeting_outcomes_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.meeting_outcomes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: meeting_outcomes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.meeting_outcomes_id_seq OWNED BY public.meeting_outcomes.id;


--
-- Name: meeting_participant_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_participant_journals (
    id bigint NOT NULL,
    journal_id integer NOT NULL,
    user_id integer NOT NULL,
    invited boolean DEFAULT false NOT NULL,
    attended boolean DEFAULT false NOT NULL,
    participation_status character varying
);


--
-- Name: meeting_participant_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.meeting_participant_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: meeting_participant_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.meeting_participant_journals_id_seq OWNED BY public.meeting_participant_journals.id;


--
-- Name: meeting_participants; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_participants (
    id bigint NOT NULL,
    user_id bigint,
    meeting_id bigint,
    email character varying,
    name character varying,
    invited boolean,
    attended boolean,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    participation_status character varying DEFAULT 'needs-action'::character varying NOT NULL,
    comment text
);


--
-- Name: meeting_participants_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.meeting_participants_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: meeting_participants_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.meeting_participants_id_seq OWNED BY public.meeting_participants.id;


--
-- Name: meeting_section_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_section_journals (
    id bigint NOT NULL,
    journal_id integer NOT NULL,
    section_id bigint NOT NULL,
    title character varying,
    "position" integer,
    backlog boolean DEFAULT false NOT NULL
);


--
-- Name: meeting_section_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.meeting_section_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: meeting_section_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.meeting_section_journals_id_seq OWNED BY public.meeting_section_journals.id;


--
-- Name: meeting_sections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_sections (
    id bigint NOT NULL,
    "position" integer,
    title character varying,
    meeting_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    backlog boolean DEFAULT false NOT NULL
);


--
-- Name: meeting_sections_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.meeting_sections_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: meeting_sections_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.meeting_sections_id_seq OWNED BY public.meeting_sections.id;


--
-- Name: meetings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meetings (
    id bigint NOT NULL,
    title character varying,
    author_id bigint,
    project_id bigint,
    location character varying,
    start_time timestamp with time zone,
    duration double precision,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    state integer DEFAULT 0 NOT NULL,
    lock_version integer DEFAULT 0 NOT NULL,
    recurring_meeting_id bigint,
    template boolean DEFAULT false NOT NULL,
    uid character varying,
    notify boolean DEFAULT true NOT NULL,
    sharing character varying,
    recurrence_start_time timestamp with time zone,
    agenda_sorting_mode character varying DEFAULT 'manual'::character varying NOT NULL,
    CONSTRAINT recurrence_start_time_absent_for_templates CHECK (((template = false) OR (recurrence_start_time IS NULL))),
    CONSTRAINT recurrence_start_time_present_for_occurrences CHECK ((template OR (recurring_meeting_id IS NULL) OR (recurrence_start_time IS NOT NULL)))
);


--
-- Name: meetings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.meetings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: meetings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.meetings_id_seq OWNED BY public.meetings.id;


--
-- Name: member_roles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.member_roles (
    id bigint NOT NULL,
    member_id bigint NOT NULL,
    role_id bigint NOT NULL,
    inherited_from bigint
);


--
-- Name: member_roles_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.member_roles_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: member_roles_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.member_roles_id_seq OWNED BY public.member_roles.id;


--
-- Name: members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.members (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    project_id bigint,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    entity_type character varying,
    entity_id bigint
);


--
-- Name: members_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.members_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: members_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.members_id_seq OWNED BY public.members.id;


--
-- Name: menu_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.menu_items (
    id bigint NOT NULL,
    name character varying,
    title character varying,
    parent_id bigint,
    options text,
    navigatable_id bigint,
    type character varying
);


--
-- Name: menu_items_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.menu_items_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: menu_items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.menu_items_id_seq OWNED BY public.menu_items.id;


--
-- Name: message_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.message_journals (
    id bigint NOT NULL,
    forum_id bigint NOT NULL,
    parent_id bigint,
    subject character varying NOT NULL,
    content text,
    author_id bigint,
    locked boolean,
    sticky integer
);


--
-- Name: message_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.message_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: message_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.message_journals_id_seq OWNED BY public.message_journals.id;


--
-- Name: message_work_packages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.message_work_packages (
    id bigint NOT NULL,
    message_id bigint NOT NULL,
    work_package_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: message_work_packages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.message_work_packages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: message_work_packages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.message_work_packages_id_seq OWNED BY public.message_work_packages.id;


--
-- Name: messages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.messages (
    id bigint NOT NULL,
    forum_id bigint NOT NULL,
    parent_id bigint,
    subject character varying DEFAULT ''::character varying NOT NULL,
    content text,
    author_id bigint,
    replies_count integer DEFAULT 0 NOT NULL,
    last_reply_id bigint,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    locked boolean DEFAULT false,
    sticky integer DEFAULT 0,
    sticked_on timestamp with time zone,
    accepted_solution boolean DEFAULT false NOT NULL
);


--
-- Name: messages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.messages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: messages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.messages_id_seq OWNED BY public.messages.id;


--
-- Name: news; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.news (
    id bigint NOT NULL,
    project_id bigint,
    title character varying DEFAULT ''::character varying NOT NULL,
    summary character varying DEFAULT ''::character varying,
    description text,
    author_id bigint NOT NULL,
    created_at timestamp with time zone,
    comments_count integer DEFAULT 0 NOT NULL,
    updated_at timestamp with time zone
);


--
-- Name: news_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.news_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: news_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.news_id_seq OWNED BY public.news.id;


--
-- Name: news_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.news_journals (
    id bigint NOT NULL,
    project_id bigint,
    title character varying NOT NULL,
    summary character varying,
    description text,
    author_id bigint NOT NULL,
    comments_count integer NOT NULL
);


--
-- Name: news_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.news_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: news_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.news_journals_id_seq OWNED BY public.news_journals.id;


--
-- Name: non_working_days; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.non_working_days (
    id bigint NOT NULL,
    name character varying NOT NULL,
    date date NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: non_working_days_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.non_working_days_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: non_working_days_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.non_working_days_id_seq OWNED BY public.non_working_days.id;


--
-- Name: notification_settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.notification_settings (
    id bigint NOT NULL,
    project_id bigint,
    user_id bigint NOT NULL,
    watched boolean DEFAULT true,
    mentioned boolean DEFAULT true,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    work_package_commented boolean DEFAULT false,
    work_package_created boolean DEFAULT false,
    work_package_processed boolean DEFAULT false,
    work_package_prioritized boolean DEFAULT false,
    work_package_scheduled boolean DEFAULT false,
    news_added boolean DEFAULT false,
    news_commented boolean DEFAULT false,
    document_added boolean DEFAULT false,
    forum_messages boolean DEFAULT false,
    wiki_page_added boolean DEFAULT false,
    wiki_page_updated boolean DEFAULT false,
    membership_added boolean DEFAULT false,
    membership_updated boolean DEFAULT false,
    start_date integer DEFAULT 1,
    due_date integer DEFAULT 1,
    overdue integer,
    assignee boolean DEFAULT true NOT NULL,
    responsible boolean DEFAULT true NOT NULL,
    shared boolean DEFAULT true NOT NULL
);


--
-- Name: notification_settings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.notification_settings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: notification_settings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.notification_settings_id_seq OWNED BY public.notification_settings.id;


--
-- Name: notifications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.notifications (
    id bigint NOT NULL,
    subject text,
    read_ian boolean DEFAULT false,
    reason smallint,
    recipient_id bigint NOT NULL,
    actor_id bigint,
    resource_type character varying NOT NULL,
    resource_id bigint NOT NULL,
    journal_id bigint,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    mail_reminder_sent boolean DEFAULT false,
    mail_alert_sent boolean
);


--
-- Name: notifications_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.notifications_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: notifications_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.notifications_id_seq OWNED BY public.notifications.id;


--
-- Name: oauth_access_grants; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oauth_access_grants (
    id bigint NOT NULL,
    resource_owner_id bigint NOT NULL,
    application_id bigint NOT NULL,
    token character varying NOT NULL,
    expires_in integer NOT NULL,
    redirect_uri text NOT NULL,
    created_at timestamp with time zone NOT NULL,
    revoked_at timestamp with time zone,
    scopes character varying,
    code_challenge character varying,
    code_challenge_method character varying
);


--
-- Name: oauth_access_grants_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.oauth_access_grants_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: oauth_access_grants_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.oauth_access_grants_id_seq OWNED BY public.oauth_access_grants.id;


--
-- Name: oauth_access_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oauth_access_tokens (
    id bigint NOT NULL,
    resource_owner_id bigint,
    application_id bigint,
    token character varying NOT NULL,
    refresh_token character varying,
    expires_in integer,
    revoked_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL,
    scopes character varying,
    previous_refresh_token character varying DEFAULT ''::character varying NOT NULL
);


--
-- Name: oauth_access_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.oauth_access_tokens_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: oauth_access_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.oauth_access_tokens_id_seq OWNED BY public.oauth_access_tokens.id;


--
-- Name: oauth_applications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oauth_applications (
    id bigint NOT NULL,
    name character varying NOT NULL,
    uid character varying NOT NULL,
    secret character varying NOT NULL,
    owner_type character varying,
    owner_id bigint,
    client_credentials_user_id bigint,
    redirect_uri text NOT NULL,
    scopes character varying DEFAULT ''::character varying NOT NULL,
    confidential boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    integration_type character varying,
    integration_id bigint,
    enabled boolean DEFAULT true NOT NULL,
    builtin boolean DEFAULT false NOT NULL
);


--
-- Name: oauth_applications_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.oauth_applications_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: oauth_applications_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.oauth_applications_id_seq OWNED BY public.oauth_applications.id;


--
-- Name: oauth_client_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oauth_client_tokens (
    id bigint NOT NULL,
    oauth_client_id bigint NOT NULL,
    user_id bigint NOT NULL,
    access_token character varying,
    refresh_token character varying,
    token_type character varying,
    expires_in integer,
    scope character varying,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    lock_version integer DEFAULT 0 NOT NULL
);


--
-- Name: oauth_client_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.oauth_client_tokens_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: oauth_client_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.oauth_client_tokens_id_seq OWNED BY public.oauth_client_tokens.id;


--
-- Name: oauth_clients; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oauth_clients (
    id bigint NOT NULL,
    client_id character varying NOT NULL,
    client_secret character varying,
    integration_type character varying NOT NULL,
    integration_id bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: oauth_clients_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.oauth_clients_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: oauth_clients_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.oauth_clients_id_seq OWNED BY public.oauth_clients.id;


--
-- Name: oidc_group_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oidc_group_links (
    id bigint NOT NULL,
    group_id bigint NOT NULL,
    auth_provider_id bigint NOT NULL,
    oidc_group_name character varying NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: oidc_group_links_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.oidc_group_links_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: oidc_group_links_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.oidc_group_links_id_seq OWNED BY public.oidc_group_links.id;


--
-- Name: oidc_group_memberships; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oidc_group_memberships (
    id bigint NOT NULL,
    auth_provider_id bigint NOT NULL,
    group_user_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: oidc_group_memberships_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.oidc_group_memberships_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: oidc_group_memberships_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.oidc_group_memberships_id_seq OWNED BY public.oidc_group_memberships.id;


--
-- Name: oidc_user_session_links; Type: TABLE; Schema: public; Owner: -
--

CREATE UNLOGGED TABLE public.oidc_user_session_links (
    id bigint NOT NULL,
    oidc_session character varying NOT NULL,
    session_id bigint,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: oidc_user_session_links_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE UNLOGGED SEQUENCE public.oidc_user_session_links_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: oidc_user_session_links_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.oidc_user_session_links_id_seq OWNED BY public.oidc_user_session_links.id;


--
-- Name: oidc_user_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oidc_user_tokens (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    access_token character varying NOT NULL,
    refresh_token character varying,
    audiences jsonb DEFAULT '[]'::jsonb NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    expires_at timestamp(0) with time zone
);


--
-- Name: oidc_user_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.oidc_user_tokens_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: oidc_user_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.oidc_user_tokens_id_seq OWNED BY public.oidc_user_tokens.id;


--
-- Name: ordered_persisted_query_entities; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ordered_persisted_query_entities (
    id bigint NOT NULL,
    "position" integer NOT NULL,
    persisted_query_id bigint NOT NULL,
    entity_type character varying NOT NULL,
    entity_id bigint NOT NULL
);


--
-- Name: ordered_persisted_query_entities_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ordered_persisted_query_entities_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ordered_persisted_query_entities_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ordered_persisted_query_entities_id_seq OWNED BY public.ordered_persisted_query_entities.id;


--
-- Name: ordered_work_packages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ordered_work_packages (
    id bigint NOT NULL,
    "position" integer NOT NULL,
    query_id bigint,
    work_package_id bigint
);


--
-- Name: ordered_work_packages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ordered_work_packages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ordered_work_packages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ordered_work_packages_id_seq OWNED BY public.ordered_work_packages.id;


--
-- Name: outbound_mail_recipients; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.outbound_mail_recipients (
    id bigint NOT NULL,
    mail character varying NOT NULL,
    sent_on date NOT NULL,
    created_at timestamp(6) with time zone NOT NULL
);


--
-- Name: outbound_mail_recipients_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.outbound_mail_recipients_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: outbound_mail_recipients_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.outbound_mail_recipients_id_seq OWNED BY public.outbound_mail_recipients.id;


--
-- Name: paper_trail_audits; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.paper_trail_audits (
    id bigint NOT NULL,
    item_type character varying NOT NULL,
    item_id bigint NOT NULL,
    event character varying NOT NULL,
    whodunnit character varying,
    stack text,
    object jsonb,
    object_changes jsonb,
    created_at timestamp(6) with time zone
);


--
-- Name: paper_trail_audits_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.paper_trail_audits_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: paper_trail_audits_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.paper_trail_audits_id_seq OWNED BY public.paper_trail_audits.id;


--
-- Name: persisted_queries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.persisted_queries (
    id bigint NOT NULL,
    type character varying,
    name character varying,
    project_id bigint,
    principal_id bigint,
    filters jsonb DEFAULT '[]'::jsonb,
    selects jsonb DEFAULT '[]'::jsonb,
    orders jsonb DEFAULT '[]'::jsonb,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    manual_elements boolean DEFAULT false NOT NULL,
    group_bys jsonb DEFAULT '[]'::jsonb
);


--
-- Name: persisted_queries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.persisted_queries_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: persisted_queries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.persisted_queries_id_seq OWNED BY public.persisted_queries.id;


--
-- Name: persisted_views; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.persisted_views (
    id bigint NOT NULL,
    type character varying,
    name character varying,
    project_id bigint,
    principal_id bigint,
    query_type character varying,
    query_id bigint,
    parent_id bigint,
    options jsonb DEFAULT '{}'::jsonb NOT NULL,
    public boolean DEFAULT false NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    category character varying
);


--
-- Name: persisted_views_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.persisted_views_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: persisted_views_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.persisted_views_id_seq OWNED BY public.persisted_views.id;


--
-- Name: placeholder_user_details; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.placeholder_user_details (
    id bigint NOT NULL,
    principal_id bigint NOT NULL,
    user_filter jsonb DEFAULT '[]'::jsonb,
    description text,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: placeholder_user_details_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.placeholder_user_details_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: placeholder_user_details_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.placeholder_user_details_id_seq OWNED BY public.placeholder_user_details.id;


--
-- Name: project_custom_field_project_mappings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_custom_field_project_mappings (
    id bigint NOT NULL,
    custom_field_id bigint,
    project_id bigint,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    creation_wizard boolean DEFAULT false NOT NULL
);


--
-- Name: project_custom_field_project_mappings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.project_custom_field_project_mappings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: project_custom_field_project_mappings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.project_custom_field_project_mappings_id_seq OWNED BY public.project_custom_field_project_mappings.id;


--
-- Name: project_custom_field_type_mappings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_custom_field_type_mappings (
    id bigint NOT NULL,
    custom_field_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    type_variant_id bigint NOT NULL
);


--
-- Name: project_custom_field_type_mappings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.project_custom_field_type_mappings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: project_custom_field_type_mappings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.project_custom_field_type_mappings_id_seq OWNED BY public.project_custom_field_type_mappings.id;


--
-- Name: project_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_journals (
    id bigint NOT NULL,
    name character varying NOT NULL,
    description text,
    public boolean NOT NULL,
    parent_id bigint,
    identifier character varying NOT NULL,
    active boolean NOT NULL,
    templated boolean NOT NULL,
    status_code integer,
    status_explanation text,
    workspace_type character varying NOT NULL
);


--
-- Name: project_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.project_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: project_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.project_journals_id_seq OWNED BY public.project_journals.id;


--
-- Name: project_phase_definitions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_phase_definitions (
    id bigint NOT NULL,
    name character varying,
    "position" integer,
    color_id bigint,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    start_gate boolean DEFAULT false NOT NULL,
    start_gate_name character varying,
    finish_gate boolean DEFAULT false NOT NULL,
    finish_gate_name character varying
);


--
-- Name: project_phase_definitions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.project_phase_definitions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: project_phase_definitions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.project_phase_definitions_id_seq OWNED BY public.project_phase_definitions.id;


--
-- Name: project_phase_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_phase_journals (
    id bigint NOT NULL,
    journal_id bigint NOT NULL,
    phase_id bigint NOT NULL,
    start_date date,
    finish_date date,
    active boolean DEFAULT false NOT NULL,
    duration integer,
    status character varying DEFAULT 'open'::character varying NOT NULL
);


--
-- Name: project_phase_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.project_phase_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: project_phase_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.project_phase_journals_id_seq OWNED BY public.project_phase_journals.id;


--
-- Name: project_phases; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_phases (
    id bigint NOT NULL,
    start_date date,
    finish_date date,
    active boolean DEFAULT false NOT NULL,
    project_id bigint NOT NULL,
    definition_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    duration integer,
    status character varying DEFAULT 'open'::character varying NOT NULL,
    CONSTRAINT project_phases_status_valid CHECK (((status)::text = ANY ((ARRAY['open'::character varying, 'in_progress'::character varying, 'completed'::character varying])::text[])))
);


--
-- Name: project_phases_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.project_phases_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: project_phases_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.project_phases_id_seq OWNED BY public.project_phases.id;


--
-- Name: project_queries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_queries (
    id bigint NOT NULL,
    name character varying NOT NULL,
    user_id bigint NOT NULL,
    filters json DEFAULT '[]'::json,
    selects json DEFAULT '[]'::json,
    orders json DEFAULT '[]'::json,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    public boolean DEFAULT false NOT NULL
);


--
-- Name: project_queries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.project_queries_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: project_queries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.project_queries_id_seq OWNED BY public.project_queries.id;


--
-- Name: project_storages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_storages (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    storage_id bigint NOT NULL,
    creator_id bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    project_folder_id character varying,
    project_folder_mode character varying NOT NULL,
    CONSTRAINT project_storages_project_folder_mode_check CHECK (((project_folder_mode)::text = ANY ((ARRAY['inactive'::character varying, 'manual'::character varying, 'automatic'::character varying])::text[])))
);


--
-- Name: project_storages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.project_storages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: project_storages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.project_storages_id_seq OWNED BY public.project_storages.id;


--
-- Name: project_types; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.project_types (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    type_id bigint NOT NULL,
    variant_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: project_types_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.project_types_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: project_types_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.project_types_id_seq OWNED BY public.project_types.id;


--
-- Name: projects; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.projects (
    id bigint NOT NULL,
    name character varying DEFAULT ''::character varying NOT NULL,
    description text,
    public boolean DEFAULT true NOT NULL,
    parent_id bigint,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    identifier character varying NOT NULL,
    lft integer,
    rgt integer,
    active boolean DEFAULT true NOT NULL,
    templated boolean DEFAULT false NOT NULL,
    status_code integer,
    status_explanation text,
    settings jsonb DEFAULT '{}'::jsonb NOT NULL,
    workspace_type character varying NOT NULL,
    template_id bigint,
    wp_sequence_counter integer DEFAULT 0 NOT NULL
);


--
-- Name: projects_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.projects_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: projects_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.projects_id_seq OWNED BY public.projects.id;


--
-- Name: push_subscriptions; Type: TABLE; Schema: public; Owner: -
--

CREATE UNLOGGED TABLE public.push_subscriptions (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    session_id bigint,
    endpoint text NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    browser_name character varying,
    browser_version character varying,
    platform character varying,
    suspended_at timestamp(6) with time zone
);


--
-- Name: push_subscriptions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE UNLOGGED SEQUENCE public.push_subscriptions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: push_subscriptions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.push_subscriptions_id_seq OWNED BY public.push_subscriptions.id;


--
-- Name: queries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.queries (
    id bigint NOT NULL,
    project_id bigint,
    name character varying NOT NULL,
    filters text,
    user_id bigint NOT NULL,
    public boolean DEFAULT false NOT NULL,
    column_names text,
    sort_criteria text,
    group_by character varying,
    display_sums boolean DEFAULT false NOT NULL,
    timeline_visible boolean DEFAULT false,
    show_hierarchies boolean DEFAULT false,
    timeline_zoom_level integer DEFAULT 5,
    timeline_labels text,
    highlighting_mode text,
    highlighted_attributes text,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    display_representation text,
    starred boolean DEFAULT false,
    include_subprojects boolean NOT NULL,
    timestamps character varying
);


--
-- Name: queries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.queries_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: queries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.queries_id_seq OWNED BY public.queries.id;


--
-- Name: rates; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.rates (
    id bigint NOT NULL,
    valid_from date NOT NULL,
    rate numeric(15,4) NOT NULL,
    type character varying NOT NULL,
    project_id bigint,
    user_id bigint,
    cost_type_id bigint
);


--
-- Name: rates_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.rates_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: rates_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.rates_id_seq OWNED BY public.rates.id;


--
-- Name: recaptcha_entries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.recaptcha_entries (
    id integer NOT NULL,
    user_id bigint,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    version integer NOT NULL
);


--
-- Name: recaptcha_entries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.recaptcha_entries_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: recaptcha_entries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.recaptcha_entries_id_seq OWNED BY public.recaptcha_entries.id;


--
-- Name: recurring_meeting_historic_schedules; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.recurring_meeting_historic_schedules (
    id bigint NOT NULL,
    recurring_meeting_id bigint NOT NULL,
    uid character varying NOT NULL,
    snapshot jsonb NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: recurring_meeting_historic_schedules_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.recurring_meeting_historic_schedules_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: recurring_meeting_historic_schedules_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.recurring_meeting_historic_schedules_id_seq OWNED BY public.recurring_meeting_historic_schedules.id;


--
-- Name: recurring_meeting_interim_responses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.recurring_meeting_interim_responses (
    id bigint NOT NULL,
    recurring_meeting_id bigint,
    user_id bigint,
    start_time timestamp(6) with time zone,
    participation_status character varying,
    comment text,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: recurring_meeting_interim_responses_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.recurring_meeting_interim_responses_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: recurring_meeting_interim_responses_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.recurring_meeting_interim_responses_id_seq OWNED BY public.recurring_meeting_interim_responses.id;


--
-- Name: recurring_meetings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.recurring_meetings (
    id bigint NOT NULL,
    start_time timestamp(6) with time zone,
    end_date date,
    title text,
    frequency integer DEFAULT 0 NOT NULL,
    end_after integer DEFAULT 0 NOT NULL,
    iterations integer,
    project_id bigint,
    author_id bigint,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    "interval" integer DEFAULT 1 NOT NULL,
    time_zone character varying NOT NULL,
    uid character varying,
    current_schedule_start timestamp(6) with time zone NOT NULL,
    monthly_day integer,
    monthly_ordinal integer,
    monthly_weekday character varying,
    ical_sequence integer DEFAULT 0 NOT NULL
);


--
-- Name: recurring_meetings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.recurring_meetings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: recurring_meetings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.recurring_meetings_id_seq OWNED BY public.recurring_meetings.id;


--
-- Name: relations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.relations (
    id bigint NOT NULL,
    from_id integer NOT NULL,
    to_id integer NOT NULL,
    lag integer,
    description text,
    relation_type character varying
);


--
-- Name: relations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.relations_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: relations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.relations_id_seq OWNED BY public.relations.id;


--
-- Name: reminder_notifications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.reminder_notifications (
    id bigint NOT NULL,
    reminder_id bigint,
    notification_id bigint,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: reminder_notifications_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.reminder_notifications_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: reminder_notifications_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.reminder_notifications_id_seq OWNED BY public.reminder_notifications.id;


--
-- Name: reminders; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.reminders (
    id bigint NOT NULL,
    remindable_type character varying NOT NULL,
    remindable_id bigint NOT NULL,
    creator_id bigint NOT NULL,
    remind_at timestamp(6) with time zone NOT NULL,
    completed_at timestamp(6) with time zone,
    job_id character varying,
    note text,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: reminders_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.reminders_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: reminders_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.reminders_id_seq OWNED BY public.reminders.id;


--
-- Name: remote_identities; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.remote_identities (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    auth_source_id bigint NOT NULL,
    origin_user_id character varying NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    auth_source_type character varying NOT NULL,
    integration_type character varying NOT NULL,
    integration_id bigint NOT NULL
);


--
-- Name: remote_identities_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.remote_identities_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: remote_identities_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.remote_identities_id_seq OWNED BY public.remote_identities.id;


--
-- Name: repositories; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repositories (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    url character varying DEFAULT ''::character varying NOT NULL,
    login character varying(60) DEFAULT ''::character varying,
    password character varying DEFAULT ''::character varying,
    root_url character varying DEFAULT ''::character varying,
    type character varying,
    path_encoding character varying(64),
    log_encoding character varying(64),
    scm_type character varying NOT NULL,
    required_storage_bytes bigint DEFAULT 0 NOT NULL,
    storage_updated_at timestamp with time zone
);


--
-- Name: repositories_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.repositories_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: repositories_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.repositories_id_seq OWNED BY public.repositories.id;


--
-- Name: repository_branches; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_branches (
    id bigint NOT NULL,
    provider_id bigint NOT NULL,
    work_package_id bigint NOT NULL,
    repository_user_id bigint,
    name character varying NOT NULL,
    external_project_id character varying NOT NULL,
    project_name character varying NOT NULL,
    project_web_url character varying NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: repository_branches_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.repository_branches_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: repository_branches_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.repository_branches_id_seq OWNED BY public.repository_branches.id;


--
-- Name: repository_commit_work_packages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_commit_work_packages (
    id bigint NOT NULL,
    commit_id bigint NOT NULL,
    work_package_id bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: repository_commit_work_packages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.repository_commit_work_packages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: repository_commit_work_packages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.repository_commit_work_packages_id_seq OWNED BY public.repository_commit_work_packages.id;


--
-- Name: repository_commits; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_commits (
    id bigint NOT NULL,
    provider_id bigint NOT NULL,
    repository_user_id bigint,
    sha character varying NOT NULL,
    message text DEFAULT ''::text NOT NULL,
    author_name character varying NOT NULL,
    author_email character varying NOT NULL,
    authored_at timestamp with time zone NOT NULL,
    project_name character varying NOT NULL,
    web_url character varying NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: repository_commits_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.repository_commits_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: repository_commits_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.repository_commits_id_seq OWNED BY public.repository_commits.id;


--
-- Name: repository_issue_work_packages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_issue_work_packages (
    id bigint NOT NULL,
    issue_id bigint NOT NULL,
    work_package_id bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: repository_issue_work_packages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.repository_issue_work_packages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: repository_issue_work_packages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.repository_issue_work_packages_id_seq OWNED BY public.repository_issue_work_packages.id;


--
-- Name: repository_issues; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_issues (
    id bigint NOT NULL,
    provider_id bigint NOT NULL,
    repository_user_id bigint,
    external_id character varying NOT NULL,
    display_id character varying NOT NULL,
    web_url character varying NOT NULL,
    state character varying NOT NULL,
    project_name character varying NOT NULL,
    title character varying DEFAULT ''::character varying NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    labels jsonb DEFAULT '[]'::jsonb NOT NULL,
    external_updated_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: repository_issues_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.repository_issues_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: repository_issues_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.repository_issues_id_seq OWNED BY public.repository_issues.id;


--
-- Name: repository_merge_request_work_packages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_merge_request_work_packages (
    id bigint NOT NULL,
    merge_request_id bigint NOT NULL,
    work_package_id bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: repository_merge_request_work_packages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.repository_merge_request_work_packages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: repository_merge_request_work_packages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.repository_merge_request_work_packages_id_seq OWNED BY public.repository_merge_request_work_packages.id;


--
-- Name: repository_merge_requests; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_merge_requests (
    id bigint NOT NULL,
    provider_id bigint NOT NULL,
    repository_user_id bigint,
    external_id character varying NOT NULL,
    display_id character varying NOT NULL,
    web_url character varying NOT NULL,
    state character varying NOT NULL,
    project_name character varying NOT NULL,
    title character varying DEFAULT ''::character varying NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    labels jsonb DEFAULT '[]'::jsonb NOT NULL,
    external_updated_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: repository_merge_requests_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.repository_merge_requests_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: repository_merge_requests_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.repository_merge_requests_id_seq OWNED BY public.repository_merge_requests.id;


--
-- Name: repository_pipelines; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_pipelines (
    id bigint NOT NULL,
    provider_id bigint NOT NULL,
    merge_request_id bigint NOT NULL,
    repository_user_id bigint,
    external_id bigint NOT NULL,
    external_project_id character varying NOT NULL,
    status character varying NOT NULL,
    web_url character varying NOT NULL,
    details_url character varying,
    ci_details jsonb,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: repository_pipelines_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.repository_pipelines_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: repository_pipelines_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.repository_pipelines_id_seq OWNED BY public.repository_pipelines.id;


--
-- Name: repository_providers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_providers (
    id bigint NOT NULL,
    name character varying NOT NULL,
    type character varying NOT NULL,
    options jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: repository_providers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.repository_providers_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: repository_providers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.repository_providers_id_seq OWNED BY public.repository_providers.id;


--
-- Name: repository_users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_users (
    id bigint NOT NULL,
    provider_id bigint NOT NULL,
    external_id character varying NOT NULL,
    name character varying NOT NULL,
    username character varying NOT NULL,
    avatar_url character varying NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: repository_users_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.repository_users_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: repository_users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.repository_users_id_seq OWNED BY public.repository_users.id;


--
-- Name: resource_allocation_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.resource_allocation_journals (
    id bigint NOT NULL,
    entity_type character varying,
    entity_id bigint,
    principal_id bigint,
    state character varying,
    start_date date,
    end_date date,
    allocated_time integer,
    requested_by_id bigint,
    reviewed_by_id bigint,
    principal_assigned_by_id bigint,
    placeholder_user_id bigint
);


--
-- Name: resource_allocation_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.resource_allocation_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: resource_allocation_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.resource_allocation_journals_id_seq OWNED BY public.resource_allocation_journals.id;


--
-- Name: resource_allocation_scheduled_entries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.resource_allocation_scheduled_entries (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    resource_allocation_id bigint NOT NULL,
    allocated_on date NOT NULL,
    minutes integer NOT NULL,
    overtime_minutes integer DEFAULT 0 NOT NULL
);


--
-- Name: resource_allocation_scheduled_entries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.resource_allocation_scheduled_entries_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: resource_allocation_scheduled_entries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.resource_allocation_scheduled_entries_id_seq OWNED BY public.resource_allocation_scheduled_entries.id;


--
-- Name: resource_allocations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.resource_allocations (
    id bigint NOT NULL,
    entity_type character varying NOT NULL,
    entity_id bigint NOT NULL,
    principal_id bigint,
    state character varying NOT NULL,
    start_date date NOT NULL,
    end_date date NOT NULL,
    allocated_time integer NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    requested_by_id bigint,
    reviewed_by_id bigint,
    principal_assigned_by_id bigint,
    placeholder_user_id bigint
);


--
-- Name: resource_allocations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.resource_allocations_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: resource_allocations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.resource_allocations_id_seq OWNED BY public.resource_allocations.id;


--
-- Name: role_permissions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.role_permissions (
    id bigint NOT NULL,
    permission character varying,
    role_id bigint,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: role_permissions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.role_permissions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: role_permissions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.role_permissions_id_seq OWNED BY public.role_permissions.id;


--
-- Name: roles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.roles (
    id bigint NOT NULL,
    name character varying DEFAULT ''::character varying NOT NULL,
    "position" integer DEFAULT 1,
    builtin integer DEFAULT 0 NOT NULL,
    type character varying(30) DEFAULT 'Role'::character varying,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
);


--
-- Name: roles_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.roles_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: roles_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.roles_id_seq OWNED BY public.roles.id;


--
-- Name: schema_migrations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.schema_migrations (
    version character varying NOT NULL
);


--
-- Name: scim_clients; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.scim_clients (
    id bigint NOT NULL,
    name character varying NOT NULL,
    auth_provider_id bigint NOT NULL,
    authentication_method integer NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: scim_clients_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.scim_clients_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: scim_clients_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.scim_clients_id_seq OWNED BY public.scim_clients.id;


--
-- Name: semantic_indexes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.semantic_indexes (
    id bigint NOT NULL,
    title character varying NOT NULL,
    description text,
    model_id character varying(512) NOT NULL,
    embedding_vector_dimensions integer NOT NULL,
    context_token_length integer NOT NULL,
    state character varying DEFAULT 'not_indexed'::character varying NOT NULL,
    error_message text,
    indexing_batch_id uuid,
    uuid uuid,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: semantic_indexes_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.semantic_indexes_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: semantic_indexes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.semantic_indexes_id_seq OWNED BY public.semantic_indexes.id;


--
-- Name: service_account_associations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.service_account_associations (
    id bigint NOT NULL,
    service_account_id bigint NOT NULL,
    service_id bigint NOT NULL,
    service_type character varying NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: service_account_associations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.service_account_associations_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: service_account_associations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.service_account_associations_id_seq OWNED BY public.service_account_associations.id;


--
-- Name: sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE UNLOGGED TABLE public.sessions (
    id bigint NOT NULL,
    session_id character varying NOT NULL,
    data text,
    updated_at timestamp with time zone,
    user_id bigint
);


--
-- Name: sessions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE UNLOGGED SEQUENCE public.sessions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: sessions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.sessions_id_seq OWNED BY public.sessions.id;


--
-- Name: settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.settings (
    id bigint NOT NULL,
    name character varying DEFAULT ''::character varying NOT NULL,
    value text,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
);


--
-- Name: settings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.settings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: settings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.settings_id_seq OWNED BY public.settings.id;


--
-- Name: sprint_goals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.sprint_goals (
    id bigint NOT NULL,
    sprint_id bigint NOT NULL,
    project_id bigint NOT NULL,
    text text NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: sprint_goals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.sprint_goals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: sprint_goals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.sprint_goals_id_seq OWNED BY public.sprint_goals.id;


--
-- Name: sprints; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.sprints (
    id bigint NOT NULL,
    name character varying NOT NULL COLLATE public.natural_sort_order,
    status character varying DEFAULT 'in_planning'::character varying NOT NULL,
    start_date date,
    finish_date date,
    project_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    started_at timestamp(6) with time zone,
    completed_at timestamp(6) with time zone,
    origin_source character varying,
    origin_id character varying,
    origin_url character varying,
    origin_host character varying,
    CONSTRAINT sprint_finish_date_after_start_date CHECK ((finish_date >= start_date))
);


--
-- Name: sprints_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.sprints_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: sprints_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.sprints_id_seq OWNED BY public.sprints.id;


--
-- Name: squashtm_project_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.squashtm_project_links (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    squashtm_project_id bigint NOT NULL,
    squashtm_project_name character varying NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: squashtm_project_links_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.squashtm_project_links_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: squashtm_project_links_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.squashtm_project_links_id_seq OWNED BY public.squashtm_project_links.id;


--
-- Name: squashtm_servers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.squashtm_servers (
    id bigint NOT NULL,
    base_url character varying NOT NULL,
    api_token text NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: squashtm_servers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.squashtm_servers_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: squashtm_servers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.squashtm_servers_id_seq OWNED BY public.squashtm_servers.id;


--
-- Name: squashtm_work_package_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.squashtm_work_package_links (
    id bigint NOT NULL,
    work_package_id bigint NOT NULL,
    squashtm_requirement_id bigint NOT NULL,
    squashtm_requirement_name character varying NOT NULL,
    squashtm_requirement_reference character varying,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: squashtm_work_package_links_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.squashtm_work_package_links_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: squashtm_work_package_links_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.squashtm_work_package_links_id_seq OWNED BY public.squashtm_work_package_links.id;


--
-- Name: statuses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.statuses (
    id bigint NOT NULL,
    name character varying DEFAULT ''::character varying NOT NULL,
    is_default boolean DEFAULT false NOT NULL,
    "position" integer DEFAULT 1,
    default_done_ratio integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    color_id bigint,
    is_readonly boolean DEFAULT false,
    excluded_from_totals boolean DEFAULT false NOT NULL,
    category character varying
);


--
-- Name: statuses_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.statuses_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: statuses_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.statuses_id_seq OWNED BY public.statuses.id;


--
-- Name: storages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.storages (
    id bigint NOT NULL,
    provider_type character varying NOT NULL,
    name character varying NOT NULL,
    host character varying,
    creator_id bigint NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    provider_fields jsonb DEFAULT '{}'::jsonb NOT NULL,
    health_status character varying DEFAULT 'pending'::character varying NOT NULL,
    health_changed_at timestamp(6) with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    health_reason character varying,
    health_checked_at timestamp(6) with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT storages_health_status_check CHECK (((health_status)::text = ANY ((ARRAY['pending'::character varying, 'healthy'::character varying, 'unhealthy'::character varying])::text[])))
);


--
-- Name: storages_file_links_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.storages_file_links_journals (
    id bigint NOT NULL,
    journal_id bigint NOT NULL,
    file_link_id bigint NOT NULL,
    link_name character varying NOT NULL,
    storage_name character varying
);


--
-- Name: storages_file_links_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.storages_file_links_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: storages_file_links_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.storages_file_links_journals_id_seq OWNED BY public.storages_file_links_journals.id;


--
-- Name: storages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.storages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: storages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.storages_id_seq OWNED BY public.storages.id;


--
-- Name: subproject_template_assignments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.subproject_template_assignments (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    template_id bigint NOT NULL,
    workspace_type character varying NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: subproject_template_assignments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.subproject_template_assignments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: subproject_template_assignments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.subproject_template_assignments_id_seq OWNED BY public.subproject_template_assignments.id;


--
-- Name: time_entries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.time_entries (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    user_id bigint NOT NULL,
    hours double precision,
    comments text,
    activity_id bigint,
    spent_on date NOT NULL,
    tyear integer NOT NULL,
    tmonth integer NOT NULL,
    tweek integer NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    overridden_costs numeric(15,4),
    costs numeric(15,4),
    rate_id bigint,
    logged_by_id bigint NOT NULL,
    ongoing boolean DEFAULT false NOT NULL,
    start_time integer,
    time_zone character varying,
    entity_type character varying,
    entity_id bigint
);


--
-- Name: time_entries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.time_entries_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: time_entries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.time_entries_id_seq OWNED BY public.time_entries.id;


--
-- Name: time_entry_activities_projects; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.time_entry_activities_projects (
    id bigint NOT NULL,
    activity_id bigint NOT NULL,
    project_id bigint NOT NULL,
    active boolean DEFAULT true
);


--
-- Name: time_entry_activities_projects_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.time_entry_activities_projects_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: time_entry_activities_projects_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.time_entry_activities_projects_id_seq OWNED BY public.time_entry_activities_projects.id;


--
-- Name: time_entry_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.time_entry_journals (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    user_id bigint NOT NULL,
    hours double precision,
    comments text,
    activity_id bigint,
    spent_on date NOT NULL,
    tyear integer NOT NULL,
    tmonth integer NOT NULL,
    tweek integer NOT NULL,
    overridden_costs numeric(15,2),
    costs numeric(15,2),
    rate_id bigint,
    logged_by_id bigint NOT NULL,
    start_time integer,
    time_zone character varying,
    entity_type character varying,
    entity_id bigint
);


--
-- Name: time_entry_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.time_entry_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: time_entry_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.time_entry_journals_id_seq OWNED BY public.time_entry_journals.id;


--
-- Name: token_bucket_states; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.token_bucket_states (
    id bigint NOT NULL,
    identifier character varying NOT NULL,
    microtokens bigint NOT NULL,
    refilled_at timestamp(6) with time zone NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: token_bucket_states_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.token_bucket_states_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: token_bucket_states_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.token_bucket_states_id_seq OWNED BY public.token_bucket_states.id;


--
-- Name: tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tokens (
    id bigint NOT NULL,
    user_id bigint,
    type character varying,
    value character varying(128) DEFAULT ''::character varying NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    expires_on timestamp with time zone,
    data json
);


--
-- Name: tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.tokens_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tokens_id_seq OWNED BY public.tokens.id;


--
-- Name: two_factor_authentication_devices; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.two_factor_authentication_devices (
    id bigint NOT NULL,
    type character varying,
    "default" boolean DEFAULT false NOT NULL,
    active boolean DEFAULT false NOT NULL,
    channel character varying NOT NULL,
    phone_number character varying,
    identifier character varying NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    last_used_at integer,
    otp_secret text,
    user_id bigint,
    webauthn_external_id character varying,
    webauthn_public_key character varying,
    webauthn_sign_count bigint DEFAULT 0 NOT NULL
);


--
-- Name: two_factor_authentication_devices_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.two_factor_authentication_devices_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: two_factor_authentication_devices_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.two_factor_authentication_devices_id_seq OWNED BY public.two_factor_authentication_devices.id;


--
-- Name: type_variants; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.type_variants (
    id bigint NOT NULL,
    type_id bigint NOT NULL,
    project_id bigint,
    variant_name character varying,
    is_default_variant boolean DEFAULT false NOT NULL,
    enabled_in_new_projects boolean DEFAULT false NOT NULL,
    default_work_package_description text,
    patterns text,
    pdf_export_templates_config jsonb DEFAULT '{}'::jsonb,
    pdf_export_source_id bigint,
    defaults_source_id bigint,
    form_configuration_excluded_elements text[] DEFAULT '{}'::text[] NOT NULL,
    project_attributes_source_id bigint,
    project_attributes_excluded_elements text[] DEFAULT '{}'::text[] NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    required_attributes text[] DEFAULT '{}'::text[] NOT NULL,
    workflow_id bigint NOT NULL,
    linked_aspects text[] DEFAULT '{}'::text[] NOT NULL,
    form_configuration_id bigint NOT NULL,
    created_by_migration boolean DEFAULT false NOT NULL,
    description text,
    CONSTRAINT type_variants_base_has_no_name CHECK ((is_default_variant = (variant_name IS NULL)))
);


--
-- Name: type_variants_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.type_variants_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: type_variants_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.type_variants_id_seq OWNED BY public.type_variants.id;


--
-- Name: types; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.types (
    id bigint NOT NULL,
    name character varying DEFAULT ''::character varying NOT NULL,
    "position" integer DEFAULT 1,
    is_in_roadmap boolean DEFAULT true NOT NULL,
    is_milestone boolean DEFAULT false NOT NULL,
    color_id bigint,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    allow_project_variants boolean DEFAULT true NOT NULL,
    description text
);


--
-- Name: types_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.types_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: types_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.types_id_seq OWNED BY public.types.id;


--
-- Name: user_auth_provider_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_auth_provider_links (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    auth_provider_id bigint NOT NULL,
    external_id character varying NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: user_auth_provider_links_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_auth_provider_links_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_auth_provider_links_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_auth_provider_links_id_seq OWNED BY public.user_auth_provider_links.id;


--
-- Name: user_non_working_times; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_non_working_times (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    start_date date NOT NULL,
    end_date date NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: user_non_working_times_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_non_working_times_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_non_working_times_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_non_working_times_id_seq OWNED BY public.user_non_working_times.id;


--
-- Name: user_passwords; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_passwords (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    hashed_password character varying(128) NOT NULL,
    salt character varying(64),
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    type character varying NOT NULL
);


--
-- Name: user_passwords_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_passwords_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_passwords_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_passwords_id_seq OWNED BY public.user_passwords.id;


--
-- Name: user_preferences; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_preferences (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    settings jsonb DEFAULT '{}'::jsonb,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    dismissed_enterprise_banners jsonb DEFAULT '{}'::jsonb NOT NULL,
    whats_new_read_items jsonb DEFAULT '{}'::jsonb NOT NULL,
    whats_new_seen_version character varying,
    whats_new_seen_at timestamp(6) with time zone
);


--
-- Name: user_preferences_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_preferences_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_preferences_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_preferences_id_seq OWNED BY public.user_preferences.id;


--
-- Name: user_working_hours; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_working_hours (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    valid_from date NOT NULL,
    monday integer NOT NULL,
    tuesday integer NOT NULL,
    wednesday integer NOT NULL,
    thursday integer NOT NULL,
    friday integer NOT NULL,
    saturday integer NOT NULL,
    sunday integer NOT NULL,
    availability_factor integer DEFAULT 100 NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: user_working_hours_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_working_hours_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_working_hours_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_working_hours_id_seq OWNED BY public.user_working_hours.id;


--
-- Name: users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.users (
    id bigint NOT NULL,
    login character varying(256) DEFAULT ''::character varying NOT NULL,
    firstname character varying DEFAULT ''::character varying NOT NULL,
    lastname character varying DEFAULT ''::character varying NOT NULL,
    mail character varying DEFAULT ''::character varying NOT NULL,
    admin boolean DEFAULT false NOT NULL,
    status integer DEFAULT 1 NOT NULL,
    last_login_on timestamp with time zone,
    language character varying(5) DEFAULT ''::character varying,
    ldap_auth_source_id bigint,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    type character varying,
    identity_url character varying,
    first_login boolean DEFAULT true NOT NULL,
    force_password_change boolean DEFAULT false,
    failed_login_count integer DEFAULT 0,
    last_failed_login_on timestamp with time zone,
    consented_at timestamp with time zone,
    webauthn_id character varying
);


--
-- Name: users_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.users_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.users_id_seq OWNED BY public.users.id;


--
-- Name: versions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.versions (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    name character varying DEFAULT ''::character varying NOT NULL COLLATE public.natural_sort_order,
    description character varying DEFAULT ''::character varying,
    effective_date date,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP,
    wiki_page_title character varying,
    status character varying DEFAULT 'open'::character varying,
    sharing character varying DEFAULT 'none'::character varying NOT NULL,
    start_date date
);


--
-- Name: versions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.versions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: versions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.versions_id_seq OWNED BY public.versions.id;


--
-- Name: views; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.views (
    id bigint NOT NULL,
    query_id bigint NOT NULL,
    options jsonb DEFAULT '{}'::jsonb NOT NULL,
    type character varying NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: views_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.views_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: views_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.views_id_seq OWNED BY public.views.id;


--
-- Name: watchers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.watchers (
    id bigint NOT NULL,
    watchable_type character varying DEFAULT ''::character varying NOT NULL,
    watchable_id bigint NOT NULL,
    user_id bigint
);


--
-- Name: watchers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.watchers_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: watchers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.watchers_id_seq OWNED BY public.watchers.id;


--
-- Name: webhooks_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.webhooks_events (
    id bigint NOT NULL,
    name character varying,
    webhooks_webhook_id bigint
);


--
-- Name: webhooks_events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.webhooks_events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: webhooks_events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.webhooks_events_id_seq OWNED BY public.webhooks_events.id;


--
-- Name: webhooks_logs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.webhooks_logs (
    id bigint NOT NULL,
    webhooks_webhook_id bigint,
    event_name character varying,
    url character varying,
    request_headers text,
    request_body text,
    response_code integer,
    response_headers text,
    response_body text,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: webhooks_logs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.webhooks_logs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: webhooks_logs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.webhooks_logs_id_seq OWNED BY public.webhooks_logs.id;


--
-- Name: webhooks_projects; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.webhooks_projects (
    id bigint NOT NULL,
    project_id bigint,
    webhooks_webhook_id bigint
);


--
-- Name: webhooks_projects_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.webhooks_projects_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: webhooks_projects_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.webhooks_projects_id_seq OWNED BY public.webhooks_projects.id;


--
-- Name: webhooks_webhooks; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.webhooks_webhooks (
    id bigint NOT NULL,
    name character varying,
    url text,
    description text NOT NULL,
    secret character varying,
    enabled boolean NOT NULL,
    all_projects boolean NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL
);


--
-- Name: webhooks_webhooks_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.webhooks_webhooks_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: webhooks_webhooks_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.webhooks_webhooks_id_seq OWNED BY public.webhooks_webhooks.id;


--
-- Name: wiki_page_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.wiki_page_journals (
    id bigint NOT NULL,
    author_id bigint,
    text text
);


--
-- Name: wiki_page_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.wiki_page_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: wiki_page_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.wiki_page_journals_id_seq OWNED BY public.wiki_page_journals.id;


--
-- Name: wiki_page_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.wiki_page_links (
    id bigint NOT NULL,
    linkable_type character varying NOT NULL,
    linkable_id bigint NOT NULL,
    provider_id bigint NOT NULL,
    author_id bigint,
    type character varying NOT NULL,
    identifier character varying NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: wiki_page_links_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.wiki_page_links_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: wiki_page_links_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.wiki_page_links_id_seq OWNED BY public.wiki_page_links.id;


--
-- Name: wiki_pages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.wiki_pages (
    id bigint NOT NULL,
    wiki_id bigint NOT NULL,
    title character varying NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    protected boolean DEFAULT false NOT NULL,
    parent_id bigint,
    slug character varying NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    author_id bigint NOT NULL,
    text text,
    lock_version integer NOT NULL
);


--
-- Name: wiki_pages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.wiki_pages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: wiki_pages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.wiki_pages_id_seq OWNED BY public.wiki_pages.id;


--
-- Name: wiki_providers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.wiki_providers (
    id bigint NOT NULL,
    name character varying NOT NULL,
    universal_identifier character varying NOT NULL,
    type character varying NOT NULL,
    options jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL,
    enabled boolean DEFAULT true NOT NULL
);


--
-- Name: wiki_providers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.wiki_providers_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: wiki_providers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.wiki_providers_id_seq OWNED BY public.wiki_providers.id;


--
-- Name: wiki_redirects; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.wiki_redirects (
    id bigint NOT NULL,
    wiki_id bigint NOT NULL,
    title character varying,
    redirects_to character varying,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: wiki_redirects_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.wiki_redirects_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: wiki_redirects_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.wiki_redirects_id_seq OWNED BY public.wiki_redirects.id;


--
-- Name: wikis; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.wikis (
    id bigint NOT NULL,
    project_id bigint NOT NULL,
    start_page character varying NOT NULL,
    status integer DEFAULT 1 NOT NULL,
    created_at timestamp with time zone NOT NULL,
    updated_at timestamp with time zone NOT NULL,
    enabled boolean DEFAULT true NOT NULL
);


--
-- Name: wikis_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.wikis_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: wikis_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.wikis_id_seq OWNED BY public.wikis.id;


--
-- Name: work_package_categories; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.work_package_categories (
    id bigint NOT NULL,
    work_package_id bigint NOT NULL,
    category_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: work_package_categories_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.work_package_categories_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: work_package_categories_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.work_package_categories_id_seq OWNED BY public.work_package_categories.id;


--
-- Name: work_package_hierarchies; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.work_package_hierarchies (
    ancestor_id integer NOT NULL,
    descendant_id integer NOT NULL,
    generations integer NOT NULL
);


--
-- Name: work_package_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.work_package_journals (
    id bigint NOT NULL,
    type_id bigint NOT NULL,
    project_id bigint NOT NULL,
    subject character varying NOT NULL,
    description text,
    due_date date,
    category_id bigint,
    status_id bigint NOT NULL,
    assigned_to_id bigint,
    priority_id bigint NOT NULL,
    version_id bigint,
    author_id bigint NOT NULL,
    done_ratio integer,
    estimated_hours double precision,
    start_date date,
    parent_id bigint,
    responsible_id bigint,
    derived_estimated_hours double precision,
    schedule_manually boolean,
    duration integer,
    ignore_non_working_days boolean NOT NULL,
    derived_remaining_hours double precision,
    derived_done_ratio integer,
    story_points integer,
    remaining_hours double precision,
    budget_id integer,
    project_phase_definition_id bigint,
    sprint_id bigint,
    backlog_bucket_id bigint
);


--
-- Name: work_package_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.work_package_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: work_package_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.work_package_journals_id_seq OWNED BY public.work_package_journals.id;


--
-- Name: work_package_semantic_aliases; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.work_package_semantic_aliases (
    id bigint NOT NULL,
    identifier character varying NOT NULL,
    work_package_id bigint NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: work_package_semantic_aliases_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.work_package_semantic_aliases_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: work_package_semantic_aliases_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.work_package_semantic_aliases_id_seq OWNED BY public.work_package_semantic_aliases.id;


--
-- Name: work_package_version_journals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.work_package_version_journals (
    id bigint NOT NULL,
    journal_id bigint NOT NULL,
    version_id bigint NOT NULL,
    kind public.work_package_version_kind NOT NULL
);


--
-- Name: work_package_version_journals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.work_package_version_journals_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: work_package_version_journals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.work_package_version_journals_id_seq OWNED BY public.work_package_version_journals.id;


--
-- Name: work_package_versions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.work_package_versions (
    id bigint NOT NULL,
    work_package_id bigint NOT NULL,
    version_id bigint NOT NULL,
    kind public.work_package_version_kind NOT NULL,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: work_package_versions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.work_package_versions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: work_package_versions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.work_package_versions_id_seq OWNED BY public.work_package_versions.id;


--
-- Name: work_packages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.work_packages (
    id bigint NOT NULL,
    type_id bigint NOT NULL,
    project_id bigint NOT NULL,
    subject character varying DEFAULT ''::character varying NOT NULL,
    description text,
    due_date date,
    category_id bigint,
    status_id bigint NOT NULL,
    assigned_to_id bigint,
    priority_id bigint,
    version_id bigint,
    author_id bigint NOT NULL,
    lock_version integer DEFAULT 0 NOT NULL,
    done_ratio integer,
    estimated_hours double precision,
    created_at timestamp with time zone,
    updated_at timestamp with time zone,
    start_date date,
    responsible_id bigint,
    derived_estimated_hours double precision,
    schedule_manually boolean DEFAULT true NOT NULL,
    parent_id bigint,
    duration integer,
    ignore_non_working_days boolean DEFAULT false NOT NULL,
    derived_remaining_hours double precision,
    derived_done_ratio integer,
    "position" integer,
    story_points integer,
    remaining_hours double precision,
    budget_id integer,
    project_phase_definition_id bigint,
    sprint_id bigint,
    sequence_number integer,
    identifier character varying,
    backlog_bucket_id bigint,
    origin_source character varying,
    origin_id character varying,
    origin_url character varying,
    origin_host character varying,
    CONSTRAINT work_package_backlog_bucket_xor_sprint CHECK (((backlog_bucket_id IS NULL) OR (sprint_id IS NULL))),
    CONSTRAINT work_packages_due_larger_start_date CHECK ((due_date >= start_date))
);


--
-- Name: work_packages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.work_packages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: work_packages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.work_packages_id_seq OWNED BY public.work_packages.id;


--
-- Name: workflows; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.workflows (
    id bigint NOT NULL,
    name character varying NOT NULL,
    description text,
    created_at timestamp(6) with time zone NOT NULL,
    updated_at timestamp(6) with time zone NOT NULL
);


--
-- Name: workflows_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.workflows_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: workflows_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.workflows_id_seq OWNED BY public.workflows.id;


--
-- Name: workflows_status_transitions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.workflows_status_transitions (
    id bigint NOT NULL,
    old_status_id bigint NOT NULL,
    new_status_id bigint NOT NULL,
    role_id bigint NOT NULL,
    assignee boolean DEFAULT false NOT NULL,
    author boolean DEFAULT false NOT NULL,
    workflow_id bigint NOT NULL
);


--
-- Name: workflows_status_transitions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.workflows_status_transitions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: workflows_status_transitions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.workflows_status_transitions_id_seq OWNED BY public.workflows_status_transitions.id;


--
-- Name: ai_text_action_types id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_action_types ALTER COLUMN id SET DEFAULT nextval('public.ai_text_action_types_id_seq'::regclass);


--
-- Name: ai_text_actions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_actions ALTER COLUMN id SET DEFAULT nextval('public.ai_text_actions_id_seq'::regclass);


--
-- Name: ai_text_transform_run_events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_transform_run_events ALTER COLUMN id SET DEFAULT nextval('public.ai_text_transform_run_events_id_seq'::regclass);


--
-- Name: ai_text_transform_runs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_transform_runs ALTER COLUMN id SET DEFAULT nextval('public.ai_text_transform_runs_id_seq'::regclass);


--
-- Name: announcements id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.announcements ALTER COLUMN id SET DEFAULT nextval('public.announcements_id_seq'::regclass);


--
-- Name: attachable_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attachable_journals ALTER COLUMN id SET DEFAULT nextval('public.attachable_journals_id_seq'::regclass);


--
-- Name: attachment_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attachment_journals ALTER COLUMN id SET DEFAULT nextval('public.attachment_journals_id_seq'::regclass);


--
-- Name: attachments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attachments ALTER COLUMN id SET DEFAULT nextval('public.attachments_id_seq'::regclass);


--
-- Name: attribute_help_texts id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attribute_help_texts ALTER COLUMN id SET DEFAULT nextval('public.attribute_help_texts_id_seq'::regclass);


--
-- Name: auth_providers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_providers ALTER COLUMN id SET DEFAULT nextval('public.auth_providers_id_seq'::regclass);


--
-- Name: autologin_session_links id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.autologin_session_links ALTER COLUMN id SET DEFAULT nextval('public.autologin_session_links_id_seq'::regclass);


--
-- Name: automation_actions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automation_actions ALTER COLUMN id SET DEFAULT nextval('public.automation_actions_id_seq'::regclass);


--
-- Name: automation_triggers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automation_triggers ALTER COLUMN id SET DEFAULT nextval('public.automation_triggers_id_seq'::regclass);


--
-- Name: automations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automations ALTER COLUMN id SET DEFAULT nextval('public.automations_id_seq'::regclass);


--
-- Name: automations_projects id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automations_projects ALTER COLUMN id SET DEFAULT nextval('public.automations_projects_id_seq'::regclass);


--
-- Name: automations_roles id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automations_roles ALTER COLUMN id SET DEFAULT nextval('public.automations_roles_id_seq'::regclass);


--
-- Name: automations_statuses id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automations_statuses ALTER COLUMN id SET DEFAULT nextval('public.automations_statuses_id_seq'::regclass);


--
-- Name: automations_types id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automations_types ALTER COLUMN id SET DEFAULT nextval('public.automations_types_id_seq'::regclass);


--
-- Name: backlog_buckets id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.backlog_buckets ALTER COLUMN id SET DEFAULT nextval('public.backlog_buckets_id_seq'::regclass);


--
-- Name: bcf_comments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bcf_comments ALTER COLUMN id SET DEFAULT nextval('public.bcf_comments_id_seq'::regclass);


--
-- Name: bcf_issues id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bcf_issues ALTER COLUMN id SET DEFAULT nextval('public.bcf_issues_id_seq'::regclass);


--
-- Name: bcf_viewpoints id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bcf_viewpoints ALTER COLUMN id SET DEFAULT nextval('public.bcf_viewpoints_id_seq'::regclass);


--
-- Name: budget_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.budget_journals ALTER COLUMN id SET DEFAULT nextval('public.budget_journals_id_seq'::regclass);


--
-- Name: budgets id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.budgets ALTER COLUMN id SET DEFAULT nextval('public.budgets_id_seq'::regclass);


--
-- Name: calculated_value_errors id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calculated_value_errors ALTER COLUMN id SET DEFAULT nextval('public.calculated_value_errors_id_seq'::regclass);


--
-- Name: categories id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.categories ALTER COLUMN id SET DEFAULT nextval('public.categories_id_seq'::regclass);


--
-- Name: changes id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.changes ALTER COLUMN id SET DEFAULT nextval('public.changes_id_seq'::regclass);


--
-- Name: changeset_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.changeset_journals ALTER COLUMN id SET DEFAULT nextval('public.changeset_journals_id_seq'::regclass);


--
-- Name: changesets id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.changesets ALTER COLUMN id SET DEFAULT nextval('public.changesets_id_seq'::regclass);


--
-- Name: colors id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.colors ALTER COLUMN id SET DEFAULT nextval('public.colors_id_seq'::regclass);


--
-- Name: comments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.comments ALTER COLUMN id SET DEFAULT nextval('public.comments_id_seq'::regclass);


--
-- Name: cost_entries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cost_entries ALTER COLUMN id SET DEFAULT nextval('public.cost_entries_id_seq'::regclass);


--
-- Name: cost_queries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cost_queries ALTER COLUMN id SET DEFAULT nextval('public.cost_queries_id_seq'::regclass);


--
-- Name: cost_types id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cost_types ALTER COLUMN id SET DEFAULT nextval('public.cost_types_id_seq'::regclass);


--
-- Name: cost_types_projects id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cost_types_projects ALTER COLUMN id SET DEFAULT nextval('public.cost_types_projects_id_seq'::regclass);


--
-- Name: custom_comment_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_comment_journals ALTER COLUMN id SET DEFAULT nextval('public.custom_comment_journals_id_seq'::regclass);


--
-- Name: custom_comments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_comments ALTER COLUMN id SET DEFAULT nextval('public.custom_comments_id_seq'::regclass);


--
-- Name: custom_field_project_specific_value_restrictions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_field_project_specific_value_restrictions ALTER COLUMN id SET DEFAULT nextval('public.custom_field_project_specific_value_restrictions_id_seq'::regclass);


--
-- Name: custom_field_sections id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_field_sections ALTER COLUMN id SET DEFAULT nextval('public.custom_field_sections_id_seq'::regclass);


--
-- Name: custom_fields id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_fields ALTER COLUMN id SET DEFAULT nextval('public.custom_fields_id_seq'::regclass);


--
-- Name: custom_fields_projects id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_fields_projects ALTER COLUMN id SET DEFAULT nextval('public.custom_fields_projects_id_seq'::regclass);


--
-- Name: custom_fields_roles id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_fields_roles ALTER COLUMN id SET DEFAULT nextval('public.custom_fields_roles_id_seq'::regclass);


--
-- Name: custom_options id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_options ALTER COLUMN id SET DEFAULT nextval('public.custom_options_id_seq'::regclass);


--
-- Name: custom_styles id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_styles ALTER COLUMN id SET DEFAULT nextval('public.custom_styles_id_seq'::regclass);


--
-- Name: custom_values id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_values ALTER COLUMN id SET DEFAULT nextval('public.custom_values_id_seq'::regclass);


--
-- Name: customizable_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.customizable_journals ALTER COLUMN id SET DEFAULT nextval('public.customizable_journals_id_seq'::regclass);


--
-- Name: deploy_status_checks id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.deploy_status_checks ALTER COLUMN id SET DEFAULT nextval('public.deploy_status_checks_id_seq'::regclass);


--
-- Name: deploy_targets id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.deploy_targets ALTER COLUMN id SET DEFAULT nextval('public.deploy_targets_id_seq'::regclass);


--
-- Name: design_colors id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.design_colors ALTER COLUMN id SET DEFAULT nextval('public.design_colors_id_seq'::regclass);


--
-- Name: document_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_journals ALTER COLUMN id SET DEFAULT nextval('public.document_journals_id_seq'::regclass);


--
-- Name: document_types id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_types ALTER COLUMN id SET DEFAULT nextval('public.document_types_id_seq'::regclass);


--
-- Name: documents id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.documents ALTER COLUMN id SET DEFAULT nextval('public.documents_id_seq'::regclass);


--
-- Name: emoji_reactions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.emoji_reactions ALTER COLUMN id SET DEFAULT nextval('public.emoji_reactions_id_seq'::regclass);


--
-- Name: enabled_modules id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.enabled_modules ALTER COLUMN id SET DEFAULT nextval('public.enabled_modules_id_seq'::regclass);


--
-- Name: enterprise_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.enterprise_tokens ALTER COLUMN id SET DEFAULT nextval('public.enterprise_tokens_id_seq'::regclass);


--
-- Name: enumerations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.enumerations ALTER COLUMN id SET DEFAULT nextval('public.enumerations_id_seq'::regclass);


--
-- Name: export_settings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.export_settings ALTER COLUMN id SET DEFAULT nextval('public.export_settings_id_seq'::regclass);


--
-- Name: exports id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.exports ALTER COLUMN id SET DEFAULT nextval('public.exports_id_seq'::regclass);


--
-- Name: favorites id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.favorites ALTER COLUMN id SET DEFAULT nextval('public.favorites_id_seq'::regclass);


--
-- Name: file_links id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_links ALTER COLUMN id SET DEFAULT nextval('public.file_links_id_seq'::regclass);


--
-- Name: form_configuration_attributes id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configuration_attributes ALTER COLUMN id SET DEFAULT nextval('public.form_configuration_attributes_id_seq'::regclass);


--
-- Name: form_configuration_groups id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configuration_groups ALTER COLUMN id SET DEFAULT nextval('public.form_configuration_groups_id_seq'::regclass);


--
-- Name: form_configurations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configurations ALTER COLUMN id SET DEFAULT nextval('public.form_configurations_id_seq'::regclass);


--
-- Name: forums id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.forums ALTER COLUMN id SET DEFAULT nextval('public.forums_id_seq'::regclass);


--
-- Name: friendly_id_slugs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.friendly_id_slugs ALTER COLUMN id SET DEFAULT nextval('public.friendly_id_slugs_id_seq'::regclass);


--
-- Name: github_check_runs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.github_check_runs ALTER COLUMN id SET DEFAULT nextval('public.github_check_runs_id_seq'::regclass);


--
-- Name: github_pull_requests id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.github_pull_requests ALTER COLUMN id SET DEFAULT nextval('public.github_pull_requests_id_seq'::regclass);


--
-- Name: github_users id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.github_users ALTER COLUMN id SET DEFAULT nextval('public.github_users_id_seq'::regclass);


--
-- Name: gitlab_branches id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_branches ALTER COLUMN id SET DEFAULT nextval('public.gitlab_branches_id_seq'::regclass);


--
-- Name: gitlab_commits id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_commits ALTER COLUMN id SET DEFAULT nextval('public.gitlab_commits_id_seq'::regclass);


--
-- Name: gitlab_commits_work_packages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_commits_work_packages ALTER COLUMN id SET DEFAULT nextval('public.gitlab_commits_work_packages_id_seq'::regclass);


--
-- Name: gitlab_issues id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_issues ALTER COLUMN id SET DEFAULT nextval('public.gitlab_issues_id_seq'::regclass);


--
-- Name: gitlab_merge_requests id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_merge_requests ALTER COLUMN id SET DEFAULT nextval('public.gitlab_merge_requests_id_seq'::regclass);


--
-- Name: gitlab_pipelines id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_pipelines ALTER COLUMN id SET DEFAULT nextval('public.gitlab_pipelines_id_seq'::regclass);


--
-- Name: gitlab_users id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_users ALTER COLUMN id SET DEFAULT nextval('public.gitlab_users_id_seq'::regclass);


--
-- Name: grid_widgets id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.grid_widgets ALTER COLUMN id SET DEFAULT nextval('public.grid_widgets_id_seq'::regclass);


--
-- Name: grids id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.grids ALTER COLUMN id SET DEFAULT nextval('public.grids_id_seq'::regclass);


--
-- Name: group_details id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_details ALTER COLUMN id SET DEFAULT nextval('public.group_details_id_seq'::regclass);


--
-- Name: group_users id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_users ALTER COLUMN id SET DEFAULT nextval('public.group_users_id_seq'::regclass);


--
-- Name: health_reports id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.health_reports ALTER COLUMN id SET DEFAULT nextval('public.health_reports_id_seq'::regclass);


--
-- Name: hierarchical_items id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.hierarchical_items ALTER COLUMN id SET DEFAULT nextval('public.hierarchical_items_id_seq'::regclass);


--
-- Name: ical_token_query_assignments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ical_token_query_assignments ALTER COLUMN id SET DEFAULT nextval('public.ical_token_query_assignments_id_seq'::regclass);


--
-- Name: ifc_models id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ifc_models ALTER COLUMN id SET DEFAULT nextval('public.ifc_models_id_seq'::regclass);


--
-- Name: itsm_email_messages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_email_messages ALTER COLUMN id SET DEFAULT nextval('public.itsm_email_messages_id_seq'::regclass);


--
-- Name: itsm_email_threads id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_email_threads ALTER COLUMN id SET DEFAULT nextval('public.itsm_email_threads_id_seq'::regclass);


--
-- Name: itsm_incoming_mailboxes id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_incoming_mailboxes ALTER COLUMN id SET DEFAULT nextval('public.itsm_incoming_mailboxes_id_seq'::regclass);


--
-- Name: itsm_mail_servers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_mail_servers ALTER COLUMN id SET DEFAULT nextval('public.itsm_mail_servers_id_seq'::regclass);


--
-- Name: itsm_outgoing_mailboxes id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_outgoing_mailboxes ALTER COLUMN id SET DEFAULT nextval('public.itsm_outgoing_mailboxes_id_seq'::regclass);


--
-- Name: jira_attachments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_attachments ALTER COLUMN id SET DEFAULT nextval('public.jira_attachments_id_seq'::regclass);


--
-- Name: jira_boards id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_boards ALTER COLUMN id SET DEFAULT nextval('public.jira_boards_id_seq'::regclass);


--
-- Name: jira_fields id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_fields ALTER COLUMN id SET DEFAULT nextval('public.jira_fields_id_seq'::regclass);


--
-- Name: jira_import_job_cursors id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_import_job_cursors ALTER COLUMN id SET DEFAULT nextval('public.jira_import_job_cursors_id_seq'::regclass);


--
-- Name: jira_import_transitions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_import_transitions ALTER COLUMN id SET DEFAULT nextval('public.jira_import_transitions_id_seq'::regclass);


--
-- Name: jira_imports id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_imports ALTER COLUMN id SET DEFAULT nextval('public.jira_imports_id_seq'::regclass);


--
-- Name: jira_issue_types id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_issue_types ALTER COLUMN id SET DEFAULT nextval('public.jira_issue_types_id_seq'::regclass);


--
-- Name: jira_issues id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_issues ALTER COLUMN id SET DEFAULT nextval('public.jira_issues_id_seq'::regclass);


--
-- Name: jira_open_project_references id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_open_project_references ALTER COLUMN id SET DEFAULT nextval('public.jira_open_project_references_id_seq'::regclass);


--
-- Name: jira_priorities id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_priorities ALTER COLUMN id SET DEFAULT nextval('public.jira_priorities_id_seq'::regclass);


--
-- Name: jira_projects id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_projects ALTER COLUMN id SET DEFAULT nextval('public.jira_projects_id_seq'::regclass);


--
-- Name: jira_sprints id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_sprints ALTER COLUMN id SET DEFAULT nextval('public.jira_sprints_id_seq'::regclass);


--
-- Name: jira_statuses id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_statuses ALTER COLUMN id SET DEFAULT nextval('public.jira_statuses_id_seq'::regclass);


--
-- Name: jira_users id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_users ALTER COLUMN id SET DEFAULT nextval('public.jira_users_id_seq'::regclass);


--
-- Name: jira_versions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_versions ALTER COLUMN id SET DEFAULT nextval('public.jira_versions_id_seq'::regclass);


--
-- Name: jiras id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jiras ALTER COLUMN id SET DEFAULT nextval('public.jiras_id_seq'::regclass);


--
-- Name: job_statuses id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_statuses ALTER COLUMN id SET DEFAULT nextval('public.job_statuses_id_seq'::regclass);


--
-- Name: journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.journals ALTER COLUMN id SET DEFAULT nextval('public.journals_id_seq'::regclass);


--
-- Name: label_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.label_journals ALTER COLUMN id SET DEFAULT nextval('public.label_journals_id_seq'::regclass);


--
-- Name: labelings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.labelings ALTER COLUMN id SET DEFAULT nextval('public.labelings_id_seq'::regclass);


--
-- Name: labels id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.labels ALTER COLUMN id SET DEFAULT nextval('public.labels_id_seq'::regclass);


--
-- Name: labor_budget_items id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.labor_budget_items ALTER COLUMN id SET DEFAULT nextval('public.labor_budget_items_id_seq'::regclass);


--
-- Name: last_project_folders id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.last_project_folders ALTER COLUMN id SET DEFAULT nextval('public.last_project_folders_id_seq'::regclass);


--
-- Name: ldap_auth_sources id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_auth_sources ALTER COLUMN id SET DEFAULT nextval('public.ldap_auth_sources_id_seq'::regclass);


--
-- Name: ldap_departments_memberships id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_departments_memberships ALTER COLUMN id SET DEFAULT nextval('public.ldap_departments_memberships_id_seq'::regclass);


--
-- Name: ldap_departments_synchronized_departments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_departments_synchronized_departments ALTER COLUMN id SET DEFAULT nextval('public.ldap_departments_synchronized_departments_id_seq'::regclass);


--
-- Name: ldap_departments_synchronized_trees id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_departments_synchronized_trees ALTER COLUMN id SET DEFAULT nextval('public.ldap_departments_synchronized_trees_id_seq'::regclass);


--
-- Name: ldap_groups_memberships id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_groups_memberships ALTER COLUMN id SET DEFAULT nextval('public.ldap_groups_memberships_id_seq'::regclass);


--
-- Name: ldap_groups_synchronized_filters id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_groups_synchronized_filters ALTER COLUMN id SET DEFAULT nextval('public.ldap_groups_synchronized_filters_id_seq'::regclass);


--
-- Name: ldap_groups_synchronized_groups id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_groups_synchronized_groups ALTER COLUMN id SET DEFAULT nextval('public.ldap_groups_synchronized_groups_id_seq'::regclass);


--
-- Name: llm_capability_verdicts id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_capability_verdicts ALTER COLUMN id SET DEFAULT nextval('public.llm_capability_verdicts_id_seq'::regclass);


--
-- Name: llm_connections id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_connections ALTER COLUMN id SET DEFAULT nextval('public.llm_connections_id_seq'::regclass);


--
-- Name: llm_feature_bindings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_feature_bindings ALTER COLUMN id SET DEFAULT nextval('public.llm_feature_bindings_id_seq'::regclass);


--
-- Name: llm_models id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_models ALTER COLUMN id SET DEFAULT nextval('public.llm_models_id_seq'::regclass);


--
-- Name: material_budget_items id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.material_budget_items ALTER COLUMN id SET DEFAULT nextval('public.material_budget_items_id_seq'::regclass);


--
-- Name: mcp_configurations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mcp_configurations ALTER COLUMN id SET DEFAULT nextval('public.mcp_configurations_id_seq'::regclass);


--
-- Name: meeting_agenda_item_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_agenda_item_journals ALTER COLUMN id SET DEFAULT nextval('public.meeting_agenda_item_journals_id_seq'::regclass);


--
-- Name: meeting_agenda_items id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_agenda_items ALTER COLUMN id SET DEFAULT nextval('public.meeting_agenda_items_id_seq'::regclass);


--
-- Name: meeting_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_journals ALTER COLUMN id SET DEFAULT nextval('public.meeting_journals_id_seq'::regclass);


--
-- Name: meeting_outcome_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_outcome_journals ALTER COLUMN id SET DEFAULT nextval('public.meeting_outcome_journals_id_seq'::regclass);


--
-- Name: meeting_outcomes id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_outcomes ALTER COLUMN id SET DEFAULT nextval('public.meeting_outcomes_id_seq'::regclass);


--
-- Name: meeting_participant_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_participant_journals ALTER COLUMN id SET DEFAULT nextval('public.meeting_participant_journals_id_seq'::regclass);


--
-- Name: meeting_participants id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_participants ALTER COLUMN id SET DEFAULT nextval('public.meeting_participants_id_seq'::regclass);


--
-- Name: meeting_section_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_section_journals ALTER COLUMN id SET DEFAULT nextval('public.meeting_section_journals_id_seq'::regclass);


--
-- Name: meeting_sections id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_sections ALTER COLUMN id SET DEFAULT nextval('public.meeting_sections_id_seq'::regclass);


--
-- Name: meetings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meetings ALTER COLUMN id SET DEFAULT nextval('public.meetings_id_seq'::regclass);


--
-- Name: member_roles id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.member_roles ALTER COLUMN id SET DEFAULT nextval('public.member_roles_id_seq'::regclass);


--
-- Name: members id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.members ALTER COLUMN id SET DEFAULT nextval('public.members_id_seq'::regclass);


--
-- Name: menu_items id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.menu_items ALTER COLUMN id SET DEFAULT nextval('public.menu_items_id_seq'::regclass);


--
-- Name: message_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_journals ALTER COLUMN id SET DEFAULT nextval('public.message_journals_id_seq'::regclass);


--
-- Name: message_work_packages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_work_packages ALTER COLUMN id SET DEFAULT nextval('public.message_work_packages_id_seq'::regclass);


--
-- Name: messages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages ALTER COLUMN id SET DEFAULT nextval('public.messages_id_seq'::regclass);


--
-- Name: news id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news ALTER COLUMN id SET DEFAULT nextval('public.news_id_seq'::regclass);


--
-- Name: news_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_journals ALTER COLUMN id SET DEFAULT nextval('public.news_journals_id_seq'::regclass);


--
-- Name: non_working_days id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.non_working_days ALTER COLUMN id SET DEFAULT nextval('public.non_working_days_id_seq'::regclass);


--
-- Name: notification_settings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notification_settings ALTER COLUMN id SET DEFAULT nextval('public.notification_settings_id_seq'::regclass);


--
-- Name: notifications id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications ALTER COLUMN id SET DEFAULT nextval('public.notifications_id_seq'::regclass);


--
-- Name: oauth_access_grants id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_access_grants ALTER COLUMN id SET DEFAULT nextval('public.oauth_access_grants_id_seq'::regclass);


--
-- Name: oauth_access_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_access_tokens ALTER COLUMN id SET DEFAULT nextval('public.oauth_access_tokens_id_seq'::regclass);


--
-- Name: oauth_applications id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_applications ALTER COLUMN id SET DEFAULT nextval('public.oauth_applications_id_seq'::regclass);


--
-- Name: oauth_client_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_client_tokens ALTER COLUMN id SET DEFAULT nextval('public.oauth_client_tokens_id_seq'::regclass);


--
-- Name: oauth_clients id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_clients ALTER COLUMN id SET DEFAULT nextval('public.oauth_clients_id_seq'::regclass);


--
-- Name: oidc_group_links id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_group_links ALTER COLUMN id SET DEFAULT nextval('public.oidc_group_links_id_seq'::regclass);


--
-- Name: oidc_group_memberships id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_group_memberships ALTER COLUMN id SET DEFAULT nextval('public.oidc_group_memberships_id_seq'::regclass);


--
-- Name: oidc_user_session_links id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_user_session_links ALTER COLUMN id SET DEFAULT nextval('public.oidc_user_session_links_id_seq'::regclass);


--
-- Name: oidc_user_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_user_tokens ALTER COLUMN id SET DEFAULT nextval('public.oidc_user_tokens_id_seq'::regclass);


--
-- Name: ordered_persisted_query_entities id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ordered_persisted_query_entities ALTER COLUMN id SET DEFAULT nextval('public.ordered_persisted_query_entities_id_seq'::regclass);


--
-- Name: ordered_work_packages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ordered_work_packages ALTER COLUMN id SET DEFAULT nextval('public.ordered_work_packages_id_seq'::regclass);


--
-- Name: outbound_mail_recipients id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.outbound_mail_recipients ALTER COLUMN id SET DEFAULT nextval('public.outbound_mail_recipients_id_seq'::regclass);


--
-- Name: paper_trail_audits id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.paper_trail_audits ALTER COLUMN id SET DEFAULT nextval('public.paper_trail_audits_id_seq'::regclass);


--
-- Name: persisted_queries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.persisted_queries ALTER COLUMN id SET DEFAULT nextval('public.persisted_queries_id_seq'::regclass);


--
-- Name: persisted_views id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.persisted_views ALTER COLUMN id SET DEFAULT nextval('public.persisted_views_id_seq'::regclass);


--
-- Name: placeholder_user_details id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.placeholder_user_details ALTER COLUMN id SET DEFAULT nextval('public.placeholder_user_details_id_seq'::regclass);


--
-- Name: project_custom_field_project_mappings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_custom_field_project_mappings ALTER COLUMN id SET DEFAULT nextval('public.project_custom_field_project_mappings_id_seq'::regclass);


--
-- Name: project_custom_field_type_mappings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_custom_field_type_mappings ALTER COLUMN id SET DEFAULT nextval('public.project_custom_field_type_mappings_id_seq'::regclass);


--
-- Name: project_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_journals ALTER COLUMN id SET DEFAULT nextval('public.project_journals_id_seq'::regclass);


--
-- Name: project_phase_definitions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_phase_definitions ALTER COLUMN id SET DEFAULT nextval('public.project_phase_definitions_id_seq'::regclass);


--
-- Name: project_phase_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_phase_journals ALTER COLUMN id SET DEFAULT nextval('public.project_phase_journals_id_seq'::regclass);


--
-- Name: project_phases id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_phases ALTER COLUMN id SET DEFAULT nextval('public.project_phases_id_seq'::regclass);


--
-- Name: project_queries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_queries ALTER COLUMN id SET DEFAULT nextval('public.project_queries_id_seq'::regclass);


--
-- Name: project_storages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_storages ALTER COLUMN id SET DEFAULT nextval('public.project_storages_id_seq'::regclass);


--
-- Name: project_types id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_types ALTER COLUMN id SET DEFAULT nextval('public.project_types_id_seq'::regclass);


--
-- Name: projects id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.projects ALTER COLUMN id SET DEFAULT nextval('public.projects_id_seq'::regclass);


--
-- Name: push_subscriptions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.push_subscriptions ALTER COLUMN id SET DEFAULT nextval('public.push_subscriptions_id_seq'::regclass);


--
-- Name: queries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.queries ALTER COLUMN id SET DEFAULT nextval('public.queries_id_seq'::regclass);


--
-- Name: rates id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.rates ALTER COLUMN id SET DEFAULT nextval('public.rates_id_seq'::regclass);


--
-- Name: recaptcha_entries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recaptcha_entries ALTER COLUMN id SET DEFAULT nextval('public.recaptcha_entries_id_seq'::regclass);


--
-- Name: recurring_meeting_historic_schedules id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_meeting_historic_schedules ALTER COLUMN id SET DEFAULT nextval('public.recurring_meeting_historic_schedules_id_seq'::regclass);


--
-- Name: recurring_meeting_interim_responses id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_meeting_interim_responses ALTER COLUMN id SET DEFAULT nextval('public.recurring_meeting_interim_responses_id_seq'::regclass);


--
-- Name: recurring_meetings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_meetings ALTER COLUMN id SET DEFAULT nextval('public.recurring_meetings_id_seq'::regclass);


--
-- Name: relations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.relations ALTER COLUMN id SET DEFAULT nextval('public.relations_id_seq'::regclass);


--
-- Name: reminder_notifications id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reminder_notifications ALTER COLUMN id SET DEFAULT nextval('public.reminder_notifications_id_seq'::regclass);


--
-- Name: reminders id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reminders ALTER COLUMN id SET DEFAULT nextval('public.reminders_id_seq'::regclass);


--
-- Name: remote_identities id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.remote_identities ALTER COLUMN id SET DEFAULT nextval('public.remote_identities_id_seq'::regclass);


--
-- Name: repositories id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repositories ALTER COLUMN id SET DEFAULT nextval('public.repositories_id_seq'::regclass);


--
-- Name: repository_branches id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_branches ALTER COLUMN id SET DEFAULT nextval('public.repository_branches_id_seq'::regclass);


--
-- Name: repository_commit_work_packages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_commit_work_packages ALTER COLUMN id SET DEFAULT nextval('public.repository_commit_work_packages_id_seq'::regclass);


--
-- Name: repository_commits id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_commits ALTER COLUMN id SET DEFAULT nextval('public.repository_commits_id_seq'::regclass);


--
-- Name: repository_issue_work_packages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_issue_work_packages ALTER COLUMN id SET DEFAULT nextval('public.repository_issue_work_packages_id_seq'::regclass);


--
-- Name: repository_issues id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_issues ALTER COLUMN id SET DEFAULT nextval('public.repository_issues_id_seq'::regclass);


--
-- Name: repository_merge_request_work_packages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_merge_request_work_packages ALTER COLUMN id SET DEFAULT nextval('public.repository_merge_request_work_packages_id_seq'::regclass);


--
-- Name: repository_merge_requests id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_merge_requests ALTER COLUMN id SET DEFAULT nextval('public.repository_merge_requests_id_seq'::regclass);


--
-- Name: repository_pipelines id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_pipelines ALTER COLUMN id SET DEFAULT nextval('public.repository_pipelines_id_seq'::regclass);


--
-- Name: repository_providers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_providers ALTER COLUMN id SET DEFAULT nextval('public.repository_providers_id_seq'::regclass);


--
-- Name: repository_users id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_users ALTER COLUMN id SET DEFAULT nextval('public.repository_users_id_seq'::regclass);


--
-- Name: resource_allocation_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocation_journals ALTER COLUMN id SET DEFAULT nextval('public.resource_allocation_journals_id_seq'::regclass);


--
-- Name: resource_allocation_scheduled_entries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocation_scheduled_entries ALTER COLUMN id SET DEFAULT nextval('public.resource_allocation_scheduled_entries_id_seq'::regclass);


--
-- Name: resource_allocations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocations ALTER COLUMN id SET DEFAULT nextval('public.resource_allocations_id_seq'::regclass);


--
-- Name: role_permissions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.role_permissions ALTER COLUMN id SET DEFAULT nextval('public.role_permissions_id_seq'::regclass);


--
-- Name: roles id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles ALTER COLUMN id SET DEFAULT nextval('public.roles_id_seq'::regclass);


--
-- Name: scim_clients id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.scim_clients ALTER COLUMN id SET DEFAULT nextval('public.scim_clients_id_seq'::regclass);


--
-- Name: semantic_indexes id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.semantic_indexes ALTER COLUMN id SET DEFAULT nextval('public.semantic_indexes_id_seq'::regclass);


--
-- Name: service_account_associations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.service_account_associations ALTER COLUMN id SET DEFAULT nextval('public.service_account_associations_id_seq'::regclass);


--
-- Name: sessions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sessions ALTER COLUMN id SET DEFAULT nextval('public.sessions_id_seq'::regclass);


--
-- Name: settings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.settings ALTER COLUMN id SET DEFAULT nextval('public.settings_id_seq'::regclass);


--
-- Name: sprint_goals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sprint_goals ALTER COLUMN id SET DEFAULT nextval('public.sprint_goals_id_seq'::regclass);


--
-- Name: sprints id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sprints ALTER COLUMN id SET DEFAULT nextval('public.sprints_id_seq'::regclass);


--
-- Name: squashtm_project_links id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.squashtm_project_links ALTER COLUMN id SET DEFAULT nextval('public.squashtm_project_links_id_seq'::regclass);


--
-- Name: squashtm_servers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.squashtm_servers ALTER COLUMN id SET DEFAULT nextval('public.squashtm_servers_id_seq'::regclass);


--
-- Name: squashtm_work_package_links id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.squashtm_work_package_links ALTER COLUMN id SET DEFAULT nextval('public.squashtm_work_package_links_id_seq'::regclass);


--
-- Name: statuses id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.statuses ALTER COLUMN id SET DEFAULT nextval('public.statuses_id_seq'::regclass);


--
-- Name: storages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.storages ALTER COLUMN id SET DEFAULT nextval('public.storages_id_seq'::regclass);


--
-- Name: storages_file_links_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.storages_file_links_journals ALTER COLUMN id SET DEFAULT nextval('public.storages_file_links_journals_id_seq'::regclass);


--
-- Name: subproject_template_assignments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.subproject_template_assignments ALTER COLUMN id SET DEFAULT nextval('public.subproject_template_assignments_id_seq'::regclass);


--
-- Name: time_entries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_entries ALTER COLUMN id SET DEFAULT nextval('public.time_entries_id_seq'::regclass);


--
-- Name: time_entry_activities_projects id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_entry_activities_projects ALTER COLUMN id SET DEFAULT nextval('public.time_entry_activities_projects_id_seq'::regclass);


--
-- Name: time_entry_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_entry_journals ALTER COLUMN id SET DEFAULT nextval('public.time_entry_journals_id_seq'::regclass);


--
-- Name: token_bucket_states id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.token_bucket_states ALTER COLUMN id SET DEFAULT nextval('public.token_bucket_states_id_seq'::regclass);


--
-- Name: tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tokens ALTER COLUMN id SET DEFAULT nextval('public.tokens_id_seq'::regclass);


--
-- Name: two_factor_authentication_devices id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.two_factor_authentication_devices ALTER COLUMN id SET DEFAULT nextval('public.two_factor_authentication_devices_id_seq'::regclass);


--
-- Name: type_variants id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.type_variants ALTER COLUMN id SET DEFAULT nextval('public.type_variants_id_seq'::regclass);


--
-- Name: types id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.types ALTER COLUMN id SET DEFAULT nextval('public.types_id_seq'::regclass);


--
-- Name: user_auth_provider_links id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_auth_provider_links ALTER COLUMN id SET DEFAULT nextval('public.user_auth_provider_links_id_seq'::regclass);


--
-- Name: user_non_working_times id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_non_working_times ALTER COLUMN id SET DEFAULT nextval('public.user_non_working_times_id_seq'::regclass);


--
-- Name: user_passwords id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_passwords ALTER COLUMN id SET DEFAULT nextval('public.user_passwords_id_seq'::regclass);


--
-- Name: user_preferences id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_preferences ALTER COLUMN id SET DEFAULT nextval('public.user_preferences_id_seq'::regclass);


--
-- Name: user_working_hours id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_working_hours ALTER COLUMN id SET DEFAULT nextval('public.user_working_hours_id_seq'::regclass);


--
-- Name: users id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users ALTER COLUMN id SET DEFAULT nextval('public.users_id_seq'::regclass);


--
-- Name: versions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.versions ALTER COLUMN id SET DEFAULT nextval('public.versions_id_seq'::regclass);


--
-- Name: views id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.views ALTER COLUMN id SET DEFAULT nextval('public.views_id_seq'::regclass);


--
-- Name: watchers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.watchers ALTER COLUMN id SET DEFAULT nextval('public.watchers_id_seq'::regclass);


--
-- Name: webhooks_events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_events ALTER COLUMN id SET DEFAULT nextval('public.webhooks_events_id_seq'::regclass);


--
-- Name: webhooks_logs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_logs ALTER COLUMN id SET DEFAULT nextval('public.webhooks_logs_id_seq'::regclass);


--
-- Name: webhooks_projects id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_projects ALTER COLUMN id SET DEFAULT nextval('public.webhooks_projects_id_seq'::regclass);


--
-- Name: webhooks_webhooks id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_webhooks ALTER COLUMN id SET DEFAULT nextval('public.webhooks_webhooks_id_seq'::regclass);


--
-- Name: wiki_page_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_page_journals ALTER COLUMN id SET DEFAULT nextval('public.wiki_page_journals_id_seq'::regclass);


--
-- Name: wiki_page_links id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_page_links ALTER COLUMN id SET DEFAULT nextval('public.wiki_page_links_id_seq'::regclass);


--
-- Name: wiki_pages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_pages ALTER COLUMN id SET DEFAULT nextval('public.wiki_pages_id_seq'::regclass);


--
-- Name: wiki_providers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_providers ALTER COLUMN id SET DEFAULT nextval('public.wiki_providers_id_seq'::regclass);


--
-- Name: wiki_redirects id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_redirects ALTER COLUMN id SET DEFAULT nextval('public.wiki_redirects_id_seq'::regclass);


--
-- Name: wikis id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wikis ALTER COLUMN id SET DEFAULT nextval('public.wikis_id_seq'::regclass);


--
-- Name: work_package_categories id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_categories ALTER COLUMN id SET DEFAULT nextval('public.work_package_categories_id_seq'::regclass);


--
-- Name: work_package_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_journals ALTER COLUMN id SET DEFAULT nextval('public.work_package_journals_id_seq'::regclass);


--
-- Name: work_package_semantic_aliases id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_semantic_aliases ALTER COLUMN id SET DEFAULT nextval('public.work_package_semantic_aliases_id_seq'::regclass);


--
-- Name: work_package_version_journals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_version_journals ALTER COLUMN id SET DEFAULT nextval('public.work_package_version_journals_id_seq'::regclass);


--
-- Name: work_package_versions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_versions ALTER COLUMN id SET DEFAULT nextval('public.work_package_versions_id_seq'::regclass);


--
-- Name: work_packages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_packages ALTER COLUMN id SET DEFAULT nextval('public.work_packages_id_seq'::regclass);


--
-- Name: workflows id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflows ALTER COLUMN id SET DEFAULT nextval('public.workflows_id_seq'::regclass);


--
-- Name: workflows_status_transitions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflows_status_transitions ALTER COLUMN id SET DEFAULT nextval('public.workflows_status_transitions_id_seq'::regclass);


--
-- Name: ai_text_action_types ai_text_action_types_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_action_types
    ADD CONSTRAINT ai_text_action_types_pkey PRIMARY KEY (id);


--
-- Name: ai_text_actions ai_text_actions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_actions
    ADD CONSTRAINT ai_text_actions_pkey PRIMARY KEY (id);


--
-- Name: ai_text_transform_run_events ai_text_transform_run_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_transform_run_events
    ADD CONSTRAINT ai_text_transform_run_events_pkey PRIMARY KEY (id);


--
-- Name: ai_text_transform_runs ai_text_transform_runs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_transform_runs
    ADD CONSTRAINT ai_text_transform_runs_pkey PRIMARY KEY (id);


--
-- Name: announcements announcements_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.announcements
    ADD CONSTRAINT announcements_pkey PRIMARY KEY (id);


--
-- Name: ar_internal_metadata ar_internal_metadata_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ar_internal_metadata
    ADD CONSTRAINT ar_internal_metadata_pkey PRIMARY KEY (key);


--
-- Name: attachable_journals attachable_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attachable_journals
    ADD CONSTRAINT attachable_journals_pkey PRIMARY KEY (id);


--
-- Name: attachment_journals attachment_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attachment_journals
    ADD CONSTRAINT attachment_journals_pkey PRIMARY KEY (id);


--
-- Name: attachments attachments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attachments
    ADD CONSTRAINT attachments_pkey PRIMARY KEY (id);


--
-- Name: attribute_help_texts attribute_help_texts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attribute_help_texts
    ADD CONSTRAINT attribute_help_texts_pkey PRIMARY KEY (id);


--
-- Name: auth_providers auth_providers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_providers
    ADD CONSTRAINT auth_providers_pkey PRIMARY KEY (id);


--
-- Name: autologin_session_links autologin_session_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.autologin_session_links
    ADD CONSTRAINT autologin_session_links_pkey PRIMARY KEY (id);


--
-- Name: automation_actions automation_actions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automation_actions
    ADD CONSTRAINT automation_actions_pkey PRIMARY KEY (id);


--
-- Name: automation_triggers automation_triggers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automation_triggers
    ADD CONSTRAINT automation_triggers_pkey PRIMARY KEY (id);


--
-- Name: automations automations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automations
    ADD CONSTRAINT automations_pkey PRIMARY KEY (id);


--
-- Name: automations_projects automations_projects_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automations_projects
    ADD CONSTRAINT automations_projects_pkey PRIMARY KEY (id);


--
-- Name: automations_roles automations_roles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automations_roles
    ADD CONSTRAINT automations_roles_pkey PRIMARY KEY (id);


--
-- Name: automations_statuses automations_statuses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automations_statuses
    ADD CONSTRAINT automations_statuses_pkey PRIMARY KEY (id);


--
-- Name: automations_types automations_types_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automations_types
    ADD CONSTRAINT automations_types_pkey PRIMARY KEY (id);


--
-- Name: backlog_buckets backlog_buckets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.backlog_buckets
    ADD CONSTRAINT backlog_buckets_pkey PRIMARY KEY (id);


--
-- Name: bcf_comments bcf_comments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bcf_comments
    ADD CONSTRAINT bcf_comments_pkey PRIMARY KEY (id);


--
-- Name: bcf_issues bcf_issues_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bcf_issues
    ADD CONSTRAINT bcf_issues_pkey PRIMARY KEY (id);


--
-- Name: bcf_viewpoints bcf_viewpoints_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bcf_viewpoints
    ADD CONSTRAINT bcf_viewpoints_pkey PRIMARY KEY (id);


--
-- Name: budget_journals budget_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.budget_journals
    ADD CONSTRAINT budget_journals_pkey PRIMARY KEY (id);


--
-- Name: budgets budgets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.budgets
    ADD CONSTRAINT budgets_pkey PRIMARY KEY (id);


--
-- Name: calculated_value_errors calculated_value_errors_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calculated_value_errors
    ADD CONSTRAINT calculated_value_errors_pkey PRIMARY KEY (id);


--
-- Name: categories categories_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.categories
    ADD CONSTRAINT categories_pkey PRIMARY KEY (id);


--
-- Name: changes changes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.changes
    ADD CONSTRAINT changes_pkey PRIMARY KEY (id);


--
-- Name: changeset_journals changeset_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.changeset_journals
    ADD CONSTRAINT changeset_journals_pkey PRIMARY KEY (id);


--
-- Name: changesets changesets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.changesets
    ADD CONSTRAINT changesets_pkey PRIMARY KEY (id);


--
-- Name: colors colors_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.colors
    ADD CONSTRAINT colors_pkey PRIMARY KEY (id);


--
-- Name: comments comments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.comments
    ADD CONSTRAINT comments_pkey PRIMARY KEY (id);


--
-- Name: cost_entries cost_entries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cost_entries
    ADD CONSTRAINT cost_entries_pkey PRIMARY KEY (id);


--
-- Name: cost_queries cost_queries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cost_queries
    ADD CONSTRAINT cost_queries_pkey PRIMARY KEY (id);


--
-- Name: cost_types cost_types_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cost_types
    ADD CONSTRAINT cost_types_pkey PRIMARY KEY (id);


--
-- Name: cost_types_projects cost_types_projects_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cost_types_projects
    ADD CONSTRAINT cost_types_projects_pkey PRIMARY KEY (id);


--
-- Name: custom_comment_journals custom_comment_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_comment_journals
    ADD CONSTRAINT custom_comment_journals_pkey PRIMARY KEY (id);


--
-- Name: custom_comments custom_comments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_comments
    ADD CONSTRAINT custom_comments_pkey PRIMARY KEY (id);


--
-- Name: custom_field_project_specific_value_restrictions custom_field_project_specific_value_restrictions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_field_project_specific_value_restrictions
    ADD CONSTRAINT custom_field_project_specific_value_restrictions_pkey PRIMARY KEY (id);


--
-- Name: custom_field_sections custom_field_sections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_field_sections
    ADD CONSTRAINT custom_field_sections_pkey PRIMARY KEY (id);


--
-- Name: custom_fields custom_fields_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_fields
    ADD CONSTRAINT custom_fields_pkey PRIMARY KEY (id);


--
-- Name: custom_fields_projects custom_fields_projects_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_fields_projects
    ADD CONSTRAINT custom_fields_projects_pkey PRIMARY KEY (id);


--
-- Name: custom_fields_roles custom_fields_roles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_fields_roles
    ADD CONSTRAINT custom_fields_roles_pkey PRIMARY KEY (id);


--
-- Name: custom_options custom_options_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_options
    ADD CONSTRAINT custom_options_pkey PRIMARY KEY (id);


--
-- Name: custom_styles custom_styles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_styles
    ADD CONSTRAINT custom_styles_pkey PRIMARY KEY (id);


--
-- Name: custom_values custom_values_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_values
    ADD CONSTRAINT custom_values_pkey PRIMARY KEY (id);


--
-- Name: customizable_journals customizable_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.customizable_journals
    ADD CONSTRAINT customizable_journals_pkey PRIMARY KEY (id);


--
-- Name: deploy_status_checks deploy_status_checks_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.deploy_status_checks
    ADD CONSTRAINT deploy_status_checks_pkey PRIMARY KEY (id);


--
-- Name: deploy_targets deploy_targets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.deploy_targets
    ADD CONSTRAINT deploy_targets_pkey PRIMARY KEY (id);


--
-- Name: design_colors design_colors_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.design_colors
    ADD CONSTRAINT design_colors_pkey PRIMARY KEY (id);


--
-- Name: document_journals document_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_journals
    ADD CONSTRAINT document_journals_pkey PRIMARY KEY (id);


--
-- Name: document_types document_types_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_types
    ADD CONSTRAINT document_types_pkey PRIMARY KEY (id);


--
-- Name: documents documents_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.documents
    ADD CONSTRAINT documents_pkey PRIMARY KEY (id);


--
-- Name: emoji_reactions emoji_reactions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.emoji_reactions
    ADD CONSTRAINT emoji_reactions_pkey PRIMARY KEY (id);


--
-- Name: enabled_modules enabled_modules_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.enabled_modules
    ADD CONSTRAINT enabled_modules_pkey PRIMARY KEY (id);


--
-- Name: enterprise_tokens enterprise_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.enterprise_tokens
    ADD CONSTRAINT enterprise_tokens_pkey PRIMARY KEY (id);


--
-- Name: enumerations enumerations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.enumerations
    ADD CONSTRAINT enumerations_pkey PRIMARY KEY (id);


--
-- Name: export_settings export_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.export_settings
    ADD CONSTRAINT export_settings_pkey PRIMARY KEY (id);


--
-- Name: exports exports_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.exports
    ADD CONSTRAINT exports_pkey PRIMARY KEY (id);


--
-- Name: favorites favorites_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.favorites
    ADD CONSTRAINT favorites_pkey PRIMARY KEY (id);


--
-- Name: file_links file_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_links
    ADD CONSTRAINT file_links_pkey PRIMARY KEY (id);


--
-- Name: form_configuration_attributes form_configuration_attributes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configuration_attributes
    ADD CONSTRAINT form_configuration_attributes_pkey PRIMARY KEY (id);


--
-- Name: form_configuration_attributes form_configuration_attributes_position; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configuration_attributes
    ADD CONSTRAINT form_configuration_attributes_position UNIQUE (form_configuration_group_id, "position") DEFERRABLE INITIALLY DEFERRED;


--
-- Name: form_configuration_groups form_configuration_groups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configuration_groups
    ADD CONSTRAINT form_configuration_groups_pkey PRIMARY KEY (id);


--
-- Name: form_configuration_groups form_configuration_groups_position; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configuration_groups
    ADD CONSTRAINT form_configuration_groups_position UNIQUE (form_configuration_id, "position") DEFERRABLE INITIALLY DEFERRED;


--
-- Name: form_configurations form_configurations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configurations
    ADD CONSTRAINT form_configurations_pkey PRIMARY KEY (id);


--
-- Name: forums forums_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.forums
    ADD CONSTRAINT forums_pkey PRIMARY KEY (id);


--
-- Name: friendly_id_slugs friendly_id_slugs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.friendly_id_slugs
    ADD CONSTRAINT friendly_id_slugs_pkey PRIMARY KEY (id);


--
-- Name: github_check_runs github_check_runs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.github_check_runs
    ADD CONSTRAINT github_check_runs_pkey PRIMARY KEY (id);


--
-- Name: github_pull_requests github_pull_requests_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.github_pull_requests
    ADD CONSTRAINT github_pull_requests_pkey PRIMARY KEY (id);


--
-- Name: github_users github_users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.github_users
    ADD CONSTRAINT github_users_pkey PRIMARY KEY (id);


--
-- Name: gitlab_branches gitlab_branches_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_branches
    ADD CONSTRAINT gitlab_branches_pkey PRIMARY KEY (id);


--
-- Name: gitlab_commits gitlab_commits_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_commits
    ADD CONSTRAINT gitlab_commits_pkey PRIMARY KEY (id);


--
-- Name: gitlab_commits_work_packages gitlab_commits_work_packages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_commits_work_packages
    ADD CONSTRAINT gitlab_commits_work_packages_pkey PRIMARY KEY (id);


--
-- Name: gitlab_issues gitlab_issues_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_issues
    ADD CONSTRAINT gitlab_issues_pkey PRIMARY KEY (id);


--
-- Name: gitlab_merge_requests gitlab_merge_requests_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_merge_requests
    ADD CONSTRAINT gitlab_merge_requests_pkey PRIMARY KEY (id);


--
-- Name: gitlab_pipelines gitlab_pipelines_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_pipelines
    ADD CONSTRAINT gitlab_pipelines_pkey PRIMARY KEY (id);


--
-- Name: gitlab_users gitlab_users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_users
    ADD CONSTRAINT gitlab_users_pkey PRIMARY KEY (id);


--
-- Name: good_job_batches good_job_batches_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.good_job_batches
    ADD CONSTRAINT good_job_batches_pkey PRIMARY KEY (id);


--
-- Name: good_job_executions good_job_executions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.good_job_executions
    ADD CONSTRAINT good_job_executions_pkey PRIMARY KEY (id);


--
-- Name: good_job_processes good_job_processes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.good_job_processes
    ADD CONSTRAINT good_job_processes_pkey PRIMARY KEY (id);


--
-- Name: good_job_settings good_job_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.good_job_settings
    ADD CONSTRAINT good_job_settings_pkey PRIMARY KEY (id);


--
-- Name: good_jobs good_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.good_jobs
    ADD CONSTRAINT good_jobs_pkey PRIMARY KEY (id);


--
-- Name: grid_widgets grid_widgets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.grid_widgets
    ADD CONSTRAINT grid_widgets_pkey PRIMARY KEY (id);


--
-- Name: grids grids_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.grids
    ADD CONSTRAINT grids_pkey PRIMARY KEY (id);


--
-- Name: group_details group_details_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_details
    ADD CONSTRAINT group_details_pkey PRIMARY KEY (id);


--
-- Name: group_users group_users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_users
    ADD CONSTRAINT group_users_pkey PRIMARY KEY (id);


--
-- Name: health_reports health_reports_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.health_reports
    ADD CONSTRAINT health_reports_pkey PRIMARY KEY (id);


--
-- Name: hierarchical_items hierarchical_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.hierarchical_items
    ADD CONSTRAINT hierarchical_items_pkey PRIMARY KEY (id);


--
-- Name: ical_token_query_assignments ical_token_query_assignments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ical_token_query_assignments
    ADD CONSTRAINT ical_token_query_assignments_pkey PRIMARY KEY (id);


--
-- Name: ifc_models ifc_models_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ifc_models
    ADD CONSTRAINT ifc_models_pkey PRIMARY KEY (id);


--
-- Name: itsm_email_messages itsm_email_messages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_email_messages
    ADD CONSTRAINT itsm_email_messages_pkey PRIMARY KEY (id);


--
-- Name: itsm_email_threads itsm_email_threads_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_email_threads
    ADD CONSTRAINT itsm_email_threads_pkey PRIMARY KEY (id);


--
-- Name: itsm_incoming_mailboxes itsm_incoming_mailboxes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_incoming_mailboxes
    ADD CONSTRAINT itsm_incoming_mailboxes_pkey PRIMARY KEY (id);


--
-- Name: itsm_mail_servers itsm_mail_servers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_mail_servers
    ADD CONSTRAINT itsm_mail_servers_pkey PRIMARY KEY (id);


--
-- Name: itsm_outgoing_mailboxes itsm_outgoing_mailboxes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_outgoing_mailboxes
    ADD CONSTRAINT itsm_outgoing_mailboxes_pkey PRIMARY KEY (id);


--
-- Name: jira_attachments jira_attachments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_attachments
    ADD CONSTRAINT jira_attachments_pkey PRIMARY KEY (id);


--
-- Name: jira_boards jira_boards_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_boards
    ADD CONSTRAINT jira_boards_pkey PRIMARY KEY (id);


--
-- Name: jira_fields jira_fields_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_fields
    ADD CONSTRAINT jira_fields_pkey PRIMARY KEY (id);


--
-- Name: jira_import_job_cursors jira_import_job_cursors_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_import_job_cursors
    ADD CONSTRAINT jira_import_job_cursors_pkey PRIMARY KEY (id);


--
-- Name: jira_import_transitions jira_import_transitions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_import_transitions
    ADD CONSTRAINT jira_import_transitions_pkey PRIMARY KEY (id);


--
-- Name: jira_imports jira_imports_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_imports
    ADD CONSTRAINT jira_imports_pkey PRIMARY KEY (id);


--
-- Name: jira_issue_types jira_issue_types_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_issue_types
    ADD CONSTRAINT jira_issue_types_pkey PRIMARY KEY (id);


--
-- Name: jira_issues jira_issues_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_issues
    ADD CONSTRAINT jira_issues_pkey PRIMARY KEY (id);


--
-- Name: jira_open_project_references jira_open_project_references_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_open_project_references
    ADD CONSTRAINT jira_open_project_references_pkey PRIMARY KEY (id);


--
-- Name: jira_priorities jira_priorities_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_priorities
    ADD CONSTRAINT jira_priorities_pkey PRIMARY KEY (id);


--
-- Name: jira_projects jira_projects_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_projects
    ADD CONSTRAINT jira_projects_pkey PRIMARY KEY (id);


--
-- Name: jira_sprints jira_sprints_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_sprints
    ADD CONSTRAINT jira_sprints_pkey PRIMARY KEY (id);


--
-- Name: jira_statuses jira_statuses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_statuses
    ADD CONSTRAINT jira_statuses_pkey PRIMARY KEY (id);


--
-- Name: jira_users jira_users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_users
    ADD CONSTRAINT jira_users_pkey PRIMARY KEY (id);


--
-- Name: jira_versions jira_versions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_versions
    ADD CONSTRAINT jira_versions_pkey PRIMARY KEY (id);


--
-- Name: jiras jiras_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jiras
    ADD CONSTRAINT jiras_pkey PRIMARY KEY (id);


--
-- Name: job_statuses job_statuses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_statuses
    ADD CONSTRAINT job_statuses_pkey PRIMARY KEY (id);


--
-- Name: journals journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.journals
    ADD CONSTRAINT journals_pkey PRIMARY KEY (id);


--
-- Name: label_journals label_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.label_journals
    ADD CONSTRAINT label_journals_pkey PRIMARY KEY (id);


--
-- Name: labelings labelings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.labelings
    ADD CONSTRAINT labelings_pkey PRIMARY KEY (id);


--
-- Name: labels labels_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.labels
    ADD CONSTRAINT labels_pkey PRIMARY KEY (id);


--
-- Name: labor_budget_items labor_budget_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.labor_budget_items
    ADD CONSTRAINT labor_budget_items_pkey PRIMARY KEY (id);


--
-- Name: last_project_folders last_project_folders_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.last_project_folders
    ADD CONSTRAINT last_project_folders_pkey PRIMARY KEY (id);


--
-- Name: ldap_auth_sources ldap_auth_sources_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_auth_sources
    ADD CONSTRAINT ldap_auth_sources_pkey PRIMARY KEY (id);


--
-- Name: ldap_departments_memberships ldap_departments_memberships_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_departments_memberships
    ADD CONSTRAINT ldap_departments_memberships_pkey PRIMARY KEY (id);


--
-- Name: ldap_departments_synchronized_departments ldap_departments_synchronized_departments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_departments_synchronized_departments
    ADD CONSTRAINT ldap_departments_synchronized_departments_pkey PRIMARY KEY (id);


--
-- Name: ldap_departments_synchronized_trees ldap_departments_synchronized_trees_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_departments_synchronized_trees
    ADD CONSTRAINT ldap_departments_synchronized_trees_pkey PRIMARY KEY (id);


--
-- Name: ldap_groups_memberships ldap_groups_memberships_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_groups_memberships
    ADD CONSTRAINT ldap_groups_memberships_pkey PRIMARY KEY (id);


--
-- Name: ldap_groups_synchronized_filters ldap_groups_synchronized_filters_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_groups_synchronized_filters
    ADD CONSTRAINT ldap_groups_synchronized_filters_pkey PRIMARY KEY (id);


--
-- Name: ldap_groups_synchronized_groups ldap_groups_synchronized_groups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_groups_synchronized_groups
    ADD CONSTRAINT ldap_groups_synchronized_groups_pkey PRIMARY KEY (id);


--
-- Name: llm_capability_verdicts llm_capability_verdicts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_capability_verdicts
    ADD CONSTRAINT llm_capability_verdicts_pkey PRIMARY KEY (id);


--
-- Name: llm_connections llm_connections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_connections
    ADD CONSTRAINT llm_connections_pkey PRIMARY KEY (id);


--
-- Name: llm_feature_bindings llm_feature_bindings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_feature_bindings
    ADD CONSTRAINT llm_feature_bindings_pkey PRIMARY KEY (id);


--
-- Name: llm_models llm_models_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_models
    ADD CONSTRAINT llm_models_pkey PRIMARY KEY (id);


--
-- Name: material_budget_items material_budget_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.material_budget_items
    ADD CONSTRAINT material_budget_items_pkey PRIMARY KEY (id);


--
-- Name: mcp_configurations mcp_configurations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mcp_configurations
    ADD CONSTRAINT mcp_configurations_pkey PRIMARY KEY (id);


--
-- Name: meeting_agenda_item_journals meeting_agenda_item_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_agenda_item_journals
    ADD CONSTRAINT meeting_agenda_item_journals_pkey PRIMARY KEY (id);


--
-- Name: meeting_agenda_items meeting_agenda_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_agenda_items
    ADD CONSTRAINT meeting_agenda_items_pkey PRIMARY KEY (id);


--
-- Name: meeting_journals meeting_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_journals
    ADD CONSTRAINT meeting_journals_pkey PRIMARY KEY (id);


--
-- Name: meeting_outcome_journals meeting_outcome_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_outcome_journals
    ADD CONSTRAINT meeting_outcome_journals_pkey PRIMARY KEY (id);


--
-- Name: meeting_outcomes meeting_outcomes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_outcomes
    ADD CONSTRAINT meeting_outcomes_pkey PRIMARY KEY (id);


--
-- Name: meeting_participant_journals meeting_participant_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_participant_journals
    ADD CONSTRAINT meeting_participant_journals_pkey PRIMARY KEY (id);


--
-- Name: meeting_participants meeting_participants_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_participants
    ADD CONSTRAINT meeting_participants_pkey PRIMARY KEY (id);


--
-- Name: meeting_section_journals meeting_section_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_section_journals
    ADD CONSTRAINT meeting_section_journals_pkey PRIMARY KEY (id);


--
-- Name: meeting_sections meeting_sections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_sections
    ADD CONSTRAINT meeting_sections_pkey PRIMARY KEY (id);


--
-- Name: meetings meetings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meetings
    ADD CONSTRAINT meetings_pkey PRIMARY KEY (id);


--
-- Name: member_roles member_roles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.member_roles
    ADD CONSTRAINT member_roles_pkey PRIMARY KEY (id);


--
-- Name: members members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.members
    ADD CONSTRAINT members_pkey PRIMARY KEY (id);


--
-- Name: menu_items menu_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.menu_items
    ADD CONSTRAINT menu_items_pkey PRIMARY KEY (id);


--
-- Name: message_journals message_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_journals
    ADD CONSTRAINT message_journals_pkey PRIMARY KEY (id);


--
-- Name: message_work_packages message_work_packages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_work_packages
    ADD CONSTRAINT message_work_packages_pkey PRIMARY KEY (id);


--
-- Name: messages messages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT messages_pkey PRIMARY KEY (id);


--
-- Name: news_journals news_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news_journals
    ADD CONSTRAINT news_journals_pkey PRIMARY KEY (id);


--
-- Name: news news_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.news
    ADD CONSTRAINT news_pkey PRIMARY KEY (id);


--
-- Name: user_non_working_times no_overlapping_non_working_times; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_non_working_times
    ADD CONSTRAINT no_overlapping_non_working_times EXCLUDE USING gist (user_id WITH =, daterange(start_date, end_date, '[]'::text) WITH &&);


--
-- Name: journals non_overlapping_journals_validity_periods; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.journals
    ADD CONSTRAINT non_overlapping_journals_validity_periods EXCLUDE USING gist (journable_id WITH =, journable_type WITH =, validity_period WITH &&) DEFERRABLE;


--
-- Name: non_working_days non_working_days_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.non_working_days
    ADD CONSTRAINT non_working_days_pkey PRIMARY KEY (id);


--
-- Name: notification_settings notification_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notification_settings
    ADD CONSTRAINT notification_settings_pkey PRIMARY KEY (id);


--
-- Name: notifications notifications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);


--
-- Name: oauth_access_grants oauth_access_grants_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_access_grants
    ADD CONSTRAINT oauth_access_grants_pkey PRIMARY KEY (id);


--
-- Name: oauth_access_tokens oauth_access_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_access_tokens
    ADD CONSTRAINT oauth_access_tokens_pkey PRIMARY KEY (id);


--
-- Name: oauth_applications oauth_applications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_applications
    ADD CONSTRAINT oauth_applications_pkey PRIMARY KEY (id);


--
-- Name: oauth_client_tokens oauth_client_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_client_tokens
    ADD CONSTRAINT oauth_client_tokens_pkey PRIMARY KEY (id);


--
-- Name: oauth_clients oauth_clients_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_clients
    ADD CONSTRAINT oauth_clients_pkey PRIMARY KEY (id);


--
-- Name: oidc_group_links oidc_group_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_group_links
    ADD CONSTRAINT oidc_group_links_pkey PRIMARY KEY (id);


--
-- Name: oidc_group_memberships oidc_group_memberships_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_group_memberships
    ADD CONSTRAINT oidc_group_memberships_pkey PRIMARY KEY (id);


--
-- Name: oidc_user_session_links oidc_user_session_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_user_session_links
    ADD CONSTRAINT oidc_user_session_links_pkey PRIMARY KEY (id);


--
-- Name: oidc_user_tokens oidc_user_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_user_tokens
    ADD CONSTRAINT oidc_user_tokens_pkey PRIMARY KEY (id);


--
-- Name: ordered_persisted_query_entities ordered_persisted_query_entities_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ordered_persisted_query_entities
    ADD CONSTRAINT ordered_persisted_query_entities_pkey PRIMARY KEY (id);


--
-- Name: ordered_work_packages ordered_work_packages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ordered_work_packages
    ADD CONSTRAINT ordered_work_packages_pkey PRIMARY KEY (id);


--
-- Name: outbound_mail_recipients outbound_mail_recipients_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.outbound_mail_recipients
    ADD CONSTRAINT outbound_mail_recipients_pkey PRIMARY KEY (id);


--
-- Name: paper_trail_audits paper_trail_audits_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.paper_trail_audits
    ADD CONSTRAINT paper_trail_audits_pkey PRIMARY KEY (id);


--
-- Name: persisted_queries persisted_queries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.persisted_queries
    ADD CONSTRAINT persisted_queries_pkey PRIMARY KEY (id);


--
-- Name: persisted_views persisted_views_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.persisted_views
    ADD CONSTRAINT persisted_views_pkey PRIMARY KEY (id);


--
-- Name: placeholder_user_details placeholder_user_details_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.placeholder_user_details
    ADD CONSTRAINT placeholder_user_details_pkey PRIMARY KEY (id);


--
-- Name: project_custom_field_project_mappings project_custom_field_project_mappings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_custom_field_project_mappings
    ADD CONSTRAINT project_custom_field_project_mappings_pkey PRIMARY KEY (id);


--
-- Name: project_custom_field_type_mappings project_custom_field_type_mappings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_custom_field_type_mappings
    ADD CONSTRAINT project_custom_field_type_mappings_pkey PRIMARY KEY (id);


--
-- Name: project_journals project_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_journals
    ADD CONSTRAINT project_journals_pkey PRIMARY KEY (id);


--
-- Name: project_phase_definitions project_phase_definitions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_phase_definitions
    ADD CONSTRAINT project_phase_definitions_pkey PRIMARY KEY (id);


--
-- Name: project_phase_journals project_phase_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_phase_journals
    ADD CONSTRAINT project_phase_journals_pkey PRIMARY KEY (id);


--
-- Name: project_phases project_phases_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_phases
    ADD CONSTRAINT project_phases_pkey PRIMARY KEY (id);


--
-- Name: project_queries project_queries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_queries
    ADD CONSTRAINT project_queries_pkey PRIMARY KEY (id);


--
-- Name: project_storages project_storages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_storages
    ADD CONSTRAINT project_storages_pkey PRIMARY KEY (id);


--
-- Name: project_types project_types_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_types
    ADD CONSTRAINT project_types_pkey PRIMARY KEY (id);


--
-- Name: projects projects_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.projects
    ADD CONSTRAINT projects_pkey PRIMARY KEY (id);


--
-- Name: push_subscriptions push_subscriptions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.push_subscriptions
    ADD CONSTRAINT push_subscriptions_pkey PRIMARY KEY (id);


--
-- Name: queries queries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.queries
    ADD CONSTRAINT queries_pkey PRIMARY KEY (id);


--
-- Name: rates rates_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.rates
    ADD CONSTRAINT rates_pkey PRIMARY KEY (id);


--
-- Name: recaptcha_entries recaptcha_entries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recaptcha_entries
    ADD CONSTRAINT recaptcha_entries_pkey PRIMARY KEY (id);


--
-- Name: recurring_meeting_historic_schedules recurring_meeting_historic_schedules_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_meeting_historic_schedules
    ADD CONSTRAINT recurring_meeting_historic_schedules_pkey PRIMARY KEY (id);


--
-- Name: recurring_meeting_interim_responses recurring_meeting_interim_responses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_meeting_interim_responses
    ADD CONSTRAINT recurring_meeting_interim_responses_pkey PRIMARY KEY (id);


--
-- Name: recurring_meetings recurring_meetings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_meetings
    ADD CONSTRAINT recurring_meetings_pkey PRIMARY KEY (id);


--
-- Name: relations relations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.relations
    ADD CONSTRAINT relations_pkey PRIMARY KEY (id);


--
-- Name: reminder_notifications reminder_notifications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reminder_notifications
    ADD CONSTRAINT reminder_notifications_pkey PRIMARY KEY (id);


--
-- Name: reminders reminders_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reminders
    ADD CONSTRAINT reminders_pkey PRIMARY KEY (id);


--
-- Name: remote_identities remote_identities_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.remote_identities
    ADD CONSTRAINT remote_identities_pkey PRIMARY KEY (id);


--
-- Name: repositories repositories_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repositories
    ADD CONSTRAINT repositories_pkey PRIMARY KEY (id);


--
-- Name: repository_branches repository_branches_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_branches
    ADD CONSTRAINT repository_branches_pkey PRIMARY KEY (id);


--
-- Name: repository_commit_work_packages repository_commit_work_packages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_commit_work_packages
    ADD CONSTRAINT repository_commit_work_packages_pkey PRIMARY KEY (id);


--
-- Name: repository_commits repository_commits_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_commits
    ADD CONSTRAINT repository_commits_pkey PRIMARY KEY (id);


--
-- Name: repository_issue_work_packages repository_issue_work_packages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_issue_work_packages
    ADD CONSTRAINT repository_issue_work_packages_pkey PRIMARY KEY (id);


--
-- Name: repository_issues repository_issues_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_issues
    ADD CONSTRAINT repository_issues_pkey PRIMARY KEY (id);


--
-- Name: repository_merge_request_work_packages repository_merge_request_work_packages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_merge_request_work_packages
    ADD CONSTRAINT repository_merge_request_work_packages_pkey PRIMARY KEY (id);


--
-- Name: repository_merge_requests repository_merge_requests_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_merge_requests
    ADD CONSTRAINT repository_merge_requests_pkey PRIMARY KEY (id);


--
-- Name: repository_pipelines repository_pipelines_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_pipelines
    ADD CONSTRAINT repository_pipelines_pkey PRIMARY KEY (id);


--
-- Name: repository_providers repository_providers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_providers
    ADD CONSTRAINT repository_providers_pkey PRIMARY KEY (id);


--
-- Name: repository_users repository_users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_users
    ADD CONSTRAINT repository_users_pkey PRIMARY KEY (id);


--
-- Name: resource_allocation_journals resource_allocation_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocation_journals
    ADD CONSTRAINT resource_allocation_journals_pkey PRIMARY KEY (id);


--
-- Name: resource_allocation_scheduled_entries resource_allocation_scheduled_entries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocation_scheduled_entries
    ADD CONSTRAINT resource_allocation_scheduled_entries_pkey PRIMARY KEY (id);


--
-- Name: resource_allocations resource_allocations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocations
    ADD CONSTRAINT resource_allocations_pkey PRIMARY KEY (id);


--
-- Name: role_permissions role_permissions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.role_permissions
    ADD CONSTRAINT role_permissions_pkey PRIMARY KEY (id);


--
-- Name: roles roles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles
    ADD CONSTRAINT roles_pkey PRIMARY KEY (id);


--
-- Name: schema_migrations schema_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.schema_migrations
    ADD CONSTRAINT schema_migrations_pkey PRIMARY KEY (version);


--
-- Name: scim_clients scim_clients_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.scim_clients
    ADD CONSTRAINT scim_clients_pkey PRIMARY KEY (id);


--
-- Name: semantic_indexes semantic_indexes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.semantic_indexes
    ADD CONSTRAINT semantic_indexes_pkey PRIMARY KEY (id);


--
-- Name: service_account_associations service_account_associations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.service_account_associations
    ADD CONSTRAINT service_account_associations_pkey PRIMARY KEY (id);


--
-- Name: sessions sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sessions
    ADD CONSTRAINT sessions_pkey PRIMARY KEY (id);


--
-- Name: settings settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.settings
    ADD CONSTRAINT settings_pkey PRIMARY KEY (id);


--
-- Name: sprint_goals sprint_goals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sprint_goals
    ADD CONSTRAINT sprint_goals_pkey PRIMARY KEY (id);


--
-- Name: sprints sprints_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sprints
    ADD CONSTRAINT sprints_pkey PRIMARY KEY (id);


--
-- Name: squashtm_project_links squashtm_project_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.squashtm_project_links
    ADD CONSTRAINT squashtm_project_links_pkey PRIMARY KEY (id);


--
-- Name: squashtm_servers squashtm_servers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.squashtm_servers
    ADD CONSTRAINT squashtm_servers_pkey PRIMARY KEY (id);


--
-- Name: squashtm_work_package_links squashtm_work_package_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.squashtm_work_package_links
    ADD CONSTRAINT squashtm_work_package_links_pkey PRIMARY KEY (id);


--
-- Name: statuses statuses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.statuses
    ADD CONSTRAINT statuses_pkey PRIMARY KEY (id);


--
-- Name: storages_file_links_journals storages_file_links_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.storages_file_links_journals
    ADD CONSTRAINT storages_file_links_journals_pkey PRIMARY KEY (id);


--
-- Name: storages storages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.storages
    ADD CONSTRAINT storages_pkey PRIMARY KEY (id);


--
-- Name: subproject_template_assignments subproject_template_assignments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.subproject_template_assignments
    ADD CONSTRAINT subproject_template_assignments_pkey PRIMARY KEY (id);


--
-- Name: time_entries time_entries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_entries
    ADD CONSTRAINT time_entries_pkey PRIMARY KEY (id);


--
-- Name: time_entry_activities_projects time_entry_activities_projects_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_entry_activities_projects
    ADD CONSTRAINT time_entry_activities_projects_pkey PRIMARY KEY (id);


--
-- Name: time_entry_journals time_entry_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_entry_journals
    ADD CONSTRAINT time_entry_journals_pkey PRIMARY KEY (id);


--
-- Name: token_bucket_states token_bucket_states_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.token_bucket_states
    ADD CONSTRAINT token_bucket_states_pkey PRIMARY KEY (id);


--
-- Name: tokens tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tokens
    ADD CONSTRAINT tokens_pkey PRIMARY KEY (id);


--
-- Name: two_factor_authentication_devices two_factor_authentication_devices_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.two_factor_authentication_devices
    ADD CONSTRAINT two_factor_authentication_devices_pkey PRIMARY KEY (id);


--
-- Name: type_variants type_variants_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.type_variants
    ADD CONSTRAINT type_variants_pkey PRIMARY KEY (id);


--
-- Name: types types_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.types
    ADD CONSTRAINT types_pkey PRIMARY KEY (id);


--
-- Name: user_auth_provider_links user_auth_provider_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_auth_provider_links
    ADD CONSTRAINT user_auth_provider_links_pkey PRIMARY KEY (id);


--
-- Name: user_non_working_times user_non_working_times_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_non_working_times
    ADD CONSTRAINT user_non_working_times_pkey PRIMARY KEY (id);


--
-- Name: user_passwords user_passwords_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_passwords
    ADD CONSTRAINT user_passwords_pkey PRIMARY KEY (id);


--
-- Name: user_preferences user_preferences_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_preferences
    ADD CONSTRAINT user_preferences_pkey PRIMARY KEY (id);


--
-- Name: user_working_hours user_working_hours_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_working_hours
    ADD CONSTRAINT user_working_hours_pkey PRIMARY KEY (id);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: versions versions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.versions
    ADD CONSTRAINT versions_pkey PRIMARY KEY (id);


--
-- Name: views views_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.views
    ADD CONSTRAINT views_pkey PRIMARY KEY (id);


--
-- Name: watchers watchers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.watchers
    ADD CONSTRAINT watchers_pkey PRIMARY KEY (id);


--
-- Name: webhooks_events webhooks_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_events
    ADD CONSTRAINT webhooks_events_pkey PRIMARY KEY (id);


--
-- Name: webhooks_logs webhooks_logs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_logs
    ADD CONSTRAINT webhooks_logs_pkey PRIMARY KEY (id);


--
-- Name: webhooks_projects webhooks_projects_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_projects
    ADD CONSTRAINT webhooks_projects_pkey PRIMARY KEY (id);


--
-- Name: webhooks_webhooks webhooks_webhooks_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_webhooks
    ADD CONSTRAINT webhooks_webhooks_pkey PRIMARY KEY (id);


--
-- Name: wiki_page_journals wiki_page_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_page_journals
    ADD CONSTRAINT wiki_page_journals_pkey PRIMARY KEY (id);


--
-- Name: wiki_page_links wiki_page_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_page_links
    ADD CONSTRAINT wiki_page_links_pkey PRIMARY KEY (id);


--
-- Name: wiki_pages wiki_pages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_pages
    ADD CONSTRAINT wiki_pages_pkey PRIMARY KEY (id);


--
-- Name: wiki_providers wiki_providers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_providers
    ADD CONSTRAINT wiki_providers_pkey PRIMARY KEY (id);


--
-- Name: wiki_redirects wiki_redirects_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_redirects
    ADD CONSTRAINT wiki_redirects_pkey PRIMARY KEY (id);


--
-- Name: wikis wikis_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wikis
    ADD CONSTRAINT wikis_pkey PRIMARY KEY (id);


--
-- Name: work_package_categories work_package_categories_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_categories
    ADD CONSTRAINT work_package_categories_pkey PRIMARY KEY (id);


--
-- Name: work_package_journals work_package_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_journals
    ADD CONSTRAINT work_package_journals_pkey PRIMARY KEY (id);


--
-- Name: work_package_semantic_aliases work_package_semantic_aliases_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_semantic_aliases
    ADD CONSTRAINT work_package_semantic_aliases_pkey PRIMARY KEY (id);


--
-- Name: work_package_version_journals work_package_version_journals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_version_journals
    ADD CONSTRAINT work_package_version_journals_pkey PRIMARY KEY (id);


--
-- Name: work_package_versions work_package_versions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_versions
    ADD CONSTRAINT work_package_versions_pkey PRIMARY KEY (id);


--
-- Name: work_packages work_packages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_packages
    ADD CONSTRAINT work_packages_pkey PRIMARY KEY (id);


--
-- Name: workflows workflows_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflows
    ADD CONSTRAINT workflows_pkey PRIMARY KEY (id);


--
-- Name: workflows_status_transitions workflows_status_transitions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflows_status_transitions
    ADD CONSTRAINT workflows_status_transitions_pkey PRIMARY KEY (id);


--
-- Name: changesets_changeset_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX changesets_changeset_id ON public.changes USING btree (changeset_id);


--
-- Name: changesets_repos_rev; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX changesets_repos_rev ON public.changesets USING btree (repository_id, revision);


--
-- Name: changesets_repos_scmid; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX changesets_repos_scmid ON public.changesets USING btree (repository_id, scmid);


--
-- Name: changesets_work_packages_ids; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX changesets_work_packages_ids ON public.changesets_work_packages USING btree (changeset_id, work_package_id);


--
-- Name: custom_values_customized; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX custom_values_customized ON public.custom_values USING btree (customized_type, customized_id);


--
-- Name: documents_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX documents_project_id ON public.documents USING btree (project_id);


--
-- Name: enabled_modules_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX enabled_modules_project_id ON public.enabled_modules USING btree (project_id);


--
-- Name: forums_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX forums_project_id ON public.forums USING btree (project_id);


--
-- Name: github_pr_wp_pr_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX github_pr_wp_pr_id ON public.github_pull_requests_work_packages USING btree (github_pull_request_id);


--
-- Name: gitlab_issues_wp_issue_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX gitlab_issues_wp_issue_id ON public.gitlab_issues_work_packages USING btree (gitlab_issue_id);


--
-- Name: gitlab_mr_wp_mr_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX gitlab_mr_wp_mr_id ON public.gitlab_merge_requests_work_packages USING btree (gitlab_merge_request_id);


--
-- Name: group_user_ids; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX group_user_ids ON public.group_users USING btree (group_id, user_id);


--
-- Name: idx_ai_text_action_types_on_action_and_type; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_ai_text_action_types_on_action_and_type ON public.ai_text_action_types USING btree (ai_text_action_id, type_id);


--
-- Name: idx_backlog_excluded_types; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_backlog_excluded_types ON public.backlog_excluded_types USING btree (project_id, type_id);


--
-- Name: idx_meeting_agenda_item_journals_journal_item; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_meeting_agenda_item_journals_journal_item ON public.meeting_agenda_item_journals USING btree (journal_id, agenda_item_id);


--
-- Name: idx_meeting_outcome_journals_journal_outcome; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_meeting_outcome_journals_journal_outcome ON public.meeting_outcome_journals USING btree (journal_id, outcome_id);


--
-- Name: idx_meeting_participant_journals_journal_user; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_meeting_participant_journals_journal_user ON public.meeting_participant_journals USING btree (journal_id, user_id);


--
-- Name: idx_meeting_section_journals_journal_section; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_meeting_section_journals_journal_section ON public.meeting_section_journals USING btree (journal_id, section_id);


--
-- Name: idx_on_auth_provider_id_external_id_a8e6468bb9; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_auth_provider_id_external_id_a8e6468bb9 ON public.user_auth_provider_links USING btree (auth_provider_id, external_id);


--
-- Name: idx_on_auth_provider_id_group_user_id_f86563dc26; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_auth_provider_id_group_user_id_f86563dc26 ON public.oidc_group_memberships USING btree (auth_provider_id, group_user_id);


--
-- Name: idx_on_custom_field_id_customized_type_customized_i_88a74ac425; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_custom_field_id_customized_type_customized_i_88a74ac425 ON public.custom_comments USING btree (custom_field_id, customized_type, customized_id);


--
-- Name: idx_on_form_configuration_group_id_eb1b1a9890; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_on_form_configuration_group_id_eb1b1a9890 ON public.form_configuration_attributes USING btree (form_configuration_group_id);


--
-- Name: idx_on_jira_import_id_job_class_arguments_ceeff94524; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_jira_import_id_job_class_arguments_ceeff94524 ON public.jira_import_job_cursors USING btree (jira_import_id, job_class, arguments);


--
-- Name: idx_on_jira_import_id_op_entity_id_op_entity_class_a96c4c5198; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_jira_import_id_op_entity_id_op_entity_class_a96c4c5198 ON public.jira_open_project_references USING btree (jira_import_id, op_entity_id, op_entity_class);


--
-- Name: idx_on_ldap_auth_source_id_7351d9f517; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_on_ldap_auth_source_id_7351d9f517 ON public.ldap_departments_synchronized_trees USING btree (ldap_auth_source_id);


--
-- Name: idx_on_ldap_auth_source_id_b7770e09c5; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_on_ldap_auth_source_id_b7770e09c5 ON public.ldap_departments_synchronized_departments USING btree (ldap_auth_source_id);


--
-- Name: idx_on_ldap_entry_uuid_15e07227e6; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_on_ldap_entry_uuid_15e07227e6 ON public.ldap_departments_synchronized_departments USING btree (ldap_entry_uuid);


--
-- Name: idx_on_llm_connection_id_feature_key_fb43b8c8b5; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_llm_connection_id_feature_key_fb43b8c8b5 ON public.llm_feature_bindings USING btree (llm_connection_id, feature_key);


--
-- Name: idx_on_merge_request_id_cf1891223f; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_on_merge_request_id_cf1891223f ON public.repository_merge_request_work_packages USING btree (merge_request_id);


--
-- Name: idx_on_provider_id_external_project_id_name_5ab8806581; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_provider_id_external_project_id_name_5ab8806581 ON public.repository_branches USING btree (provider_id, external_project_id, name);


--
-- Name: idx_on_recurring_meeting_id_e5d98678ae; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_on_recurring_meeting_id_e5d98678ae ON public.recurring_meeting_historic_schedules USING btree (recurring_meeting_id);


--
-- Name: idx_on_recurring_meeting_id_start_time_user_id_0ea652f142; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_recurring_meeting_id_start_time_user_id_0ea652f142 ON public.recurring_meeting_interim_responses USING btree (recurring_meeting_id, start_time, user_id);


--
-- Name: idx_on_resource_allocation_id_allocated_on_00262b61d1; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_resource_allocation_id_allocated_on_00262b61d1 ON public.resource_allocation_scheduled_entries USING btree (resource_allocation_id, allocated_on);


--
-- Name: idx_on_service_type_service_id_a76ab08fa2; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_service_type_service_id_a76ab08fa2 ON public.service_account_associations USING btree (service_type, service_id);


--
-- Name: idx_on_subject_type_subject_id_created_at_0d0955e9d2; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_on_subject_type_subject_id_created_at_0d0955e9d2 ON public.health_reports USING btree (subject_type, subject_id, created_at);


--
-- Name: idx_on_synchronized_department_id_e4d0413b19; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_on_synchronized_department_id_e4d0413b19 ON public.ldap_departments_memberships USING btree (synchronized_department_id);


--
-- Name: idx_on_synchronized_tree_id_83a47156ea; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_on_synchronized_tree_id_83a47156ea ON public.ldap_departments_synchronized_departments USING btree (synchronized_tree_id);


--
-- Name: idx_on_type_linkable_type_linkable_id_3a275784df; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_on_type_linkable_type_linkable_id_3a275784df ON public.wiki_page_links USING btree (type, linkable_type, linkable_id);


--
-- Name: idx_on_user_id_allocated_on_8e37eec1ee; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_on_user_id_allocated_on_8e37eec1ee ON public.resource_allocation_scheduled_entries USING btree (user_id, allocated_on);


--
-- Name: idx_on_user_id_auth_source_type_auth_source_id_inte_fc6941e656; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_user_id_auth_source_type_auth_source_id_inte_fc6941e656 ON public.remote_identities USING btree (user_id, auth_source_type, auth_source_id, integration_id, integration_type);


--
-- Name: idx_on_user_id_synchronized_department_id_600e61ade6; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_user_id_synchronized_department_id_600e61ade6 ON public.ldap_departments_memberships USING btree (user_id, synchronized_department_id);


--
-- Name: idx_on_work_package_id_commit_id_0841060697; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_work_package_id_commit_id_0841060697 ON public.repository_commit_work_packages USING btree (work_package_id, commit_id);


--
-- Name: idx_on_work_package_id_issue_id_f3beca802c; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_work_package_id_issue_id_f3beca802c ON public.repository_issue_work_packages USING btree (work_package_id, issue_id);


--
-- Name: idx_on_work_package_id_merge_request_id_5d4acd0aa1; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_on_work_package_id_merge_request_id_5d4acd0aa1 ON public.repository_merge_request_work_packages USING btree (work_package_id, merge_request_id);


--
-- Name: idx_subproj_tmpl_assignments_on_project_workspace; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_subproj_tmpl_assignments_on_project_workspace ON public.subproject_template_assignments USING btree (project_id, workspace_type);


--
-- Name: idx_wp_categories_on_category; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_wp_categories_on_category ON public.work_package_categories USING btree (category_id);


--
-- Name: idx_wp_categories_on_wp_category; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_wp_categories_on_wp_category ON public.work_package_categories USING btree (work_package_id, category_id);


--
-- Name: idx_wp_versions_on_version; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_wp_versions_on_version ON public.work_package_versions USING btree (version_id);


--
-- Name: idx_wp_versions_on_wp_version_kind; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_wp_versions_on_wp_version_kind ON public.work_package_versions USING btree (work_package_id, version_id, kind);


--
-- Name: index_ai_text_action_types_on_type_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ai_text_action_types_on_type_id ON public.ai_text_action_types USING btree (type_id);


--
-- Name: index_ai_text_transform_run_events_on_run_id_and_seq; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_ai_text_transform_run_events_on_run_id_and_seq ON public.ai_text_transform_run_events USING btree (run_id, seq);


--
-- Name: index_ai_text_transform_runs_on_ai_text_action_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ai_text_transform_runs_on_ai_text_action_id ON public.ai_text_transform_runs USING btree (ai_text_action_id);


--
-- Name: index_ai_text_transform_runs_on_finished_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ai_text_transform_runs_on_finished_at ON public.ai_text_transform_runs USING btree (finished_at);


--
-- Name: index_ai_text_transform_runs_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ai_text_transform_runs_on_user_id ON public.ai_text_transform_runs USING btree (user_id);


--
-- Name: index_ai_text_transform_runs_on_uuid; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_ai_text_transform_runs_on_uuid ON public.ai_text_transform_runs USING btree (uuid);


--
-- Name: index_announcements_on_show_until_and_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_announcements_on_show_until_and_active ON public.announcements USING btree (show_until, active);


--
-- Name: index_attachable_journals_on_attachment_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_attachable_journals_on_attachment_id ON public.attachable_journals USING btree (attachment_id);


--
-- Name: index_attachable_journals_on_journal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_attachable_journals_on_journal_id ON public.attachable_journals USING btree (journal_id);


--
-- Name: index_attachments_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_attachments_on_author_id ON public.attachments USING btree (author_id);


--
-- Name: index_attachments_on_container_id_and_container_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_attachments_on_container_id_and_container_type ON public.attachments USING btree (container_id, container_type);


--
-- Name: index_attachments_on_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_attachments_on_created_at ON public.attachments USING btree (created_at);


--
-- Name: index_attachments_on_file_tsv; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_attachments_on_file_tsv ON public.attachments USING gin (file_tsv);


--
-- Name: index_attachments_on_fulltext_tsv; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_attachments_on_fulltext_tsv ON public.attachments USING gin (fulltext_tsv);


--
-- Name: index_auth_providers_on_creator_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_auth_providers_on_creator_id ON public.auth_providers USING btree (creator_id);


--
-- Name: index_auth_providers_on_display_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_auth_providers_on_display_name ON public.auth_providers USING btree (display_name);


--
-- Name: index_auth_providers_on_slug; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_auth_providers_on_slug ON public.auth_providers USING btree (slug);


--
-- Name: index_autologin_session_links_on_session_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_autologin_session_links_on_session_id ON public.autologin_session_links USING btree (session_id);


--
-- Name: index_autologin_session_links_on_token_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_autologin_session_links_on_token_id ON public.autologin_session_links USING btree (token_id);


--
-- Name: index_automation_actions_on_automation_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_automation_actions_on_automation_id ON public.automation_actions USING btree (automation_id);


--
-- Name: index_automation_actions_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_automation_actions_on_custom_field_id ON public.automation_actions USING btree (custom_field_id);


--
-- Name: index_automation_triggers_on_automation_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_automation_triggers_on_automation_id ON public.automation_triggers USING btree (automation_id);


--
-- Name: index_automations_projects_on_automation_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_automations_projects_on_automation_id ON public.automations_projects USING btree (automation_id);


--
-- Name: index_automations_projects_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_automations_projects_on_project_id ON public.automations_projects USING btree (project_id);


--
-- Name: index_automations_roles_on_automation_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_automations_roles_on_automation_id ON public.automations_roles USING btree (automation_id);


--
-- Name: index_automations_roles_on_role_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_automations_roles_on_role_id ON public.automations_roles USING btree (role_id);


--
-- Name: index_automations_statuses_on_automation_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_automations_statuses_on_automation_id ON public.automations_statuses USING btree (automation_id);


--
-- Name: index_automations_statuses_on_status_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_automations_statuses_on_status_id ON public.automations_statuses USING btree (status_id);


--
-- Name: index_automations_types_on_automation_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_automations_types_on_automation_id ON public.automations_types USING btree (automation_id);


--
-- Name: index_automations_types_on_type_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_automations_types_on_type_id ON public.automations_types USING btree (type_id);


--
-- Name: index_backlog_buckets_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_backlog_buckets_on_project_id ON public.backlog_buckets USING btree (project_id);


--
-- Name: index_bcf_comments_on_issue_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_bcf_comments_on_issue_id ON public.bcf_comments USING btree (issue_id);


--
-- Name: index_bcf_comments_on_journal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_bcf_comments_on_journal_id ON public.bcf_comments USING btree (journal_id);


--
-- Name: index_bcf_comments_on_uuid; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_bcf_comments_on_uuid ON public.bcf_comments USING btree (uuid);


--
-- Name: index_bcf_comments_on_uuid_and_issue_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_bcf_comments_on_uuid_and_issue_id ON public.bcf_comments USING btree (uuid, issue_id);


--
-- Name: index_bcf_comments_on_viewpoint_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_bcf_comments_on_viewpoint_id ON public.bcf_comments USING btree (viewpoint_id);


--
-- Name: index_bcf_issues_on_uuid; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_bcf_issues_on_uuid ON public.bcf_issues USING btree (uuid);


--
-- Name: index_bcf_issues_on_work_package_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_bcf_issues_on_work_package_id ON public.bcf_issues USING btree (work_package_id);


--
-- Name: index_bcf_viewpoints_on_issue_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_bcf_viewpoints_on_issue_id ON public.bcf_viewpoints USING btree (issue_id);


--
-- Name: index_bcf_viewpoints_on_uuid; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_bcf_viewpoints_on_uuid ON public.bcf_viewpoints USING btree (uuid);


--
-- Name: index_bcf_viewpoints_on_uuid_and_issue_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_bcf_viewpoints_on_uuid_and_issue_id ON public.bcf_viewpoints USING btree (uuid, issue_id);


--
-- Name: index_budgets_on_project_id_and_updated_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_budgets_on_project_id_and_updated_at ON public.budgets USING btree (project_id, updated_at);


--
-- Name: index_calculated_value_errors_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_calculated_value_errors_on_custom_field_id ON public.calculated_value_errors USING btree (custom_field_id);


--
-- Name: index_calculated_value_errors_on_customized; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_calculated_value_errors_on_customized ON public.calculated_value_errors USING btree (customized_type, customized_id);


--
-- Name: index_categories_on_assigned_to_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_categories_on_assigned_to_id ON public.categories USING btree (assigned_to_id);


--
-- Name: index_categories_on_project_id_and_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_categories_on_project_id_and_LOWER_name" ON public.categories USING btree (project_id, lower((name)::text));


--
-- Name: index_cf_ps_value_restrictions_on_custom_field; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_cf_ps_value_restrictions_on_custom_field ON public.custom_field_project_specific_value_restrictions USING btree (custom_field_id);


--
-- Name: index_cf_ps_value_restrictions_on_item; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_cf_ps_value_restrictions_on_item ON public.custom_field_project_specific_value_restrictions USING btree (hierarchy_item_id);


--
-- Name: index_cf_ps_value_restrictions_on_option; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_cf_ps_value_restrictions_on_option ON public.custom_field_project_specific_value_restrictions USING btree (custom_option_id);


--
-- Name: index_cf_ps_value_restrictions_on_project_and_item; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_cf_ps_value_restrictions_on_project_and_item ON public.custom_field_project_specific_value_restrictions USING btree (project_id, hierarchy_item_id);


--
-- Name: index_cf_ps_value_restrictions_on_project_and_option; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_cf_ps_value_restrictions_on_project_and_option ON public.custom_field_project_specific_value_restrictions USING btree (project_id, custom_option_id);


--
-- Name: index_changesets_on_committed_on; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_changesets_on_committed_on ON public.changesets USING btree (committed_on);


--
-- Name: index_changesets_on_repository_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_changesets_on_repository_id ON public.changesets USING btree (repository_id);


--
-- Name: index_changesets_on_repository_id_and_committed_on; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_changesets_on_repository_id_and_committed_on ON public.changesets USING btree (repository_id, committed_on);


--
-- Name: index_changesets_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_changesets_on_user_id ON public.changesets USING btree (user_id);


--
-- Name: index_comments_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_comments_on_author_id ON public.comments USING btree (author_id);


--
-- Name: index_comments_on_commented_id_and_commented_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_comments_on_commented_id_and_commented_type ON public.comments USING btree (commented_id, commented_type);


--
-- Name: index_cost_entries_on_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_cost_entries_on_entity ON public.cost_entries USING btree (entity_type, entity_id);


--
-- Name: index_cost_entries_on_entity_type_and_entity_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_cost_entries_on_entity_type_and_entity_id ON public.cost_entries USING btree (entity_type, entity_id);


--
-- Name: index_cost_entries_on_logged_by_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_cost_entries_on_logged_by_id ON public.cost_entries USING btree (logged_by_id);


--
-- Name: index_cost_types_on_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_cost_types_on_LOWER_name" ON public.cost_types USING btree (lower((name)::text));


--
-- Name: index_cost_types_projects_on_cost_type_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_cost_types_projects_on_cost_type_id ON public.cost_types_projects USING btree (cost_type_id);


--
-- Name: index_cost_types_projects_on_cost_type_id_and_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_cost_types_projects_on_cost_type_id_and_project_id ON public.cost_types_projects USING btree (cost_type_id, project_id);


--
-- Name: index_cost_types_projects_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_cost_types_projects_on_project_id ON public.cost_types_projects USING btree (project_id);


--
-- Name: index_custom_comment_journals_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_comment_journals_on_custom_field_id ON public.custom_comment_journals USING btree (custom_field_id);


--
-- Name: index_custom_comment_journals_on_journal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_comment_journals_on_journal_id ON public.custom_comment_journals USING btree (journal_id);


--
-- Name: index_custom_comments_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_comments_on_custom_field_id ON public.custom_comments USING btree (custom_field_id);


--
-- Name: index_custom_comments_on_customized; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_comments_on_customized ON public.custom_comments USING btree (customized_type, customized_id);


--
-- Name: index_custom_fields_on_custom_field_section_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_fields_on_custom_field_section_id ON public.custom_fields USING btree (custom_field_section_id);


--
-- Name: index_custom_fields_on_formula; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_fields_on_formula ON public.custom_fields USING gin (formula);


--
-- Name: index_custom_fields_on_id_and_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_fields_on_id_and_type ON public.custom_fields USING btree (id, type);


--
-- Name: index_custom_fields_on_type_and_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_custom_fields_on_type_and_LOWER_name" ON public.custom_fields USING btree (type, lower((name)::text));


--
-- Name: index_custom_fields_on_type_and_semantic_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_custom_fields_on_type_and_semantic_key ON public.custom_fields USING btree (type, semantic_key) WHERE (semantic_key IS NOT NULL);


--
-- Name: index_custom_fields_projects_on_custom_field_id_and_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_custom_fields_projects_on_custom_field_id_and_project_id ON public.custom_fields_projects USING btree (custom_field_id, project_id);


--
-- Name: index_custom_fields_roles_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_custom_fields_roles_on_custom_field_id ON public.custom_fields_roles USING btree (custom_field_id);


--
-- Name: index_custom_fields_roles_on_role_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_fields_roles_on_role_id ON public.custom_fields_roles USING btree (role_id);


--
-- Name: index_custom_options_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_options_on_custom_field_id ON public.custom_options USING btree (custom_field_id);


--
-- Name: index_custom_options_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_options_on_project_id ON public.custom_options USING btree (project_id);


--
-- Name: index_custom_options_on_value; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_options_on_value ON public.custom_options USING gin (value gin_trgm_ops);


--
-- Name: index_custom_values_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_values_on_custom_field_id ON public.custom_values USING btree (custom_field_id);


--
-- Name: index_custom_values_on_value; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_custom_values_on_value ON public.custom_values USING gin (value gin_trgm_ops);


--
-- Name: index_customizable_journals_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_customizable_journals_on_custom_field_id ON public.customizable_journals USING btree (custom_field_id);


--
-- Name: index_customizable_journals_on_journal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_customizable_journals_on_journal_id ON public.customizable_journals USING btree (journal_id);


--
-- Name: index_deploy_status_checks_on_deploy_target_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_deploy_status_checks_on_deploy_target_id ON public.deploy_status_checks USING btree (deploy_target_id);


--
-- Name: index_deploy_status_checks_on_github_pull_request_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_deploy_status_checks_on_github_pull_request_id ON public.deploy_status_checks USING btree (github_pull_request_id);


--
-- Name: index_deploy_targets_on_host; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_deploy_targets_on_host ON public.deploy_targets USING btree (host);


--
-- Name: index_design_colors_on_variable; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_design_colors_on_variable ON public.design_colors USING btree (variable);


--
-- Name: index_document_types_on_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_document_types_on_name ON public.document_types USING btree (name);


--
-- Name: index_documents_on_category_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_documents_on_category_id ON public.documents USING btree (category_id);


--
-- Name: index_documents_on_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_documents_on_created_at ON public.documents USING btree (created_at);


--
-- Name: index_documents_on_type_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_documents_on_type_id ON public.documents USING btree (type_id);


--
-- Name: index_emoji_reactions_on_reactable; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_emoji_reactions_on_reactable ON public.emoji_reactions USING btree (reactable_type, reactable_id);


--
-- Name: index_emoji_reactions_on_reaction; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_emoji_reactions_on_reaction ON public.emoji_reactions USING btree (reaction);


--
-- Name: index_emoji_reactions_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_emoji_reactions_on_user_id ON public.emoji_reactions USING btree (user_id);


--
-- Name: index_emoji_reactions_unique_agenda_item_vote; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_emoji_reactions_unique_agenda_item_vote ON public.emoji_reactions USING btree (user_id, reactable_type, reactable_id) WHERE ((reactable_type)::text = 'MeetingAgendaItem'::text);


--
-- Name: index_emoji_reactions_uniqueness; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_emoji_reactions_uniqueness ON public.emoji_reactions USING btree (user_id, reactable_type, reactable_id, reaction);


--
-- Name: index_enabled_modules_on_name; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_enabled_modules_on_name ON public.enabled_modules USING btree (name);


--
-- Name: index_enterprise_tokens_on_valid_from_and_valid_until; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_enterprise_tokens_on_valid_from_and_valid_until ON public.enterprise_tokens USING btree (valid_from, valid_until);


--
-- Name: index_enumerations_on_color_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_enumerations_on_color_id ON public.enumerations USING btree (color_id);


--
-- Name: index_enumerations_on_id_and_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_enumerations_on_id_and_type ON public.enumerations USING btree (id, type);


--
-- Name: index_enumerations_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_enumerations_on_project_id ON public.enumerations USING btree (project_id);


--
-- Name: index_enumerations_on_type_project_id_and_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_enumerations_on_type_project_id_and_LOWER_name" ON public.enumerations USING btree (type, project_id, lower((name)::text)) NULLS NOT DISTINCT;


--
-- Name: index_export_settings_on_query_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_export_settings_on_query_id ON public.export_settings USING btree (query_id);


--
-- Name: index_export_settings_on_query_id_and_format; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_export_settings_on_query_id_and_format ON public.export_settings USING btree (query_id, format);


--
-- Name: index_favorites_on_favored; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_favorites_on_favored ON public.favorites USING btree (favorited_type, favorited_id);


--
-- Name: index_favorites_on_favorited_type_and_favorited_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_favorites_on_favorited_type_and_favorited_id ON public.favorites USING btree (favorited_type, favorited_id);


--
-- Name: index_favorites_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_favorites_on_user_id ON public.favorites USING btree (user_id);


--
-- Name: index_favorites_on_user_id_and_favorited_type_and_favorited_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_favorites_on_user_id_and_favorited_type_and_favorited_id ON public.favorites USING btree (user_id, favorited_type, favorited_id);


--
-- Name: index_file_links_on_container_id_and_container_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_file_links_on_container_id_and_container_type ON public.file_links USING btree (container_id, container_type);


--
-- Name: index_file_links_on_creator_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_file_links_on_creator_id ON public.file_links USING btree (creator_id);


--
-- Name: index_file_links_on_origin_id_and_storage_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_file_links_on_origin_id_and_storage_id ON public.file_links USING btree (origin_id, storage_id);


--
-- Name: index_file_links_on_storage_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_file_links_on_storage_id ON public.file_links USING btree (storage_id);


--
-- Name: index_form_configuration_attributes_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_form_configuration_attributes_on_custom_field_id ON public.form_configuration_attributes USING btree (custom_field_id);


--
-- Name: index_form_configuration_attributes_on_form_configuration_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_form_configuration_attributes_on_form_configuration_id ON public.form_configuration_attributes USING btree (form_configuration_id);


--
-- Name: index_form_configuration_attributes_on_owner_and_custom_field; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_form_configuration_attributes_on_owner_and_custom_field ON public.form_configuration_attributes USING btree (form_configuration_id, custom_field_id) WHERE (custom_field_id IS NOT NULL);


--
-- Name: index_form_configuration_attributes_on_owner_and_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_form_configuration_attributes_on_owner_and_key ON public.form_configuration_attributes USING btree (form_configuration_id, attribute_key) WHERE (attribute_key IS NOT NULL);


--
-- Name: index_form_configuration_groups_on_form_configuration_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_form_configuration_groups_on_form_configuration_id ON public.form_configuration_groups USING btree (form_configuration_id);


--
-- Name: index_form_configuration_groups_on_id_and_owner; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_form_configuration_groups_on_id_and_owner ON public.form_configuration_groups USING btree (id, form_configuration_id);


--
-- Name: index_form_configuration_groups_on_owner_and_default_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_form_configuration_groups_on_owner_and_default_key ON public.form_configuration_groups USING btree (form_configuration_id, default_key) WHERE (default_key IS NOT NULL);


--
-- Name: index_form_configuration_groups_on_query_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_form_configuration_groups_on_query_id ON public.form_configuration_groups USING btree (query_id);


--
-- Name: index_form_configurations_on_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_form_configurations_on_LOWER_name" ON public.form_configurations USING btree (lower((name)::text));


--
-- Name: index_form_configurations_on_default; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_form_configurations_on_default ON public.form_configurations USING btree (is_default) WHERE is_default;


--
-- Name: index_forums_on_last_message_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_forums_on_last_message_id ON public.forums USING btree (last_message_id);


--
-- Name: index_friendly_id_slugs_on_slug_and_sluggable_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_friendly_id_slugs_on_slug_and_sluggable_type ON public.friendly_id_slugs USING btree (slug, sluggable_type);


--
-- Name: index_friendly_id_slugs_on_slug_and_sluggable_type_and_scope; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_friendly_id_slugs_on_slug_and_sluggable_type_and_scope ON public.friendly_id_slugs USING btree (slug, sluggable_type, scope) NULLS NOT DISTINCT;


--
-- Name: index_friendly_id_slugs_on_sluggable_type_and_sluggable_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_friendly_id_slugs_on_sluggable_type_and_sluggable_id ON public.friendly_id_slugs USING btree (sluggable_type, sluggable_id);


--
-- Name: index_github_check_runs_on_github_pull_request_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_github_check_runs_on_github_pull_request_id ON public.github_check_runs USING btree (github_pull_request_id);


--
-- Name: index_github_pull_requests_on_github_html_url; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_github_pull_requests_on_github_html_url ON public.github_pull_requests USING btree (github_html_url);


--
-- Name: index_github_pull_requests_on_github_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_github_pull_requests_on_github_user_id ON public.github_pull_requests USING btree (github_user_id);


--
-- Name: index_github_pull_requests_on_merged_by_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_github_pull_requests_on_merged_by_id ON public.github_pull_requests USING btree (merged_by_id);


--
-- Name: index_gitlab_branches_on_gitlab_project_id_and_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_gitlab_branches_on_gitlab_project_id_and_name ON public.gitlab_branches USING btree (gitlab_project_id, name);


--
-- Name: index_gitlab_branches_on_gitlab_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_gitlab_branches_on_gitlab_user_id ON public.gitlab_branches USING btree (gitlab_user_id);


--
-- Name: index_gitlab_branches_on_work_package_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_gitlab_branches_on_work_package_id ON public.gitlab_branches USING btree (work_package_id);


--
-- Name: index_gitlab_commits_on_gitlab_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_gitlab_commits_on_gitlab_user_id ON public.gitlab_commits USING btree (gitlab_user_id);


--
-- Name: index_gitlab_commits_work_packages_on_gitlab_commit_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_gitlab_commits_work_packages_on_gitlab_commit_id ON public.gitlab_commits_work_packages USING btree (gitlab_commit_id);


--
-- Name: index_gitlab_commits_work_packages_on_work_package_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_gitlab_commits_work_packages_on_work_package_id ON public.gitlab_commits_work_packages USING btree (work_package_id);


--
-- Name: index_gitlab_issues_on_gitlab_html_url; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_gitlab_issues_on_gitlab_html_url ON public.gitlab_issues USING btree (gitlab_html_url);


--
-- Name: index_gitlab_issues_on_gitlab_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_gitlab_issues_on_gitlab_user_id ON public.gitlab_issues USING btree (gitlab_user_id);


--
-- Name: index_gitlab_merge_requests_on_gitlab_html_url; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_gitlab_merge_requests_on_gitlab_html_url ON public.gitlab_merge_requests USING btree (gitlab_html_url);


--
-- Name: index_gitlab_merge_requests_on_gitlab_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_gitlab_merge_requests_on_gitlab_user_id ON public.gitlab_merge_requests USING btree (gitlab_user_id);


--
-- Name: index_gitlab_merge_requests_on_merged_by_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_gitlab_merge_requests_on_merged_by_id ON public.gitlab_merge_requests USING btree (merged_by_id);


--
-- Name: index_gitlab_pipelines_on_gitlab_merge_request_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_gitlab_pipelines_on_gitlab_merge_request_id ON public.gitlab_pipelines USING btree (gitlab_merge_request_id);


--
-- Name: index_good_job_executions_on_active_job_id_and_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_job_executions_on_active_job_id_and_created_at ON public.good_job_executions USING btree (active_job_id, created_at);


--
-- Name: index_good_job_executions_on_process_id_and_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_job_executions_on_process_id_and_created_at ON public.good_job_executions USING btree (process_id, created_at);


--
-- Name: index_good_job_jobs_for_candidate_lookup; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_job_jobs_for_candidate_lookup ON public.good_jobs USING btree (priority, created_at) WHERE (finished_at IS NULL);


--
-- Name: index_good_job_settings_on_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_good_job_settings_on_key ON public.good_job_settings USING btree (key);


--
-- Name: index_good_jobs_for_candidate_dequeue_unlocked; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_for_candidate_dequeue_unlocked ON public.good_jobs USING btree (priority, scheduled_at, id) WHERE ((finished_at IS NULL) AND (locked_by_id IS NULL));


--
-- Name: index_good_jobs_jobs_on_finished_at_only; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_jobs_on_finished_at_only ON public.good_jobs USING btree (finished_at) WHERE (finished_at IS NOT NULL);


--
-- Name: index_good_jobs_jobs_on_priority_created_at_when_unfinished; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_jobs_on_priority_created_at_when_unfinished ON public.good_jobs USING btree (priority DESC NULLS LAST, created_at) WHERE (finished_at IS NULL);


--
-- Name: index_good_jobs_on_active_job_id_and_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_active_job_id_and_created_at ON public.good_jobs USING btree (active_job_id, created_at);


--
-- Name: index_good_jobs_on_batch_callback_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_batch_callback_id ON public.good_jobs USING btree (batch_callback_id) WHERE (batch_callback_id IS NOT NULL);


--
-- Name: index_good_jobs_on_batch_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_batch_id ON public.good_jobs USING btree (batch_id) WHERE (batch_id IS NOT NULL);


--
-- Name: index_good_jobs_on_concurrency_key_and_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_concurrency_key_and_created_at ON public.good_jobs USING btree (concurrency_key, created_at);


--
-- Name: index_good_jobs_on_concurrency_key_when_unfinished; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_concurrency_key_when_unfinished ON public.good_jobs USING btree (concurrency_key) WHERE (finished_at IS NULL);


--
-- Name: index_good_jobs_on_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_created_at ON public.good_jobs USING btree (created_at);


--
-- Name: index_good_jobs_on_cron_key_and_created_at_cond; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_cron_key_and_created_at_cond ON public.good_jobs USING btree (cron_key, created_at) WHERE (cron_key IS NOT NULL);


--
-- Name: index_good_jobs_on_cron_key_and_cron_at_cond; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_good_jobs_on_cron_key_and_cron_at_cond ON public.good_jobs USING btree (cron_key, cron_at) WHERE (cron_key IS NOT NULL);


--
-- Name: index_good_jobs_on_discarded; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_discarded ON public.good_jobs USING btree (finished_at DESC) WHERE ((finished_at IS NOT NULL) AND (error IS NOT NULL));


--
-- Name: index_good_jobs_on_job_class; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_job_class ON public.good_jobs USING btree (job_class);


--
-- Name: index_good_jobs_on_labels; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_labels ON public.good_jobs USING gin (labels) WHERE (labels IS NOT NULL);


--
-- Name: index_good_jobs_on_locked_by_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_locked_by_id ON public.good_jobs USING btree (locked_by_id) WHERE (locked_by_id IS NOT NULL);


--
-- Name: index_good_jobs_on_priority_scheduled_at_unfinished; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_priority_scheduled_at_unfinished ON public.good_jobs USING btree (priority, scheduled_at, id) WHERE (finished_at IS NULL);


--
-- Name: index_good_jobs_on_priority_scheduled_at_unfinished_unlocked; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_priority_scheduled_at_unfinished_unlocked ON public.good_jobs USING btree (priority, scheduled_at) WHERE ((finished_at IS NULL) AND (locked_by_id IS NULL));


--
-- Name: index_good_jobs_on_queue_name; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_queue_name ON public.good_jobs USING btree (queue_name);


--
-- Name: index_good_jobs_on_queue_name_and_scheduled_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_queue_name_and_scheduled_at ON public.good_jobs USING btree (queue_name, scheduled_at) WHERE (finished_at IS NULL);


--
-- Name: index_good_jobs_on_queue_name_priority_scheduled_at_unfinished; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_queue_name_priority_scheduled_at_unfinished ON public.good_jobs USING btree (queue_name, scheduled_at, id) WHERE (finished_at IS NULL);


--
-- Name: index_good_jobs_on_scheduled_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_scheduled_at ON public.good_jobs USING btree (scheduled_at) WHERE (finished_at IS NULL);


--
-- Name: index_good_jobs_on_scheduled_at_and_queue_name; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_scheduled_at_and_queue_name ON public.good_jobs USING btree (scheduled_at, queue_name);


--
-- Name: index_good_jobs_on_unfinished_or_errored; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_good_jobs_on_unfinished_or_errored ON public.good_jobs USING btree (id) WHERE ((finished_at IS NULL) OR (error IS NOT NULL));


--
-- Name: index_grid_widgets_on_grid_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_grid_widgets_on_grid_id ON public.grid_widgets USING btree (grid_id);


--
-- Name: index_grids_on_project_and_linked; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_grids_on_project_and_linked ON public.grids USING btree (project_id, linked_type, linked_id);


--
-- Name: index_grids_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_grids_on_project_id ON public.grids USING btree (project_id);


--
-- Name: index_grids_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_grids_on_user_id ON public.grids USING btree (user_id);


--
-- Name: index_group_details_on_organizational_unit; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_group_details_on_organizational_unit ON public.group_details USING btree (organizational_unit);


--
-- Name: index_group_details_on_parent_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_group_details_on_parent_id ON public.group_details USING btree (parent_id);


--
-- Name: index_group_details_on_principal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_group_details_on_principal_id ON public.group_details USING btree (principal_id);


--
-- Name: index_group_users_on_user_id_and_group_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_group_users_on_user_id_and_group_id ON public.group_users USING btree (user_id, group_id);


--
-- Name: index_hierarchical_items_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_hierarchical_items_on_custom_field_id ON public.hierarchical_items USING btree (custom_field_id);


--
-- Name: index_hierarchical_items_on_position_cache; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_hierarchical_items_on_position_cache ON public.hierarchical_items USING btree (position_cache);


--
-- Name: index_hierarchical_items_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_hierarchical_items_on_project_id ON public.hierarchical_items USING btree (project_id);


--
-- Name: index_ical_token_query_assignments_on_ical_token_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ical_token_query_assignments_on_ical_token_id ON public.ical_token_query_assignments USING btree (ical_token_id);


--
-- Name: index_ical_token_query_assignments_on_query_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ical_token_query_assignments_on_query_id ON public.ical_token_query_assignments USING btree (query_id);


--
-- Name: index_ifc_models_on_is_default; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ifc_models_on_is_default ON public.ifc_models USING btree (is_default);


--
-- Name: index_ifc_models_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ifc_models_on_project_id ON public.ifc_models USING btree (project_id);


--
-- Name: index_ifc_models_on_uploader_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ifc_models_on_uploader_id ON public.ifc_models USING btree (uploader_id);


--
-- Name: index_itsm_email_messages_on_journal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_itsm_email_messages_on_journal_id ON public.itsm_email_messages USING btree (journal_id);


--
-- Name: index_itsm_email_messages_on_message_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_itsm_email_messages_on_message_id ON public.itsm_email_messages USING btree (message_id);


--
-- Name: index_itsm_email_messages_on_thread_id_and_message_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_itsm_email_messages_on_thread_id_and_message_id ON public.itsm_email_messages USING btree (thread_id, message_id);


--
-- Name: index_itsm_email_threads_on_work_package_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_itsm_email_threads_on_work_package_id ON public.itsm_email_threads USING btree (work_package_id);


--
-- Name: index_itsm_incoming_mailboxes_on_email; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_itsm_incoming_mailboxes_on_email ON public.itsm_incoming_mailboxes USING btree (email);


--
-- Name: index_itsm_incoming_mailboxes_on_mail_server_and_username; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_itsm_incoming_mailboxes_on_mail_server_and_username ON public.itsm_incoming_mailboxes USING btree (mail_server_id, lower((username)::text));


--
-- Name: index_itsm_incoming_mailboxes_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_itsm_incoming_mailboxes_on_project_id ON public.itsm_incoming_mailboxes USING btree (project_id);


--
-- Name: index_itsm_mail_servers_on_protocol_and_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_itsm_mail_servers_on_protocol_and_name ON public.itsm_mail_servers USING btree (protocol, lower((name)::text));


--
-- Name: index_itsm_outgoing_mailboxes_on_mail_server_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_itsm_outgoing_mailboxes_on_mail_server_id ON public.itsm_outgoing_mailboxes USING btree (mail_server_id);


--
-- Name: index_itsm_outgoing_mailboxes_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_itsm_outgoing_mailboxes_on_project_id ON public.itsm_outgoing_mailboxes USING btree (project_id);


--
-- Name: index_jira_attachments_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_attachments_on_jira_import_id ON public.jira_attachments USING btree (jira_import_id);


--
-- Name: index_jira_attachments_on_jira_issue_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_attachments_on_jira_issue_id ON public.jira_attachments USING btree (jira_issue_id);


--
-- Name: index_jira_attachments_on_jira_issue_id_and_origin_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_attachments_on_jira_issue_id_and_origin_id ON public.jira_attachments USING btree (jira_issue_id, origin_id);


--
-- Name: index_jira_attachments_on_jira_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_attachments_on_jira_project_id ON public.jira_attachments USING btree (jira_project_id);


--
-- Name: index_jira_boards_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_boards_on_jira_import_id ON public.jira_boards USING btree (jira_import_id);


--
-- Name: index_jira_boards_on_jira_import_id_and_origin_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_boards_on_jira_import_id_and_origin_id ON public.jira_boards USING btree (jira_import_id, origin_id);


--
-- Name: index_jira_fields_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_fields_on_jira_import_id ON public.jira_fields USING btree (jira_import_id);


--
-- Name: index_jira_fields_on_jira_import_id_and_origin_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_fields_on_jira_import_id_and_origin_id ON public.jira_fields USING btree (jira_import_id, origin_id);


--
-- Name: index_jira_import_job_cursors_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_import_job_cursors_on_jira_import_id ON public.jira_import_job_cursors USING btree (jira_import_id);


--
-- Name: index_jira_import_transitions_parent_most_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_import_transitions_parent_most_recent ON public.jira_import_transitions USING btree (jira_import_id, most_recent) WHERE most_recent;


--
-- Name: index_jira_import_transitions_parent_sort; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_import_transitions_parent_sort ON public.jira_import_transitions USING btree (jira_import_id, sort_key);


--
-- Name: index_jira_imports_on_jira_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_imports_on_jira_id ON public.jira_imports USING btree (jira_id);


--
-- Name: index_jira_issue_types_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_issue_types_on_jira_import_id ON public.jira_issue_types USING btree (jira_import_id);


--
-- Name: index_jira_issue_types_on_jira_import_id_and_origin_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_issue_types_on_jira_import_id_and_origin_id ON public.jira_issue_types USING btree (jira_import_id, origin_id);


--
-- Name: index_jira_issues_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_issues_on_jira_import_id ON public.jira_issues USING btree (jira_import_id);


--
-- Name: index_jira_issues_on_jira_import_id_and_origin_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_issues_on_jira_import_id_and_origin_id ON public.jira_issues USING btree (jira_import_id, origin_id);


--
-- Name: index_jira_issues_on_jira_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_issues_on_jira_project_id ON public.jira_issues USING btree (jira_project_id);


--
-- Name: index_jira_open_project_references_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_open_project_references_on_jira_import_id ON public.jira_open_project_references USING btree (jira_import_id);


--
-- Name: index_jira_priorities_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_priorities_on_jira_import_id ON public.jira_priorities USING btree (jira_import_id);


--
-- Name: index_jira_priorities_on_jira_import_id_and_origin_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_priorities_on_jira_import_id_and_origin_id ON public.jira_priorities USING btree (jira_import_id, origin_id);


--
-- Name: index_jira_projects_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_projects_on_jira_import_id ON public.jira_projects USING btree (jira_import_id);


--
-- Name: index_jira_projects_on_jira_import_id_and_origin_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_projects_on_jira_import_id_and_origin_id ON public.jira_projects USING btree (jira_import_id, origin_id);


--
-- Name: index_jira_sprints_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_sprints_on_jira_import_id ON public.jira_sprints USING btree (jira_import_id);


--
-- Name: index_jira_sprints_on_jira_import_id_and_origin_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_sprints_on_jira_import_id_and_origin_id ON public.jira_sprints USING btree (jira_import_id, origin_id);


--
-- Name: index_jira_statuses_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_statuses_on_jira_import_id ON public.jira_statuses USING btree (jira_import_id);


--
-- Name: index_jira_statuses_on_jira_import_id_and_origin_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_statuses_on_jira_import_id_and_origin_id ON public.jira_statuses USING btree (jira_import_id, origin_id);


--
-- Name: index_jira_users_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_users_on_jira_import_id ON public.jira_users USING btree (jira_import_id);


--
-- Name: index_jira_users_on_jira_import_id_and_origin_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_users_on_jira_import_id_and_origin_id ON public.jira_users USING btree (jira_import_id, origin_id);


--
-- Name: index_jira_versions_on_jira_import_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_versions_on_jira_import_id ON public.jira_versions USING btree (jira_import_id);


--
-- Name: index_jira_versions_on_jira_import_id_and_origin_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_jira_versions_on_jira_import_id_and_origin_id ON public.jira_versions USING btree (jira_import_id, origin_id);


--
-- Name: index_jira_versions_on_jira_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_jira_versions_on_jira_project_id ON public.jira_versions USING btree (jira_project_id);


--
-- Name: index_job_statuses_on_job_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_job_statuses_on_job_id ON public.job_statuses USING btree (job_id);


--
-- Name: index_job_statuses_on_reference_type_and_reference_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_job_statuses_on_reference_type_and_reference_id ON public.job_statuses USING btree (reference_type, reference_id);


--
-- Name: index_job_statuses_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_job_statuses_on_user_id ON public.job_statuses USING btree (user_id);


--
-- Name: index_journals_on_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_journals_on_created_at ON public.journals USING btree (created_at);


--
-- Name: index_journals_on_data_id_and_data_type; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_journals_on_data_id_and_data_type ON public.journals USING btree (data_id, data_type);


--
-- Name: index_journals_on_journable_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_journals_on_journable_id ON public.journals USING btree (journable_id);


--
-- Name: index_journals_on_journable_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_journals_on_journable_type ON public.journals USING btree (journable_type);


--
-- Name: index_journals_on_journable_type_and_journable_id_and_version; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_journals_on_journable_type_and_journable_id_and_version ON public.journals USING btree (journable_type, journable_id, version);


--
-- Name: index_journals_on_notes; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_journals_on_notes ON public.journals USING gin (notes gin_trgm_ops);


--
-- Name: index_journals_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_journals_on_user_id ON public.journals USING btree (user_id);


--
-- Name: index_label_journals_on_journal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_label_journals_on_journal_id ON public.label_journals USING btree (journal_id);


--
-- Name: index_label_journals_on_label_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_label_journals_on_label_id ON public.label_journals USING btree (label_id);


--
-- Name: index_labelings_on_label_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_labelings_on_label_id ON public.labelings USING btree (label_id);


--
-- Name: index_labelings_on_labelable_and_label; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_labelings_on_labelable_and_label ON public.labelings USING btree (labelable_type, labelable_id, label_id);


--
-- Name: index_labels_on_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_labels_on_LOWER_name" ON public.labels USING btree (lower((name)::text));


--
-- Name: index_labels_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_labels_on_author_id ON public.labels USING btree (author_id);


--
-- Name: index_last_project_folders_on_project_storage_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_last_project_folders_on_project_storage_id ON public.last_project_folders USING btree (project_storage_id);


--
-- Name: index_last_project_folders_on_project_storage_id_and_mode; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_last_project_folders_on_project_storage_id_and_mode ON public.last_project_folders USING btree (project_storage_id, mode);


--
-- Name: index_ldap_auth_sources_on_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_ldap_auth_sources_on_LOWER_name" ON public.ldap_auth_sources USING btree (lower((name)::text));


--
-- Name: index_ldap_departments_memberships_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ldap_departments_memberships_on_user_id ON public.ldap_departments_memberships USING btree (user_id);


--
-- Name: index_ldap_departments_synchronized_departments_on_dn; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_ldap_departments_synchronized_departments_on_dn ON public.ldap_departments_synchronized_departments USING btree (dn);


--
-- Name: index_ldap_departments_synchronized_departments_on_group_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ldap_departments_synchronized_departments_on_group_id ON public.ldap_departments_synchronized_departments USING btree (group_id);


--
-- Name: index_ldap_groups_memberships_on_group_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ldap_groups_memberships_on_group_id ON public.ldap_groups_memberships USING btree (group_id);


--
-- Name: index_ldap_groups_memberships_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ldap_groups_memberships_on_user_id ON public.ldap_groups_memberships USING btree (user_id);


--
-- Name: index_ldap_groups_memberships_on_user_id_and_group_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_ldap_groups_memberships_on_user_id_and_group_id ON public.ldap_groups_memberships USING btree (user_id, group_id);


--
-- Name: index_ldap_groups_synchronized_filters_on_ldap_auth_source_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ldap_groups_synchronized_filters_on_ldap_auth_source_id ON public.ldap_groups_synchronized_filters USING btree (ldap_auth_source_id);


--
-- Name: index_ldap_groups_synchronized_groups_on_filter_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ldap_groups_synchronized_groups_on_filter_id ON public.ldap_groups_synchronized_groups USING btree (filter_id);


--
-- Name: index_ldap_groups_synchronized_groups_on_group_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ldap_groups_synchronized_groups_on_group_id ON public.ldap_groups_synchronized_groups USING btree (group_id);


--
-- Name: index_ldap_groups_synchronized_groups_on_ldap_auth_source_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ldap_groups_synchronized_groups_on_ldap_auth_source_id ON public.ldap_groups_synchronized_groups USING btree (ldap_auth_source_id);


--
-- Name: index_llm_capability_verdicts_on_connection_model_capability; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_llm_capability_verdicts_on_connection_model_capability ON public.llm_capability_verdicts USING btree (llm_connection_id, model_id, capability);


--
-- Name: index_llm_capability_verdicts_on_llm_connection_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_llm_capability_verdicts_on_llm_connection_id ON public.llm_capability_verdicts USING btree (llm_connection_id);


--
-- Name: index_llm_connections_on_active; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_llm_connections_on_active ON public.llm_connections USING btree (active) WHERE active;


--
-- Name: index_llm_connections_on_default_chat_model_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_llm_connections_on_default_chat_model_id ON public.llm_connections USING btree (default_chat_model_id);


--
-- Name: index_llm_connections_on_default_embedding_model_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_llm_connections_on_default_embedding_model_id ON public.llm_connections USING btree (default_embedding_model_id);


--
-- Name: index_llm_connections_on_identifier; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_llm_connections_on_identifier ON public.llm_connections USING btree (identifier);


--
-- Name: index_llm_feature_bindings_on_llm_connection_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_llm_feature_bindings_on_llm_connection_id ON public.llm_feature_bindings USING btree (llm_connection_id);


--
-- Name: index_llm_feature_bindings_on_semantic_index_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_llm_feature_bindings_on_semantic_index_id ON public.llm_feature_bindings USING btree (semantic_index_id);


--
-- Name: index_llm_models_on_llm_connection_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_llm_models_on_llm_connection_id ON public.llm_models USING btree (llm_connection_id);


--
-- Name: index_llm_models_on_llm_connection_id_and_external_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_llm_models_on_llm_connection_id_and_external_id ON public.llm_models USING btree (llm_connection_id, external_id);


--
-- Name: index_meeting_agenda_item_journals_on_presenter_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_agenda_item_journals_on_presenter_id ON public.meeting_agenda_item_journals USING btree (presenter_id);


--
-- Name: index_meeting_agenda_items_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_agenda_items_on_author_id ON public.meeting_agenda_items USING btree (author_id);


--
-- Name: index_meeting_agenda_items_on_meeting_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_agenda_items_on_meeting_id ON public.meeting_agenda_items USING btree (meeting_id);


--
-- Name: index_meeting_agenda_items_on_meeting_section_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_agenda_items_on_meeting_section_id ON public.meeting_agenda_items USING btree (meeting_section_id);


--
-- Name: index_meeting_agenda_items_on_notes; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_agenda_items_on_notes ON public.meeting_agenda_items USING gin (notes gin_trgm_ops);


--
-- Name: index_meeting_agenda_items_on_presenter_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_agenda_items_on_presenter_id ON public.meeting_agenda_items USING btree (presenter_id);


--
-- Name: index_meeting_agenda_items_on_work_package_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_agenda_items_on_work_package_id ON public.meeting_agenda_items USING btree (work_package_id);


--
-- Name: index_meeting_outcomes_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_outcomes_on_author_id ON public.meeting_outcomes USING btree (author_id);


--
-- Name: index_meeting_outcomes_on_meeting_agenda_item_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_outcomes_on_meeting_agenda_item_id ON public.meeting_outcomes USING btree (meeting_agenda_item_id);


--
-- Name: index_meeting_outcomes_on_work_package_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_outcomes_on_work_package_id ON public.meeting_outcomes USING btree (work_package_id);


--
-- Name: index_meeting_participants_on_meeting_id_and_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_meeting_participants_on_meeting_id_and_user_id ON public.meeting_participants USING btree (meeting_id, user_id);


--
-- Name: index_meeting_participants_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_participants_on_user_id ON public.meeting_participants USING btree (user_id);


--
-- Name: index_meeting_sections_on_meeting_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meeting_sections_on_meeting_id ON public.meeting_sections USING btree (meeting_id);


--
-- Name: index_meetings_on_project_id_and_updated_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meetings_on_project_id_and_updated_at ON public.meetings USING btree (project_id, updated_at);


--
-- Name: index_meetings_on_recurring_meeting_and_recurrence_start_time; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_meetings_on_recurring_meeting_and_recurrence_start_time ON public.meetings USING btree (recurring_meeting_id, recurrence_start_time) WHERE ((recurrence_start_time IS NOT NULL) AND (template = false));


--
-- Name: index_meetings_on_recurring_meeting_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_meetings_on_recurring_meeting_id ON public.meetings USING btree (recurring_meeting_id);


--
-- Name: index_meetings_on_uid; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_meetings_on_uid ON public.meetings USING btree (uid);


--
-- Name: index_member_roles_on_inherited_from; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_member_roles_on_inherited_from ON public.member_roles USING btree (inherited_from);


--
-- Name: index_member_roles_on_member_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_member_roles_on_member_id ON public.member_roles USING btree (member_id);


--
-- Name: index_member_roles_on_role_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_member_roles_on_role_id ON public.member_roles USING btree (role_id);


--
-- Name: index_members_on_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_members_on_entity ON public.members USING btree (entity_type, entity_id);


--
-- Name: index_members_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_members_on_project_id ON public.members USING btree (project_id);


--
-- Name: index_members_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_members_on_user_id ON public.members USING btree (user_id);


--
-- Name: index_members_on_user_id_and_project_with_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_members_on_user_id_and_project_with_entity ON public.members USING btree (user_id, project_id, entity_type, entity_id) WHERE ((entity_type IS NOT NULL) AND (entity_id IS NOT NULL));


--
-- Name: index_members_on_user_id_and_project_without_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_members_on_user_id_and_project_without_entity ON public.members USING btree (user_id, project_id) WHERE ((entity_type IS NULL) AND (entity_id IS NULL));


--
-- Name: index_menu_items_on_navigatable_id_and_title; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_menu_items_on_navigatable_id_and_title ON public.menu_items USING btree (navigatable_id, title);


--
-- Name: index_menu_items_on_parent_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_menu_items_on_parent_id ON public.menu_items USING btree (parent_id);


--
-- Name: index_message_work_packages_on_message_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_message_work_packages_on_message_id ON public.message_work_packages USING btree (message_id);


--
-- Name: index_message_work_packages_on_work_package_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_message_work_packages_on_work_package_id ON public.message_work_packages USING btree (work_package_id);


--
-- Name: index_messages_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_messages_on_author_id ON public.messages USING btree (author_id);


--
-- Name: index_messages_on_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_messages_on_created_at ON public.messages USING btree (created_at);


--
-- Name: index_messages_on_forum_id_and_updated_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_messages_on_forum_id_and_updated_at ON public.messages USING btree (forum_id, updated_at);


--
-- Name: index_messages_on_last_reply_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_messages_on_last_reply_id ON public.messages USING btree (last_reply_id);


--
-- Name: index_messages_on_parent_id_accepted_solution; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_messages_on_parent_id_accepted_solution ON public.messages USING btree (parent_id) WHERE accepted_solution;


--
-- Name: index_news_journals_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_news_journals_on_project_id ON public.news_journals USING btree (project_id);


--
-- Name: index_news_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_news_on_author_id ON public.news USING btree (author_id);


--
-- Name: index_news_on_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_news_on_created_at ON public.news USING btree (created_at);


--
-- Name: index_news_on_project_id_and_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_news_on_project_id_and_created_at ON public.news USING btree (project_id, created_at);


--
-- Name: index_non_working_days_on_date; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_non_working_days_on_date ON public.non_working_days USING btree (date);


--
-- Name: index_notification_settings_on_document_added; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_document_added ON public.notification_settings USING btree (document_added);


--
-- Name: index_notification_settings_on_forum_messages; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_forum_messages ON public.notification_settings USING btree (forum_messages);


--
-- Name: index_notification_settings_on_membership_added; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_membership_added ON public.notification_settings USING btree (membership_added);


--
-- Name: index_notification_settings_on_membership_updated; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_membership_updated ON public.notification_settings USING btree (membership_updated);


--
-- Name: index_notification_settings_on_news_added; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_news_added ON public.notification_settings USING btree (news_added);


--
-- Name: index_notification_settings_on_news_commented; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_news_commented ON public.notification_settings USING btree (news_commented);


--
-- Name: index_notification_settings_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_project_id ON public.notification_settings USING btree (project_id);


--
-- Name: index_notification_settings_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_user_id ON public.notification_settings USING btree (user_id);


--
-- Name: index_notification_settings_on_wiki_page_added; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_wiki_page_added ON public.notification_settings USING btree (wiki_page_added);


--
-- Name: index_notification_settings_on_wiki_page_updated; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_wiki_page_updated ON public.notification_settings USING btree (wiki_page_updated);


--
-- Name: index_notification_settings_on_work_package_commented; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_work_package_commented ON public.notification_settings USING btree (work_package_commented);


--
-- Name: index_notification_settings_on_work_package_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_work_package_created ON public.notification_settings USING btree (work_package_created);


--
-- Name: index_notification_settings_on_work_package_prioritized; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_work_package_prioritized ON public.notification_settings USING btree (work_package_prioritized);


--
-- Name: index_notification_settings_on_work_package_processed; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_work_package_processed ON public.notification_settings USING btree (work_package_processed);


--
-- Name: index_notification_settings_on_work_package_scheduled; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notification_settings_on_work_package_scheduled ON public.notification_settings USING btree (work_package_scheduled);


--
-- Name: index_notification_settings_unique_project; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_notification_settings_unique_project ON public.notification_settings USING btree (user_id, project_id) WHERE (project_id IS NOT NULL);


--
-- Name: index_notification_settings_unique_project_null; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_notification_settings_unique_project_null ON public.notification_settings USING btree (user_id) WHERE (project_id IS NULL);


--
-- Name: index_notifications_on_actor_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notifications_on_actor_id ON public.notifications USING btree (actor_id);


--
-- Name: index_notifications_on_journal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notifications_on_journal_id ON public.notifications USING btree (journal_id);


--
-- Name: index_notifications_on_mail_alert_sent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notifications_on_mail_alert_sent ON public.notifications USING btree (mail_alert_sent);


--
-- Name: index_notifications_on_mail_reminder_sent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notifications_on_mail_reminder_sent ON public.notifications USING btree (mail_reminder_sent);


--
-- Name: index_notifications_on_read_ian; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notifications_on_read_ian ON public.notifications USING btree (read_ian);


--
-- Name: index_notifications_on_recipient_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notifications_on_recipient_id ON public.notifications USING btree (recipient_id);


--
-- Name: index_notifications_on_resource; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_notifications_on_resource ON public.notifications USING btree (resource_type, resource_id);


--
-- Name: index_oauth_access_grants_on_application_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oauth_access_grants_on_application_id ON public.oauth_access_grants USING btree (application_id);


--
-- Name: index_oauth_access_grants_on_resource_owner_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oauth_access_grants_on_resource_owner_id ON public.oauth_access_grants USING btree (resource_owner_id);


--
-- Name: index_oauth_access_grants_on_token; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_oauth_access_grants_on_token ON public.oauth_access_grants USING btree (token);


--
-- Name: index_oauth_access_tokens_on_application_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oauth_access_tokens_on_application_id ON public.oauth_access_tokens USING btree (application_id);


--
-- Name: index_oauth_access_tokens_on_refresh_token; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_oauth_access_tokens_on_refresh_token ON public.oauth_access_tokens USING btree (refresh_token);


--
-- Name: index_oauth_access_tokens_on_resource_owner_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oauth_access_tokens_on_resource_owner_id ON public.oauth_access_tokens USING btree (resource_owner_id);


--
-- Name: index_oauth_access_tokens_on_token; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_oauth_access_tokens_on_token ON public.oauth_access_tokens USING btree (token);


--
-- Name: index_oauth_applications_on_integration; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oauth_applications_on_integration ON public.oauth_applications USING btree (integration_type, integration_id);


--
-- Name: index_oauth_applications_on_owner_id_and_owner_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oauth_applications_on_owner_id_and_owner_type ON public.oauth_applications USING btree (owner_id, owner_type);


--
-- Name: index_oauth_applications_on_uid; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_oauth_applications_on_uid ON public.oauth_applications USING btree (uid);


--
-- Name: index_oauth_client_tokens_on_oauth_client_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oauth_client_tokens_on_oauth_client_id ON public.oauth_client_tokens USING btree (oauth_client_id);


--
-- Name: index_oauth_client_tokens_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oauth_client_tokens_on_user_id ON public.oauth_client_tokens USING btree (user_id);


--
-- Name: index_oauth_client_tokens_on_user_id_and_oauth_client_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_oauth_client_tokens_on_user_id_and_oauth_client_id ON public.oauth_client_tokens USING btree (user_id, oauth_client_id);


--
-- Name: index_oauth_clients_on_integration; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_oauth_clients_on_integration ON public.oauth_clients USING btree (integration_type, integration_id);


--
-- Name: index_oidc_group_links_on_auth_provider_id_and_oidc_group_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_oidc_group_links_on_auth_provider_id_and_oidc_group_name ON public.oidc_group_links USING btree (auth_provider_id, oidc_group_name);


--
-- Name: index_oidc_group_links_on_group_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oidc_group_links_on_group_id ON public.oidc_group_links USING btree (group_id);


--
-- Name: index_oidc_group_memberships_on_group_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oidc_group_memberships_on_group_user_id ON public.oidc_group_memberships USING btree (group_user_id);


--
-- Name: index_oidc_user_session_links_on_oidc_session; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oidc_user_session_links_on_oidc_session ON public.oidc_user_session_links USING btree (oidc_session);


--
-- Name: index_oidc_user_session_links_on_session_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oidc_user_session_links_on_session_id ON public.oidc_user_session_links USING btree (session_id);


--
-- Name: index_oidc_user_tokens_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_oidc_user_tokens_on_user_id ON public.oidc_user_tokens USING btree (user_id);


--
-- Name: index_ordered_persisted_query_entities_on_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ordered_persisted_query_entities_on_entity ON public.ordered_persisted_query_entities USING btree (entity_type, entity_id);


--
-- Name: index_ordered_persisted_query_entities_on_persisted_query_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ordered_persisted_query_entities_on_persisted_query_id ON public.ordered_persisted_query_entities USING btree (persisted_query_id);


--
-- Name: index_ordered_pq_entities_on_query_and_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_ordered_pq_entities_on_query_and_entity ON public.ordered_persisted_query_entities USING btree (persisted_query_id, entity_type, entity_id);


--
-- Name: index_ordered_pq_entities_on_query_and_position; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ordered_pq_entities_on_query_and_position ON public.ordered_persisted_query_entities USING btree (persisted_query_id, "position");


--
-- Name: index_ordered_work_packages_on_position; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ordered_work_packages_on_position ON public.ordered_work_packages USING btree ("position");


--
-- Name: index_ordered_work_packages_on_query_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ordered_work_packages_on_query_id ON public.ordered_work_packages USING btree (query_id);


--
-- Name: index_ordered_work_packages_on_work_package_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_ordered_work_packages_on_work_package_id ON public.ordered_work_packages USING btree (work_package_id);


--
-- Name: index_outbound_mail_recipients_on_sent_on_and_mail; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_outbound_mail_recipients_on_sent_on_and_mail ON public.outbound_mail_recipients USING btree (sent_on, mail);


--
-- Name: index_paper_trail_audits_on_item_type_and_item_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_paper_trail_audits_on_item_type_and_item_id ON public.paper_trail_audits USING btree (item_type, item_id);


--
-- Name: index_persisted_queries_on_principal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_persisted_queries_on_principal_id ON public.persisted_queries USING btree (principal_id);


--
-- Name: index_persisted_queries_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_persisted_queries_on_project_id ON public.persisted_queries USING btree (project_id);


--
-- Name: index_persisted_queries_on_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_persisted_queries_on_type ON public.persisted_queries USING btree (type);


--
-- Name: index_persisted_views_on_category; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_persisted_views_on_category ON public.persisted_views USING btree (category);


--
-- Name: index_persisted_views_on_parent_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_persisted_views_on_parent_id ON public.persisted_views USING btree (parent_id);


--
-- Name: index_persisted_views_on_principal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_persisted_views_on_principal_id ON public.persisted_views USING btree (principal_id);


--
-- Name: index_persisted_views_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_persisted_views_on_project_id ON public.persisted_views USING btree (project_id);


--
-- Name: index_persisted_views_on_query; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_persisted_views_on_query ON public.persisted_views USING btree (query_type, query_id);


--
-- Name: index_persisted_views_on_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_persisted_views_on_type ON public.persisted_views USING btree (type);


--
-- Name: index_placeholder_user_details_on_principal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_placeholder_user_details_on_principal_id ON public.placeholder_user_details USING btree (principal_id);


--
-- Name: index_project_cf_project_mappings_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_cf_project_mappings_on_custom_field_id ON public.project_custom_field_project_mappings USING btree (custom_field_id);


--
-- Name: index_project_cf_type_mappings_on_custom_field_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_cf_type_mappings_on_custom_field_id ON public.project_custom_field_type_mappings USING btree (custom_field_id);


--
-- Name: index_project_custom_field_project_mappings_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_custom_field_project_mappings_on_project_id ON public.project_custom_field_project_mappings USING btree (project_id);


--
-- Name: index_project_custom_field_project_mappings_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_project_custom_field_project_mappings_unique ON public.project_custom_field_project_mappings USING btree (project_id, custom_field_id);


--
-- Name: index_project_custom_field_type_mappings_on_type_variant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_custom_field_type_mappings_on_type_variant_id ON public.project_custom_field_type_mappings USING btree (type_variant_id);


--
-- Name: index_project_custom_field_type_mappings_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_project_custom_field_type_mappings_unique ON public.project_custom_field_type_mappings USING btree (type_variant_id, custom_field_id);


--
-- Name: index_project_phase_definitions_on_color_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_phase_definitions_on_color_id ON public.project_phase_definitions USING btree (color_id);


--
-- Name: index_project_phase_definitions_on_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_project_phase_definitions_on_name ON public.project_phase_definitions USING btree (name);


--
-- Name: index_project_phase_journals_on_journal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_phase_journals_on_journal_id ON public.project_phase_journals USING btree (journal_id);


--
-- Name: index_project_phase_journals_on_phase_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_phase_journals_on_phase_id ON public.project_phase_journals USING btree (phase_id);


--
-- Name: index_project_phases_on_definition_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_phases_on_definition_id ON public.project_phases USING btree (definition_id);


--
-- Name: index_project_phases_on_finish_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_phases_on_finish_date ON public.project_phases USING btree (finish_date);


--
-- Name: index_project_phases_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_phases_on_project_id ON public.project_phases USING btree (project_id);


--
-- Name: index_project_phases_on_project_id_and_definition_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_project_phases_on_project_id_and_definition_id ON public.project_phases USING btree (project_id, definition_id);


--
-- Name: index_project_phases_on_start_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_phases_on_start_date ON public.project_phases USING btree (start_date);


--
-- Name: index_project_phases_on_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_phases_on_status ON public.project_phases USING btree (status);


--
-- Name: index_project_queries_on_public; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_queries_on_public ON public.project_queries USING btree (public);


--
-- Name: index_project_queries_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_queries_on_user_id ON public.project_queries USING btree (user_id);


--
-- Name: index_project_storages_on_creator_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_storages_on_creator_id ON public.project_storages USING btree (creator_id);


--
-- Name: index_project_storages_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_storages_on_project_id ON public.project_storages USING btree (project_id);


--
-- Name: index_project_storages_on_project_id_and_storage_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_project_storages_on_project_id_and_storage_id ON public.project_storages USING btree (project_id, storage_id);


--
-- Name: index_project_storages_on_storage_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_storages_on_storage_id ON public.project_storages USING btree (storage_id);


--
-- Name: index_project_types_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_types_on_project_id ON public.project_types USING btree (project_id);


--
-- Name: index_project_types_on_project_id_and_type_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_project_types_on_project_id_and_type_id ON public.project_types USING btree (project_id, type_id);


--
-- Name: index_project_types_on_type_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_types_on_type_id ON public.project_types USING btree (type_id);


--
-- Name: index_project_types_on_variant_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_project_types_on_variant_id ON public.project_types USING btree (variant_id);


--
-- Name: index_projects_on_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_projects_on_active ON public.projects USING btree (active);


--
-- Name: index_projects_on_lft; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_projects_on_lft ON public.projects USING btree (lft);


--
-- Name: index_projects_on_lft_and_rgt; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_projects_on_lft_and_rgt ON public.projects USING btree (lft, rgt);


--
-- Name: index_projects_on_lower_identifier; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_projects_on_lower_identifier ON public.projects USING btree (lower((identifier)::text));


--
-- Name: index_projects_on_rgt; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_projects_on_rgt ON public.projects USING btree (rgt);


--
-- Name: index_projects_on_template_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_projects_on_template_id ON public.projects USING btree (template_id);


--
-- Name: index_projects_on_workspace_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_projects_on_workspace_type ON public.projects USING btree (workspace_type);


--
-- Name: index_projects_settings_sprint_sharing; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_projects_settings_sprint_sharing ON public.projects USING gin (((settings -> 'sprint_sharing'::text)));


--
-- Name: index_push_subscriptions_on_endpoint; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_push_subscriptions_on_endpoint ON public.push_subscriptions USING btree (endpoint);


--
-- Name: index_push_subscriptions_on_session_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_push_subscriptions_on_session_id ON public.push_subscriptions USING btree (session_id);


--
-- Name: index_push_subscriptions_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_push_subscriptions_on_user_id ON public.push_subscriptions USING btree (user_id);


--
-- Name: index_queries_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_queries_on_project_id ON public.queries USING btree (project_id);


--
-- Name: index_queries_on_updated_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_queries_on_updated_at ON public.queries USING btree (updated_at);


--
-- Name: index_queries_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_queries_on_user_id ON public.queries USING btree (user_id);


--
-- Name: index_recaptcha_entries_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_recaptcha_entries_on_user_id ON public.recaptcha_entries USING btree (user_id);


--
-- Name: index_recurring_meeting_historic_schedules_on_uid; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_recurring_meeting_historic_schedules_on_uid ON public.recurring_meeting_historic_schedules USING btree (uid);


--
-- Name: index_recurring_meetings_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_recurring_meetings_on_author_id ON public.recurring_meetings USING btree (author_id);


--
-- Name: index_recurring_meetings_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_recurring_meetings_on_project_id ON public.recurring_meetings USING btree (project_id);


--
-- Name: index_recurring_meetings_on_uid; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_recurring_meetings_on_uid ON public.recurring_meetings USING btree (uid);


--
-- Name: index_relations_on_from_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_relations_on_from_id ON public.relations USING btree (from_id);


--
-- Name: index_relations_on_from_id_and_to_id_and_relation_type; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_relations_on_from_id_and_to_id_and_relation_type ON public.relations USING btree (from_id, to_id, relation_type);


--
-- Name: index_relations_on_to_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_relations_on_to_id ON public.relations USING btree (to_id);


--
-- Name: index_relations_on_to_id_and_from_id_and_relation_type; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_relations_on_to_id_and_from_id_and_relation_type ON public.relations USING btree (to_id, from_id, relation_type);


--
-- Name: index_reminder_notifications_on_notification_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_reminder_notifications_on_notification_id ON public.reminder_notifications USING btree (notification_id);


--
-- Name: index_reminder_notifications_on_reminder_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_reminder_notifications_on_reminder_id ON public.reminder_notifications USING btree (reminder_id);


--
-- Name: index_reminder_notifications_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_reminder_notifications_unique ON public.reminder_notifications USING btree (notification_id);


--
-- Name: index_reminders_on_creator_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_reminders_on_creator_id ON public.reminders USING btree (creator_id);


--
-- Name: index_reminders_on_remindable; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_reminders_on_remindable ON public.reminders USING btree (remindable_type, remindable_id);


--
-- Name: index_remote_identities_on_auth_source_type_and_auth_source_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_remote_identities_on_auth_source_type_and_auth_source_id ON public.remote_identities USING btree (auth_source_type, auth_source_id);


--
-- Name: index_remote_identities_on_integration_type_and_integration_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_remote_identities_on_integration_type_and_integration_id ON public.remote_identities USING btree (integration_type, integration_id);


--
-- Name: index_repositories_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repositories_on_project_id ON public.repositories USING btree (project_id);


--
-- Name: index_repository_branches_on_provider_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_branches_on_provider_id ON public.repository_branches USING btree (provider_id);


--
-- Name: index_repository_branches_on_repository_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_branches_on_repository_user_id ON public.repository_branches USING btree (repository_user_id);


--
-- Name: index_repository_branches_on_work_package_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_branches_on_work_package_id ON public.repository_branches USING btree (work_package_id);


--
-- Name: index_repository_commit_work_packages_on_commit_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_commit_work_packages_on_commit_id ON public.repository_commit_work_packages USING btree (commit_id);


--
-- Name: index_repository_commits_on_provider_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_commits_on_provider_id ON public.repository_commits USING btree (provider_id);


--
-- Name: index_repository_commits_on_repository_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_commits_on_repository_user_id ON public.repository_commits USING btree (repository_user_id);


--
-- Name: index_repository_issue_work_packages_on_issue_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_issue_work_packages_on_issue_id ON public.repository_issue_work_packages USING btree (issue_id);


--
-- Name: index_repository_issues_on_provider_id_and_web_url; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_repository_issues_on_provider_id_and_web_url ON public.repository_issues USING btree (provider_id, web_url);


--
-- Name: index_repository_issues_on_repository_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_issues_on_repository_user_id ON public.repository_issues USING btree (repository_user_id);


--
-- Name: index_repository_merge_requests_on_provider_id_and_web_url; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_repository_merge_requests_on_provider_id_and_web_url ON public.repository_merge_requests USING btree (provider_id, web_url);


--
-- Name: index_repository_merge_requests_on_repository_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_merge_requests_on_repository_user_id ON public.repository_merge_requests USING btree (repository_user_id);


--
-- Name: index_repository_pipelines_on_merge_request_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_pipelines_on_merge_request_id ON public.repository_pipelines USING btree (merge_request_id);


--
-- Name: index_repository_pipelines_on_provider_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_pipelines_on_provider_id ON public.repository_pipelines USING btree (provider_id);


--
-- Name: index_repository_pipelines_on_repository_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_pipelines_on_repository_user_id ON public.repository_pipelines USING btree (repository_user_id);


--
-- Name: index_repository_providers_on_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_repository_providers_on_name ON public.repository_providers USING btree (name);


--
-- Name: index_repository_providers_on_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_repository_providers_on_type ON public.repository_providers USING btree (type);


--
-- Name: index_repository_users_on_provider_id_and_external_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_repository_users_on_provider_id_and_external_id ON public.repository_users USING btree (provider_id, external_id);


--
-- Name: index_resource_allocation_journals_on_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocation_journals_on_entity ON public.resource_allocation_journals USING btree (entity_type, entity_id);


--
-- Name: index_resource_allocation_journals_on_principal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocation_journals_on_principal_id ON public.resource_allocation_journals USING btree (principal_id);


--
-- Name: index_resource_allocation_journals_on_requested_by_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocation_journals_on_requested_by_id ON public.resource_allocation_journals USING btree (requested_by_id);


--
-- Name: index_resource_allocation_journals_on_reviewed_by_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocation_journals_on_reviewed_by_id ON public.resource_allocation_journals USING btree (reviewed_by_id);


--
-- Name: index_resource_allocation_scheduled_entries_on_overtime; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocation_scheduled_entries_on_overtime ON public.resource_allocation_scheduled_entries USING btree (user_id, allocated_on) WHERE (overtime_minutes > 0);


--
-- Name: index_resource_allocations_on_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocations_on_entity ON public.resource_allocations USING btree (entity_type, entity_id);


--
-- Name: index_resource_allocations_on_placeholder_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocations_on_placeholder_user_id ON public.resource_allocations USING btree (placeholder_user_id);


--
-- Name: index_resource_allocations_on_principal_assigned_by_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocations_on_principal_assigned_by_id ON public.resource_allocations USING btree (principal_assigned_by_id);


--
-- Name: index_resource_allocations_on_principal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocations_on_principal_id ON public.resource_allocations USING btree (principal_id);


--
-- Name: index_resource_allocations_on_requested_by_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocations_on_requested_by_id ON public.resource_allocations USING btree (requested_by_id);


--
-- Name: index_resource_allocations_on_reviewed_by_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocations_on_reviewed_by_id ON public.resource_allocations USING btree (reviewed_by_id);


--
-- Name: index_resource_allocations_on_start_date_and_end_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocations_on_start_date_and_end_date ON public.resource_allocations USING btree (start_date, end_date);


--
-- Name: index_resource_allocations_on_state; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_resource_allocations_on_state ON public.resource_allocations USING btree (state);


--
-- Name: index_role_permissions_on_role_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_role_permissions_on_role_id ON public.role_permissions USING btree (role_id);


--
-- Name: index_roles_on_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_roles_on_LOWER_name" ON public.roles USING btree (lower((name)::text));


--
-- Name: index_scim_clients_on_auth_provider_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_scim_clients_on_auth_provider_id ON public.scim_clients USING btree (auth_provider_id);


--
-- Name: index_service_account_associations_on_service_account_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_service_account_associations_on_service_account_id ON public.service_account_associations USING btree (service_account_id);


--
-- Name: index_sessions_on_session_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_sessions_on_session_id ON public.sessions USING btree (session_id);


--
-- Name: index_sessions_on_updated_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_sessions_on_updated_at ON public.sessions USING btree (updated_at);


--
-- Name: index_sessions_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_sessions_on_user_id ON public.sessions USING btree (user_id);


--
-- Name: index_settings_on_name; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_settings_on_name ON public.settings USING btree (name);


--
-- Name: index_sprint_goals_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_sprint_goals_on_project_id ON public.sprint_goals USING btree (project_id);


--
-- Name: index_sprint_goals_on_sprint_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_sprint_goals_on_sprint_id ON public.sprint_goals USING btree (sprint_id);


--
-- Name: index_sprint_goals_on_sprint_id_and_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_sprint_goals_on_sprint_id_and_project_id ON public.sprint_goals USING btree (sprint_id, project_id);


--
-- Name: index_sprints_on_origin; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_sprints_on_origin ON public.sprints USING btree (origin_source, origin_host, origin_id);


--
-- Name: index_sprints_on_origin_host_and_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_sprints_on_origin_host_and_id ON public.sprints USING btree (origin_host, origin_id);


--
-- Name: index_sprints_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_sprints_on_project_id ON public.sprints USING btree (project_id);


--
-- Name: index_squashtm_project_links_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_squashtm_project_links_on_project_id ON public.squashtm_project_links USING btree (project_id);


--
-- Name: index_squashtm_project_links_on_squashtm_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_squashtm_project_links_on_squashtm_project_id ON public.squashtm_project_links USING btree (squashtm_project_id);


--
-- Name: index_squashtm_servers_singleton; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_squashtm_servers_singleton ON public.squashtm_servers USING btree ((1));


--
-- Name: index_squashtm_work_package_links_on_squashtm_requirement_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_squashtm_work_package_links_on_squashtm_requirement_id ON public.squashtm_work_package_links USING btree (squashtm_requirement_id);


--
-- Name: index_squashtm_work_package_links_on_work_package_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_squashtm_work_package_links_on_work_package_id ON public.squashtm_work_package_links USING btree (work_package_id);


--
-- Name: index_statuses_on_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_statuses_on_LOWER_name" ON public.statuses USING btree (lower((name)::text));


--
-- Name: index_statuses_on_category; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_statuses_on_category ON public.statuses USING btree (category);


--
-- Name: index_statuses_on_color_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_statuses_on_color_id ON public.statuses USING btree (color_id);


--
-- Name: index_statuses_on_is_default; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_statuses_on_is_default ON public.statuses USING btree (is_default);


--
-- Name: index_statuses_on_position; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_statuses_on_position ON public.statuses USING btree ("position");


--
-- Name: index_storages_file_links_journals_on_file_link_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_storages_file_links_journals_on_file_link_id ON public.storages_file_links_journals USING btree (file_link_id);


--
-- Name: index_storages_file_links_journals_on_journal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_storages_file_links_journals_on_journal_id ON public.storages_file_links_journals USING btree (journal_id);


--
-- Name: index_storages_on_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_storages_on_LOWER_name" ON public.storages USING btree (lower((name)::text));


--
-- Name: index_storages_on_creator_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_storages_on_creator_id ON public.storages USING btree (creator_id);


--
-- Name: index_storages_on_host; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_storages_on_host ON public.storages USING btree (host);


--
-- Name: index_storages_on_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_storages_on_name ON public.storages USING btree (name);


--
-- Name: index_subproject_template_assignments_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_subproject_template_assignments_on_project_id ON public.subproject_template_assignments USING btree (project_id);


--
-- Name: index_subproject_template_assignments_on_template_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_subproject_template_assignments_on_template_id ON public.subproject_template_assignments USING btree (template_id);


--
-- Name: index_subproject_template_assignments_on_workspace_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_subproject_template_assignments_on_workspace_type ON public.subproject_template_assignments USING btree (workspace_type);


--
-- Name: index_teap_on_project_id_and_activity_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_teap_on_project_id_and_activity_id ON public.time_entry_activities_projects USING btree (project_id, activity_id);


--
-- Name: index_time_entries_on_activity_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entries_on_activity_id ON public.time_entries USING btree (activity_id);


--
-- Name: index_time_entries_on_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entries_on_created_at ON public.time_entries USING btree (created_at);


--
-- Name: index_time_entries_on_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entries_on_entity ON public.time_entries USING btree (entity_type, entity_id);


--
-- Name: index_time_entries_on_entity_type_and_entity_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entries_on_entity_type_and_entity_id ON public.time_entries USING btree (entity_type, entity_id);


--
-- Name: index_time_entries_on_logged_by_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entries_on_logged_by_id ON public.time_entries USING btree (logged_by_id);


--
-- Name: index_time_entries_on_ongoing; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entries_on_ongoing ON public.time_entries USING btree (ongoing);


--
-- Name: index_time_entries_on_project_id_and_updated_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entries_on_project_id_and_updated_at ON public.time_entries USING btree (project_id, updated_at);


--
-- Name: index_time_entries_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entries_on_user_id ON public.time_entries USING btree (user_id);


--
-- Name: index_time_entries_on_user_id_and_ongoing; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_time_entries_on_user_id_and_ongoing ON public.time_entries USING btree (user_id, ongoing) WHERE (ongoing = true);


--
-- Name: index_time_entry_activities_projects_on_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entry_activities_projects_on_active ON public.time_entry_activities_projects USING btree (active);


--
-- Name: index_time_entry_activities_projects_on_activity_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entry_activities_projects_on_activity_id ON public.time_entry_activities_projects USING btree (activity_id);


--
-- Name: index_time_entry_activities_projects_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entry_activities_projects_on_project_id ON public.time_entry_activities_projects USING btree (project_id);


--
-- Name: index_time_entry_journals_on_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entry_journals_on_entity ON public.time_entry_journals USING btree (entity_type, entity_id);


--
-- Name: index_time_entry_journals_on_entity_type_and_entity_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entry_journals_on_entity_type_and_entity_id ON public.time_entry_journals USING btree (entity_type, entity_id);


--
-- Name: index_time_entry_journals_on_logged_by_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entry_journals_on_logged_by_id ON public.time_entry_journals USING btree (logged_by_id);


--
-- Name: index_time_entry_journals_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_time_entry_journals_on_project_id ON public.time_entry_journals USING btree (project_id);


--
-- Name: index_token_bucket_states_on_identifier; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_token_bucket_states_on_identifier ON public.token_bucket_states USING btree (identifier);


--
-- Name: index_tokens_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_tokens_on_user_id ON public.tokens USING btree (user_id);


--
-- Name: index_two_factor_authentication_devices_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_two_factor_authentication_devices_on_user_id ON public.two_factor_authentication_devices USING btree (user_id);


--
-- Name: index_two_factor_authentication_devices_on_webauthn_external_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_two_factor_authentication_devices_on_webauthn_external_id ON public.two_factor_authentication_devices USING btree (webauthn_external_id);


--
-- Name: index_type_variants_on_defaults_source_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_type_variants_on_defaults_source_id ON public.type_variants USING btree (defaults_source_id);


--
-- Name: index_type_variants_on_form_configuration_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_type_variants_on_form_configuration_id ON public.type_variants USING btree (form_configuration_id);


--
-- Name: index_type_variants_on_pdf_export_source_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_type_variants_on_pdf_export_source_id ON public.type_variants USING btree (pdf_export_source_id);


--
-- Name: index_type_variants_on_project_attributes_source_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_type_variants_on_project_attributes_source_id ON public.type_variants USING btree (project_attributes_source_id);


--
-- Name: index_type_variants_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_type_variants_on_project_id ON public.type_variants USING btree (project_id);


--
-- Name: index_type_variants_on_type_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_type_variants_on_type_id ON public.type_variants USING btree (type_id);


--
-- Name: index_type_variants_on_type_id_and_LOWER_variant_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_type_variants_on_type_id_and_LOWER_variant_name" ON public.type_variants USING btree (type_id, lower((variant_name)::text)) WHERE ((variant_name IS NOT NULL) AND (project_id IS NULL));


--
-- Name: index_type_variants_on_type_id_project_and_LOWER_variant_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_type_variants_on_type_id_project_and_LOWER_variant_name" ON public.type_variants USING btree (type_id, project_id, lower((variant_name)::text)) WHERE ((variant_name IS NOT NULL) AND (project_id IS NOT NULL));


--
-- Name: index_type_variants_on_workflow_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_type_variants_on_workflow_id ON public.type_variants USING btree (workflow_id);


--
-- Name: index_type_variants_one_base_per_type; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_type_variants_one_base_per_type ON public.type_variants USING btree (type_id) WHERE is_default_variant;


--
-- Name: index_type_variants_one_new_project_default_per_type; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_type_variants_one_new_project_default_per_type ON public.type_variants USING btree (type_id) WHERE enabled_in_new_projects;


--
-- Name: index_types_on_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_types_on_LOWER_name" ON public.types USING btree (lower((name)::text));


--
-- Name: index_types_on_color_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_types_on_color_id ON public.types USING btree (color_id);


--
-- Name: index_unique_calc_val_error_per_customized_and_cf; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_unique_calc_val_error_per_customized_and_cf ON public.calculated_value_errors USING btree (customized_type, customized_id, custom_field_id, error_code);


--
-- Name: index_user_auth_provider_links_on_auth_provider_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_user_auth_provider_links_on_auth_provider_id ON public.user_auth_provider_links USING btree (auth_provider_id);


--
-- Name: index_user_auth_provider_links_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_user_auth_provider_links_on_user_id ON public.user_auth_provider_links USING btree (user_id);


--
-- Name: index_user_auth_provider_links_on_user_id_and_auth_provider_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_user_auth_provider_links_on_user_id_and_auth_provider_id ON public.user_auth_provider_links USING btree (user_id, auth_provider_id);


--
-- Name: index_user_non_working_times_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_user_non_working_times_on_user_id ON public.user_non_working_times USING btree (user_id);


--
-- Name: index_user_passwords_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_user_passwords_on_user_id ON public.user_passwords USING btree (user_id);


--
-- Name: index_user_preferences_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_user_preferences_on_user_id ON public.user_preferences USING btree (user_id);


--
-- Name: index_user_prefs_settings_daily_reminders_enabled; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_user_prefs_settings_daily_reminders_enabled ON public.user_preferences USING gin ((((settings -> 'daily_reminders'::text) -> 'enabled'::text)));


--
-- Name: index_user_prefs_settings_daily_reminders_times; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_user_prefs_settings_daily_reminders_times ON public.user_preferences USING gin ((((settings -> 'daily_reminders'::text) -> 'times'::text)));


--
-- Name: index_user_prefs_settings_pause_reminders_enabled; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_user_prefs_settings_pause_reminders_enabled ON public.user_preferences USING btree (((((settings -> 'pause_reminders'::text) ->> 'enabled'::text))::boolean));


--
-- Name: index_user_prefs_settings_time_zone; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_user_prefs_settings_time_zone ON public.user_preferences USING gin (((settings -> 'time_zone'::text)));


--
-- Name: index_user_prefs_settings_workdays; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_user_prefs_settings_workdays ON public.user_preferences USING gin (((settings -> 'workdays'::text)));


--
-- Name: index_user_working_hours_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_user_working_hours_on_user_id ON public.user_working_hours USING btree (user_id);


--
-- Name: index_user_working_hours_on_user_id_and_valid_from; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_user_working_hours_on_user_id_and_valid_from ON public.user_working_hours USING btree (user_id, valid_from);


--
-- Name: index_user_working_hours_on_valid_from; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_user_working_hours_on_valid_from ON public.user_working_hours USING btree (valid_from);


--
-- Name: index_users_on_id_and_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_users_on_id_and_type ON public.users USING btree (id, type);


--
-- Name: index_users_on_ldap_auth_source_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_users_on_ldap_auth_source_id ON public.users USING btree (ldap_auth_source_id);


--
-- Name: index_users_on_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_users_on_type ON public.users USING btree (type);


--
-- Name: index_users_on_type_and_login; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_users_on_type_and_login ON public.users USING btree (type, login);


--
-- Name: index_users_on_type_and_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_users_on_type_and_status ON public.users USING btree (type, status);


--
-- Name: index_versions_on_project_id_and_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_versions_on_project_id_and_LOWER_name" ON public.versions USING btree (project_id, lower((name)::text));


--
-- Name: index_versions_on_sharing; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_versions_on_sharing ON public.versions USING btree (sharing);


--
-- Name: index_views_on_query_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_views_on_query_id ON public.views USING btree (query_id);


--
-- Name: index_watchers_on_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_watchers_on_user_id ON public.watchers USING btree (user_id);


--
-- Name: index_watchers_on_user_id_and_watchable; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_watchers_on_user_id_and_watchable ON public.watchers USING btree (user_id, watchable_type, watchable_id);


--
-- Name: index_watchers_on_watchable_id_and_watchable_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_watchers_on_watchable_id_and_watchable_type ON public.watchers USING btree (watchable_id, watchable_type);


--
-- Name: index_webhooks_events_on_webhooks_webhook_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_webhooks_events_on_webhooks_webhook_id ON public.webhooks_events USING btree (webhooks_webhook_id);


--
-- Name: index_webhooks_logs_on_webhooks_webhook_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_webhooks_logs_on_webhooks_webhook_id ON public.webhooks_logs USING btree (webhooks_webhook_id);


--
-- Name: index_webhooks_projects_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_webhooks_projects_on_project_id ON public.webhooks_projects USING btree (project_id);


--
-- Name: index_webhooks_projects_on_webhooks_webhook_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_webhooks_projects_on_webhooks_webhook_id ON public.webhooks_projects USING btree (webhooks_webhook_id);


--
-- Name: index_webhooks_webhooks_on_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_webhooks_webhooks_on_LOWER_name" ON public.webhooks_webhooks USING btree (lower((name)::text));


--
-- Name: index_wiki_page_links_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_wiki_page_links_on_author_id ON public.wiki_page_links USING btree (author_id);


--
-- Name: index_wiki_page_links_on_linkable; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_wiki_page_links_on_linkable ON public.wiki_page_links USING btree (linkable_type, linkable_id);


--
-- Name: index_wiki_page_links_on_provider_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_wiki_page_links_on_provider_id ON public.wiki_page_links USING btree (provider_id);


--
-- Name: index_wiki_pages_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_wiki_pages_on_author_id ON public.wiki_pages USING btree (author_id);


--
-- Name: index_wiki_pages_on_parent_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_wiki_pages_on_parent_id ON public.wiki_pages USING btree (parent_id);


--
-- Name: index_wiki_pages_on_updated_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_wiki_pages_on_updated_at ON public.wiki_pages USING btree (updated_at);


--
-- Name: index_wiki_pages_on_wiki_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_wiki_pages_on_wiki_id ON public.wiki_pages USING btree (wiki_id);


--
-- Name: index_wiki_providers_on_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_wiki_providers_on_name ON public.wiki_providers USING btree (name);


--
-- Name: index_wiki_providers_on_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_wiki_providers_on_type ON public.wiki_providers USING btree (type);


--
-- Name: index_wiki_providers_on_universal_identifier; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_wiki_providers_on_universal_identifier ON public.wiki_providers USING btree (universal_identifier);


--
-- Name: index_wiki_redirects_on_wiki_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_wiki_redirects_on_wiki_id ON public.wiki_redirects USING btree (wiki_id);


--
-- Name: index_work_package_journals_on_assigned_to_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_assigned_to_id ON public.work_package_journals USING btree (assigned_to_id);


--
-- Name: index_work_package_journals_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_author_id ON public.work_package_journals USING btree (author_id);


--
-- Name: index_work_package_journals_on_backlog_bucket_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_backlog_bucket_id ON public.work_package_journals USING btree (backlog_bucket_id);


--
-- Name: index_work_package_journals_on_category_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_category_id ON public.work_package_journals USING btree (category_id);


--
-- Name: index_work_package_journals_on_due_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_due_date ON public.work_package_journals USING btree (due_date);


--
-- Name: index_work_package_journals_on_parent_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_parent_id ON public.work_package_journals USING btree (parent_id);


--
-- Name: index_work_package_journals_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_project_id ON public.work_package_journals USING btree (project_id);


--
-- Name: index_work_package_journals_on_responsible_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_responsible_id ON public.work_package_journals USING btree (responsible_id);


--
-- Name: index_work_package_journals_on_schedule_manually; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_schedule_manually ON public.work_package_journals USING btree (schedule_manually);


--
-- Name: index_work_package_journals_on_sprint_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_sprint_id ON public.work_package_journals USING btree (sprint_id);


--
-- Name: index_work_package_journals_on_start_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_start_date ON public.work_package_journals USING btree (start_date);


--
-- Name: index_work_package_journals_on_status_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_status_id ON public.work_package_journals USING btree (status_id);


--
-- Name: index_work_package_journals_on_type_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_type_id ON public.work_package_journals USING btree (type_id);


--
-- Name: index_work_package_journals_on_version_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_journals_on_version_id ON public.work_package_journals USING btree (version_id);


--
-- Name: index_work_package_semantic_aliases_on_identifier; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_work_package_semantic_aliases_on_identifier ON public.work_package_semantic_aliases USING btree (identifier);


--
-- Name: index_work_package_semantic_aliases_on_work_package_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_semantic_aliases_on_work_package_id ON public.work_package_semantic_aliases USING btree (work_package_id);


--
-- Name: index_work_package_version_journals_on_journal_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_version_journals_on_journal_id ON public.work_package_version_journals USING btree (journal_id);


--
-- Name: index_work_package_version_journals_on_version_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_package_version_journals_on_version_id ON public.work_package_version_journals USING btree (version_id);


--
-- Name: index_work_packages_on_assigned_to_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_assigned_to_id ON public.work_packages USING btree (assigned_to_id);


--
-- Name: index_work_packages_on_author_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_author_id ON public.work_packages USING btree (author_id);


--
-- Name: index_work_packages_on_backlog_bucket_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_backlog_bucket_id ON public.work_packages USING btree (backlog_bucket_id);


--
-- Name: index_work_packages_on_category_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_category_id ON public.work_packages USING btree (category_id);


--
-- Name: index_work_packages_on_created_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_created_at ON public.work_packages USING btree (created_at);


--
-- Name: index_work_packages_on_due_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_due_date ON public.work_packages USING btree (due_date);


--
-- Name: index_work_packages_on_identifier; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_work_packages_on_identifier ON public.work_packages USING btree (identifier) WHERE (identifier IS NOT NULL);


--
-- Name: index_work_packages_on_origin; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_origin ON public.work_packages USING btree (origin_source, origin_host, origin_id);


--
-- Name: index_work_packages_on_origin_host_and_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_work_packages_on_origin_host_and_id ON public.work_packages USING btree (origin_host, origin_id);


--
-- Name: index_work_packages_on_parent_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_parent_id ON public.work_packages USING btree (parent_id);


--
-- Name: index_work_packages_on_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_project_id ON public.work_packages USING btree (project_id);


--
-- Name: index_work_packages_on_project_id_and_sequence_number; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX index_work_packages_on_project_id_and_sequence_number ON public.work_packages USING btree (project_id, sequence_number) WHERE (sequence_number IS NOT NULL);


--
-- Name: index_work_packages_on_project_id_and_updated_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_project_id_and_updated_at ON public.work_packages USING btree (project_id, updated_at);


--
-- Name: index_work_packages_on_project_phase_definition_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_project_phase_definition_id ON public.work_packages USING btree (project_phase_definition_id);


--
-- Name: index_work_packages_on_responsible_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_responsible_id ON public.work_packages USING btree (responsible_id);


--
-- Name: index_work_packages_on_schedule_manually; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_schedule_manually ON public.work_packages USING btree (schedule_manually) WHERE schedule_manually;


--
-- Name: index_work_packages_on_sequence_number; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_sequence_number ON public.work_packages USING btree (sequence_number) WHERE (sequence_number IS NOT NULL);


--
-- Name: index_work_packages_on_sprint_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_sprint_id ON public.work_packages USING btree (sprint_id);


--
-- Name: index_work_packages_on_start_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_start_date ON public.work_packages USING btree (start_date);


--
-- Name: index_work_packages_on_status_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_status_id ON public.work_packages USING btree (status_id);


--
-- Name: index_work_packages_on_subject_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_subject_trgm ON public.work_packages USING gin (subject gin_trgm_ops);


--
-- Name: index_work_packages_on_type_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_type_id ON public.work_packages USING btree (type_id);


--
-- Name: index_work_packages_on_updated_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_updated_at ON public.work_packages USING btree (updated_at);


--
-- Name: index_work_packages_on_version_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_work_packages_on_version_id ON public.work_packages USING btree (version_id);


--
-- Name: index_workflows_on_LOWER_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX "index_workflows_on_LOWER_name" ON public.workflows USING btree (lower((name)::text));


--
-- Name: index_workflows_status_transitions_on_new_status_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflows_status_transitions_on_new_status_id ON public.workflows_status_transitions USING btree (new_status_id);


--
-- Name: index_workflows_status_transitions_on_old_status_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflows_status_transitions_on_old_status_id ON public.workflows_status_transitions USING btree (old_status_id);


--
-- Name: index_workflows_status_transitions_on_role_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflows_status_transitions_on_role_id ON public.workflows_status_transitions USING btree (role_id);


--
-- Name: index_workflows_status_transitions_on_workflow_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_workflows_status_transitions_on_workflow_id ON public.workflows_status_transitions USING btree (workflow_id);


--
-- Name: index_wp_semantic_aliases_on_lower_identifier_text_pattern; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX index_wp_semantic_aliases_on_lower_identifier_text_pattern ON public.work_package_semantic_aliases USING btree (lower((identifier)::text) text_pattern_ops);


--
-- Name: issue_categories_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX issue_categories_project_id ON public.categories USING btree (project_id);


--
-- Name: item_anc_desc_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX item_anc_desc_idx ON public.hierarchical_item_hierarchies USING btree (ancestor_id, descendant_id, generations);


--
-- Name: item_desc_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX item_desc_idx ON public.hierarchical_item_hierarchies USING btree (descendant_id);


--
-- Name: messages_board_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_board_id ON public.messages USING btree (forum_id);


--
-- Name: messages_parent_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_parent_id ON public.messages USING btree (parent_id);


--
-- Name: news_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX news_project_id ON public.news USING btree (project_id);


--
-- Name: time_entries_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX time_entries_project_id ON public.time_entries USING btree (project_id);


--
-- Name: unique_index_gh_prs_wps_on_gh_pr_id_and_wp_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX unique_index_gh_prs_wps_on_gh_pr_id_and_wp_id ON public.github_pull_requests_work_packages USING btree (github_pull_request_id, work_package_id);


--
-- Name: unique_index_gl_issues_wps_on_gl_issue_id_and_wp_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX unique_index_gl_issues_wps_on_gl_issue_id_and_wp_id ON public.gitlab_issues_work_packages USING btree (gitlab_issue_id, work_package_id);


--
-- Name: unique_index_gl_mrs_wps_on_gl_mr_id_and_wp_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX unique_index_gl_mrs_wps_on_gl_mr_id_and_wp_id ON public.gitlab_merge_requests_work_packages USING btree (gitlab_merge_request_id, work_package_id);


--
-- Name: unique_inherited_role; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX unique_inherited_role ON public.member_roles USING btree (member_id, role_id, inherited_from);


--
-- Name: unique_lastname_for_placeholder_users; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX unique_lastname_for_placeholder_users ON public.users USING btree (lastname, type) WHERE ((type)::text = 'PlaceholderUser'::text);


--
-- Name: versions_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX versions_project_id ON public.versions USING btree (project_id);


--
-- Name: watchers_user_id_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX watchers_user_id_type ON public.watchers USING btree (user_id, watchable_type);


--
-- Name: wiki_pages_wiki_id_slug; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX wiki_pages_wiki_id_slug ON public.wiki_pages USING btree (wiki_id, slug);


--
-- Name: wiki_pages_wiki_id_title; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX wiki_pages_wiki_id_title ON public.wiki_pages USING btree (wiki_id, title);


--
-- Name: wiki_redirects_wiki_id_title; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX wiki_redirects_wiki_id_title ON public.wiki_redirects USING btree (wiki_id, title);


--
-- Name: wikis_project_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX wikis_project_id ON public.wikis USING btree (project_id);


--
-- Name: wkfs_role_workflow_old_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX wkfs_role_workflow_old_status ON public.workflows_status_transitions USING btree (role_id, workflow_id, old_status_id);


--
-- Name: work_package_anc_desc_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX work_package_anc_desc_idx ON public.work_package_hierarchies USING btree (ancestor_id, descendant_id, generations);


--
-- Name: work_package_desc_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX work_package_desc_idx ON public.work_package_hierarchies USING btree (descendant_id);


--
-- Name: work_package_journal_on_burndown_attributes; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX work_package_journal_on_burndown_attributes ON public.work_package_journals USING btree (sprint_id, status_id, project_id, type_id);


--
-- Name: form_configuration_attributes fk_form_configuration_attributes_group_owner; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configuration_attributes
    ADD CONSTRAINT fk_form_configuration_attributes_group_owner FOREIGN KEY (form_configuration_group_id, form_configuration_id) REFERENCES public.form_configuration_groups(id, form_configuration_id);


--
-- Name: jira_open_project_references fk_rails_0053bfddce; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_open_project_references
    ADD CONSTRAINT fk_rails_0053bfddce FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: llm_connections fk_rails_0078bbbbed; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_connections
    ADD CONSTRAINT fk_rails_0078bbbbed FOREIGN KEY (default_embedding_model_id) REFERENCES public.llm_models(id) ON DELETE SET NULL;


--
-- Name: repository_merge_requests fk_rails_03ae3c3181; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_merge_requests
    ADD CONSTRAINT fk_rails_03ae3c3181 FOREIGN KEY (repository_user_id) REFERENCES public.repository_users(id) ON DELETE SET NULL;


--
-- Name: project_storages fk_rails_04546d7b88; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_storages
    ADD CONSTRAINT fk_rails_04546d7b88 FOREIGN KEY (storage_id) REFERENCES public.storages(id) ON DELETE CASCADE;


--
-- Name: bcf_comments fk_rails_0571c4b386; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bcf_comments
    ADD CONSTRAINT fk_rails_0571c4b386 FOREIGN KEY (reply_to) REFERENCES public.bcf_comments(id) ON DELETE SET NULL;


--
-- Name: oidc_group_memberships fk_rails_061b856d2a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_group_memberships
    ADD CONSTRAINT fk_rails_061b856d2a FOREIGN KEY (group_user_id) REFERENCES public.group_users(id) ON DELETE CASCADE;


--
-- Name: squashtm_work_package_links fk_rails_0664f5f198; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.squashtm_work_package_links
    ADD CONSTRAINT fk_rails_0664f5f198 FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: notifications fk_rails_06a39bb8cc; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT fk_rails_06a39bb8cc FOREIGN KEY (actor_id) REFERENCES public.users(id);


--
-- Name: meeting_agenda_items fk_rails_06b94602d9; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_agenda_items
    ADD CONSTRAINT fk_rails_06b94602d9 FOREIGN KEY (presenter_id) REFERENCES public.users(id);


--
-- Name: repository_merge_request_work_packages fk_rails_076f67ef33; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_merge_request_work_packages
    ADD CONSTRAINT fk_rails_076f67ef33 FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: placeholder_user_details fk_rails_07b942d54a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.placeholder_user_details
    ADD CONSTRAINT fk_rails_07b942d54a FOREIGN KEY (principal_id) REFERENCES public.users(id);


--
-- Name: ordered_persisted_query_entities fk_rails_08fab5b34d; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ordered_persisted_query_entities
    ADD CONSTRAINT fk_rails_08fab5b34d FOREIGN KEY (persisted_query_id) REFERENCES public.persisted_queries(id) ON DELETE CASCADE;


--
-- Name: two_factor_authentication_devices fk_rails_0b09e132e7; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.two_factor_authentication_devices
    ADD CONSTRAINT fk_rails_0b09e132e7 FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: workflows_status_transitions fk_rails_0c5f149c21; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflows_status_transitions
    ADD CONSTRAINT fk_rails_0c5f149c21 FOREIGN KEY (role_id) REFERENCES public.roles(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: notification_settings fk_rails_0c95e91db7; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notification_settings
    ADD CONSTRAINT fk_rails_0c95e91db7 FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: project_phase_definitions fk_rails_0cd5e50674; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_phase_definitions
    ADD CONSTRAINT fk_rails_0cd5e50674 FOREIGN KEY (color_id) REFERENCES public.colors(id);


--
-- Name: cost_entries fk_rails_0d35f09506; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cost_entries
    ADD CONSTRAINT fk_rails_0d35f09506 FOREIGN KEY (logged_by_id) REFERENCES public.users(id);


--
-- Name: message_work_packages fk_rails_0e3f922619; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_work_packages
    ADD CONSTRAINT fk_rails_0e3f922619 FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: ai_text_action_types fk_rails_0e69391ef4; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_action_types
    ADD CONSTRAINT fk_rails_0e69391ef4 FOREIGN KEY (type_id) REFERENCES public.types(id) ON DELETE CASCADE;


--
-- Name: project_phase_journals fk_rails_0e71534908; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_phase_journals
    ADD CONSTRAINT fk_rails_0e71534908 FOREIGN KEY (journal_id) REFERENCES public.journals(id);


--
-- Name: oidc_group_links fk_rails_0ed529eb13; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_group_links
    ADD CONSTRAINT fk_rails_0ed529eb13 FOREIGN KEY (auth_provider_id) REFERENCES public.auth_providers(id) ON DELETE CASCADE;


--
-- Name: repository_issues fk_rails_0f05e2b64d; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_issues
    ADD CONSTRAINT fk_rails_0f05e2b64d FOREIGN KEY (repository_user_id) REFERENCES public.repository_users(id) ON DELETE SET NULL;


--
-- Name: project_custom_field_type_mappings fk_rails_121e8310f2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_custom_field_type_mappings
    ADD CONSTRAINT fk_rails_121e8310f2 FOREIGN KEY (custom_field_id) REFERENCES public.custom_fields(id);


--
-- Name: custom_fields_projects fk_rails_12fb30588e; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_fields_projects
    ADD CONSTRAINT fk_rails_12fb30588e FOREIGN KEY (project_id) REFERENCES public.projects(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: custom_field_project_specific_value_restrictions fk_rails_14adb03dbe; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_field_project_specific_value_restrictions
    ADD CONSTRAINT fk_rails_14adb03dbe FOREIGN KEY (custom_field_id) REFERENCES public.custom_fields(id) ON DELETE CASCADE;


--
-- Name: oidc_user_tokens fk_rails_1571e35480; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_user_tokens
    ADD CONSTRAINT fk_rails_1571e35480 FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: custom_options fk_rails_1698662681; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_options
    ADD CONSTRAINT fk_rails_1698662681 FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: work_package_semantic_aliases fk_rails_183ef26ed5; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_semantic_aliases
    ADD CONSTRAINT fk_rails_183ef26ed5 FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: service_account_associations fk_rails_18733515b3; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.service_account_associations
    ADD CONSTRAINT fk_rails_18733515b3 FOREIGN KEY (service_account_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: form_configuration_attributes fk_rails_1927ea7bff; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configuration_attributes
    ADD CONSTRAINT fk_rails_1927ea7bff FOREIGN KEY (form_configuration_id) REFERENCES public.form_configurations(id) ON DELETE CASCADE;


--
-- Name: itsm_email_messages fk_rails_19c8aea8a2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_email_messages
    ADD CONSTRAINT fk_rails_19c8aea8a2 FOREIGN KEY (journal_id) REFERENCES public.journals(id) ON DELETE SET NULL;


--
-- Name: project_custom_field_project_mappings fk_rails_1a1f3f10e9; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_custom_field_project_mappings
    ADD CONSTRAINT fk_rails_1a1f3f10e9 FOREIGN KEY (custom_field_id) REFERENCES public.custom_fields(id);


--
-- Name: type_variants fk_rails_1efeac767e; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.type_variants
    ADD CONSTRAINT fk_rails_1efeac767e FOREIGN KEY (type_id) REFERENCES public.types(id) ON DELETE CASCADE;


--
-- Name: type_variants fk_rails_228b27da10; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.type_variants
    ADD CONSTRAINT fk_rails_228b27da10 FOREIGN KEY (defaults_source_id) REFERENCES public.type_variants(id) ON DELETE RESTRICT;


--
-- Name: project_custom_field_type_mappings fk_rails_2309b84a95; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_custom_field_type_mappings
    ADD CONSTRAINT fk_rails_2309b84a95 FOREIGN KEY (type_variant_id) REFERENCES public.type_variants(id) ON DELETE CASCADE;


--
-- Name: automation_actions fk_rails_245fe3f495; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automation_actions
    ADD CONSTRAINT fk_rails_245fe3f495 FOREIGN KEY (custom_field_id) REFERENCES public.custom_fields(id) ON DELETE CASCADE;


--
-- Name: repository_branches fk_rails_25380fb38f; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_branches
    ADD CONSTRAINT fk_rails_25380fb38f FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: resource_allocation_scheduled_entries fk_rails_2a587e7243; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocation_scheduled_entries
    ADD CONSTRAINT fk_rails_2a587e7243 FOREIGN KEY (resource_allocation_id) REFERENCES public.resource_allocations(id) ON DELETE CASCADE;


--
-- Name: group_details fk_rails_2b463af4b4; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_details
    ADD CONSTRAINT fk_rails_2b463af4b4 FOREIGN KEY (principal_id) REFERENCES public.users(id);


--
-- Name: ldap_departments_memberships fk_rails_2b46c35189; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_departments_memberships
    ADD CONSTRAINT fk_rails_2b46c35189 FOREIGN KEY (synchronized_department_id) REFERENCES public.ldap_departments_synchronized_departments(id);


--
-- Name: ai_text_transform_runs fk_rails_2b98c42b09; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_transform_runs
    ADD CONSTRAINT fk_rails_2b98c42b09 FOREIGN KEY (ai_text_action_id) REFERENCES public.ai_text_actions(id) ON DELETE CASCADE;


--
-- Name: recurring_meeting_interim_responses fk_rails_2bc91bc636; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_meeting_interim_responses
    ADD CONSTRAINT fk_rails_2bc91bc636 FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: resource_allocations fk_rails_2e3b67c9e4; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocations
    ADD CONSTRAINT fk_rails_2e3b67c9e4 FOREIGN KEY (requested_by_id) REFERENCES public.users(id);


--
-- Name: work_package_categories fk_rails_2eda24e621; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_categories
    ADD CONSTRAINT fk_rails_2eda24e621 FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: jira_attachments fk_rails_2f24484001; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_attachments
    ADD CONSTRAINT fk_rails_2f24484001 FOREIGN KEY (jira_project_id) REFERENCES public.jira_projects(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: repository_issue_work_packages fk_rails_304f1eeae5; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_issue_work_packages
    ADD CONSTRAINT fk_rails_304f1eeae5 FOREIGN KEY (issue_id) REFERENCES public.repository_issues(id) ON DELETE CASCADE;


--
-- Name: llm_feature_bindings fk_rails_3303a3b5e4; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_feature_bindings
    ADD CONSTRAINT fk_rails_3303a3b5e4 FOREIGN KEY (llm_connection_id) REFERENCES public.llm_connections(id);


--
-- Name: ldap_departments_synchronized_departments fk_rails_344de1677b; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_departments_synchronized_departments
    ADD CONSTRAINT fk_rails_344de1677b FOREIGN KEY (synchronized_tree_id) REFERENCES public.ldap_departments_synchronized_trees(id);


--
-- Name: hierarchical_items fk_rails_35a1085251; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.hierarchical_items
    ADD CONSTRAINT fk_rails_35a1085251 FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: resource_allocations fk_rails_36384be862; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocations
    ADD CONSTRAINT fk_rails_36384be862 FOREIGN KEY (placeholder_user_id) REFERENCES public.users(id);


--
-- Name: resource_allocations fk_rails_36e194f56b; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocations
    ADD CONSTRAINT fk_rails_36e194f56b FOREIGN KEY (principal_id) REFERENCES public.users(id);


--
-- Name: type_variants fk_rails_3a016bca80; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.type_variants
    ADD CONSTRAINT fk_rails_3a016bca80 FOREIGN KEY (form_configuration_id) REFERENCES public.form_configurations(id) ON DELETE RESTRICT;


--
-- Name: oauth_applications fk_rails_3d1f3b58d2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_applications
    ADD CONSTRAINT fk_rails_3d1f3b58d2 FOREIGN KEY (client_credentials_user_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: gitlab_branches fk_rails_3ee0950fef; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gitlab_branches
    ADD CONSTRAINT fk_rails_3ee0950fef FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: meeting_agenda_items fk_rails_3f56abf49c; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_agenda_items
    ADD CONSTRAINT fk_rails_3f56abf49c FOREIGN KEY (author_id) REFERENCES public.users(id);


--
-- Name: repository_pipelines fk_rails_41318562ae; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_pipelines
    ADD CONSTRAINT fk_rails_41318562ae FOREIGN KEY (merge_request_id) REFERENCES public.repository_merge_requests(id) ON DELETE CASCADE;


--
-- Name: wiki_pages fk_rails_4189064f3f; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_pages
    ADD CONSTRAINT fk_rails_4189064f3f FOREIGN KEY (author_id) REFERENCES public.users(id);


--
-- Name: jira_sprints fk_rails_41c4a89a09; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_sprints
    ADD CONSTRAINT fk_rails_41c4a89a09 FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: persisted_queries fk_rails_42b4970965; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.persisted_queries
    ADD CONSTRAINT fk_rails_42b4970965 FOREIGN KEY (principal_id) REFERENCES public.users(id);


--
-- Name: reminder_notifications fk_rails_4398e72474; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reminder_notifications
    ADD CONSTRAINT fk_rails_4398e72474 FOREIGN KEY (reminder_id) REFERENCES public.reminders(id);


--
-- Name: push_subscriptions fk_rails_43d43720fc; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.push_subscriptions
    ADD CONSTRAINT fk_rails_43d43720fc FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: ldap_groups_synchronized_groups fk_rails_44dac1537e; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ldap_groups_synchronized_groups
    ADD CONSTRAINT fk_rails_44dac1537e FOREIGN KEY (filter_id) REFERENCES public.ldap_groups_synchronized_filters(id);


--
-- Name: repository_branches fk_rails_4505cf9d67; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_branches
    ADD CONSTRAINT fk_rails_4505cf9d67 FOREIGN KEY (provider_id) REFERENCES public.repository_providers(id) ON DELETE CASCADE;


--
-- Name: types fk_rails_46ceaf0e5b; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.types
    ADD CONSTRAINT fk_rails_46ceaf0e5b FOREIGN KEY (color_id) REFERENCES public.colors(id) ON DELETE SET NULL;


--
-- Name: llm_capability_verdicts fk_rails_495d3d2b72; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_capability_verdicts
    ADD CONSTRAINT fk_rails_495d3d2b72 FOREIGN KEY (llm_connection_id) REFERENCES public.llm_connections(id);


--
-- Name: notification_settings fk_rails_496a500fda; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notification_settings
    ADD CONSTRAINT fk_rails_496a500fda FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: notifications fk_rails_4aea6afa11; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT fk_rails_4aea6afa11 FOREIGN KEY (recipient_id) REFERENCES public.users(id);


--
-- Name: jira_issues fk_rails_4b8b163b31; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_issues
    ADD CONSTRAINT fk_rails_4b8b163b31 FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: reminder_notifications fk_rails_4db3d8d95d; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reminder_notifications
    ADD CONSTRAINT fk_rails_4db3d8d95d FOREIGN KEY (notification_id) REFERENCES public.notifications(id);


--
-- Name: export_settings fk_rails_4e2ea236a3; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.export_settings
    ADD CONSTRAINT fk_rails_4e2ea236a3 FOREIGN KEY (query_id) REFERENCES public.queries(id);


--
-- Name: bcf_issues fk_rails_4e35bc3056; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bcf_issues
    ADD CONSTRAINT fk_rails_4e35bc3056 FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: ifc_models fk_rails_4f53d4601c; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ifc_models
    ADD CONSTRAINT fk_rails_4f53d4601c FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: meeting_outcomes fk_rails_53dfa07660; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_outcomes
    ADD CONSTRAINT fk_rails_53dfa07660 FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id);


--
-- Name: jira_users fk_rails_5420508baf; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_users
    ADD CONSTRAINT fk_rails_5420508baf FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: webhooks_logs fk_rails_551257cdac; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_logs
    ADD CONSTRAINT fk_rails_551257cdac FOREIGN KEY (webhooks_webhook_id) REFERENCES public.webhooks_webhooks(id) ON DELETE CASCADE;


--
-- Name: bcf_comments fk_rails_556ef6e73e; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bcf_comments
    ADD CONSTRAINT fk_rails_556ef6e73e FOREIGN KEY (viewpoint_id) REFERENCES public.bcf_viewpoints(id) ON DELETE SET NULL;


--
-- Name: ai_text_transform_runs fk_rails_57d5ddb33e; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_transform_runs
    ADD CONSTRAINT fk_rails_57d5ddb33e FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: jira_attachments fk_rails_5926b7da58; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_attachments
    ADD CONSTRAINT fk_rails_5926b7da58 FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: notifications fk_rails_595318131c; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT fk_rails_595318131c FOREIGN KEY (journal_id) REFERENCES public.journals(id) ON DELETE CASCADE;


--
-- Name: labels fk_rails_5a94290bc0; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.labels
    ADD CONSTRAINT fk_rails_5a94290bc0 FOREIGN KEY (author_id) REFERENCES public.users(id);


--
-- Name: itsm_email_messages fk_rails_5ae4830462; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_email_messages
    ADD CONSTRAINT fk_rails_5ae4830462 FOREIGN KEY (thread_id) REFERENCES public.itsm_email_threads(id) ON DELETE CASCADE;


--
-- Name: time_entry_activities_projects fk_rails_5b669d4f34; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_entry_activities_projects
    ADD CONSTRAINT fk_rails_5b669d4f34 FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: oidc_user_session_links fk_rails_5e6a849f92; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_user_session_links
    ADD CONSTRAINT fk_rails_5e6a849f92 FOREIGN KEY (session_id) REFERENCES public.sessions(id) ON DELETE CASCADE;


--
-- Name: work_packages fk_rails_5edb6f06e6; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_packages
    ADD CONSTRAINT fk_rails_5edb6f06e6 FOREIGN KEY (status_id) REFERENCES public.statuses(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: documents fk_rails_5fe3b44db5; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.documents
    ADD CONSTRAINT fk_rails_5fe3b44db5 FOREIGN KEY (type_id) REFERENCES public.document_types(id);


--
-- Name: meeting_sections fk_rails_613c652e16; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_sections
    ADD CONSTRAINT fk_rails_613c652e16 FOREIGN KEY (meeting_id) REFERENCES public.meetings(id);


--
-- Name: file_links fk_rails_650ebb2e1a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_links
    ADD CONSTRAINT fk_rails_650ebb2e1a FOREIGN KEY (creator_id) REFERENCES public.users(id);


--
-- Name: oauth_client_tokens fk_rails_65a92bfbf4; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_client_tokens
    ADD CONSTRAINT fk_rails_65a92bfbf4 FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: project_phases fk_rails_664dd6ae3c; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_phases
    ADD CONSTRAINT fk_rails_664dd6ae3c FOREIGN KEY (definition_id) REFERENCES public.project_phase_definitions(id);


--
-- Name: workflows_status_transitions fk_rails_66af376b7e; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflows_status_transitions
    ADD CONSTRAINT fk_rails_66af376b7e FOREIGN KEY (new_status_id) REFERENCES public.statuses(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: work_package_versions fk_rails_69eacefd95; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_versions
    ADD CONSTRAINT fk_rails_69eacefd95 FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: group_details fk_rails_6b0aad5675; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_details
    ADD CONSTRAINT fk_rails_6b0aad5675 FOREIGN KEY (parent_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: storages fk_rails_6c69bacb8d; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.storages
    ADD CONSTRAINT fk_rails_6c69bacb8d FOREIGN KEY (creator_id) REFERENCES public.users(id);


--
-- Name: work_package_journals fk_rails_6d30b25fa6; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_journals
    ADD CONSTRAINT fk_rails_6d30b25fa6 FOREIGN KEY (backlog_bucket_id) REFERENCES public.backlog_buckets(id) ON DELETE SET NULL;


--
-- Name: repository_branches fk_rails_6dbffb3909; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_branches
    ADD CONSTRAINT fk_rails_6dbffb3909 FOREIGN KEY (repository_user_id) REFERENCES public.repository_users(id) ON DELETE SET NULL;


--
-- Name: oauth_client_tokens fk_rails_6e922d4135; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_client_tokens
    ADD CONSTRAINT fk_rails_6e922d4135 FOREIGN KEY (oauth_client_id) REFERENCES public.oauth_clients(id) ON DELETE CASCADE;


--
-- Name: scim_clients fk_rails_705242b5b2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.scim_clients
    ADD CONSTRAINT fk_rails_705242b5b2 FOREIGN KEY (auth_provider_id) REFERENCES public.auth_providers(id) ON DELETE RESTRICT;


--
-- Name: time_entries fk_rails_709864c72f; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_entries
    ADD CONSTRAINT fk_rails_709864c72f FOREIGN KEY (logged_by_id) REFERENCES public.users(id);


--
-- Name: jira_statuses fk_rails_7112fe0df6; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_statuses
    ADD CONSTRAINT fk_rails_7112fe0df6 FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: recurring_meetings fk_rails_718ca7b915; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_meetings
    ADD CONSTRAINT fk_rails_718ca7b915 FOREIGN KEY (author_id) REFERENCES public.users(id);


--
-- Name: resource_allocation_journals fk_rails_72d063b8c6; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocation_journals
    ADD CONSTRAINT fk_rails_72d063b8c6 FOREIGN KEY (reviewed_by_id) REFERENCES public.users(id);


--
-- Name: persisted_views fk_rails_72d1b289e5; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.persisted_views
    ADD CONSTRAINT fk_rails_72d1b289e5 FOREIGN KEY (parent_id) REFERENCES public.persisted_views(id);


--
-- Name: oauth_access_tokens fk_rails_732cb83ab7; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_access_tokens
    ADD CONSTRAINT fk_rails_732cb83ab7 FOREIGN KEY (application_id) REFERENCES public.oauth_applications(id);


--
-- Name: type_variants fk_rails_7335074917; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.type_variants
    ADD CONSTRAINT fk_rails_7335074917 FOREIGN KEY (project_attributes_source_id) REFERENCES public.type_variants(id) ON DELETE RESTRICT;


--
-- Name: work_package_categories fk_rails_73c869a542; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_categories
    ADD CONSTRAINT fk_rails_73c869a542 FOREIGN KEY (category_id) REFERENCES public.categories(id) ON DELETE CASCADE;


--
-- Name: last_project_folders fk_rails_73e1c678f1; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.last_project_folders
    ADD CONSTRAINT fk_rails_73e1c678f1 FOREIGN KEY (project_storage_id) REFERENCES public.project_storages(id) ON DELETE CASCADE;


--
-- Name: resource_allocations fk_rails_73ec109120; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocations
    ADD CONSTRAINT fk_rails_73ec109120 FOREIGN KEY (reviewed_by_id) REFERENCES public.users(id);


--
-- Name: repository_issues fk_rails_77530ef3a5; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_issues
    ADD CONSTRAINT fk_rails_77530ef3a5 FOREIGN KEY (provider_id) REFERENCES public.repository_providers(id) ON DELETE CASCADE;


--
-- Name: work_package_versions fk_rails_78bc6965e6; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_versions
    ADD CONSTRAINT fk_rails_78bc6965e6 FOREIGN KEY (version_id) REFERENCES public.versions(id) ON DELETE CASCADE;


--
-- Name: itsm_incoming_mailboxes fk_rails_7947e2f5bd; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_incoming_mailboxes
    ADD CONSTRAINT fk_rails_7947e2f5bd FOREIGN KEY (mail_server_id) REFERENCES public.itsm_mail_servers(id) ON DELETE RESTRICT;


--
-- Name: project_custom_field_project_mappings fk_rails_79aa0057e4; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_custom_field_project_mappings
    ADD CONSTRAINT fk_rails_79aa0057e4 FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: bcf_comments fk_rails_7ac870008c; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bcf_comments
    ADD CONSTRAINT fk_rails_7ac870008c FOREIGN KEY (issue_id) REFERENCES public.bcf_issues(id) ON DELETE CASCADE;


--
-- Name: jira_fields fk_rails_7b478b0222; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_fields
    ADD CONSTRAINT fk_rails_7b478b0222 FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: repository_pipelines fk_rails_7ba1558074; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_pipelines
    ADD CONSTRAINT fk_rails_7ba1558074 FOREIGN KEY (provider_id) REFERENCES public.repository_providers(id) ON DELETE CASCADE;


--
-- Name: repository_merge_request_work_packages fk_rails_7c17240006; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_merge_request_work_packages
    ADD CONSTRAINT fk_rails_7c17240006 FOREIGN KEY (merge_request_id) REFERENCES public.repository_merge_requests(id) ON DELETE CASCADE;


--
-- Name: project_phases fk_rails_7c456fca1d; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_phases
    ADD CONSTRAINT fk_rails_7c456fca1d FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: cost_types_projects fk_rails_7e3e53cf92; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cost_types_projects
    ADD CONSTRAINT fk_rails_7e3e53cf92 FOREIGN KEY (cost_type_id) REFERENCES public.cost_types(id);


--
-- Name: project_types fk_rails_7e840e25b4; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_types
    ADD CONSTRAINT fk_rails_7e840e25b4 FOREIGN KEY (type_id) REFERENCES public.types(id) ON DELETE CASCADE;


--
-- Name: remote_identities fk_rails_7f8585fc1a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.remote_identities
    ADD CONSTRAINT fk_rails_7f8585fc1a FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: emoji_reactions fk_rails_80a9e7c3ce; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.emoji_reactions
    ADD CONSTRAINT fk_rails_80a9e7c3ce FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: label_journals fk_rails_8409d29d70; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.label_journals
    ADD CONSTRAINT fk_rails_8409d29d70 FOREIGN KEY (journal_id) REFERENCES public.journals(id);


--
-- Name: push_subscriptions fk_rails_855264bb6c; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.push_subscriptions
    ADD CONSTRAINT fk_rails_855264bb6c FOREIGN KEY (session_id) REFERENCES public.sessions(id) ON DELETE SET NULL;


--
-- Name: llm_models fk_rails_85ea77286d; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_models
    ADD CONSTRAINT fk_rails_85ea77286d FOREIGN KEY (llm_connection_id) REFERENCES public.llm_connections(id);


--
-- Name: repository_users fk_rails_85eb72307c; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_users
    ADD CONSTRAINT fk_rails_85eb72307c FOREIGN KEY (provider_id) REFERENCES public.repository_providers(id) ON DELETE CASCADE;


--
-- Name: recaptcha_entries fk_rails_890a90efa9; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recaptcha_entries
    ADD CONSTRAINT fk_rails_890a90efa9 FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: ai_text_transform_run_events fk_rails_8a478af266; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_transform_run_events
    ADD CONSTRAINT fk_rails_8a478af266 FOREIGN KEY (run_id) REFERENCES public.ai_text_transform_runs(id) ON DELETE CASCADE;


--
-- Name: squashtm_project_links fk_rails_8c9d0ae5d6; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.squashtm_project_links
    ADD CONSTRAINT fk_rails_8c9d0ae5d6 FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: jira_import_transitions fk_rails_8cb4c7670b; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_import_transitions
    ADD CONSTRAINT fk_rails_8cb4c7670b FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id);


--
-- Name: oidc_group_links fk_rails_8d031325e2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_group_links
    ADD CONSTRAINT fk_rails_8d031325e2 FOREIGN KEY (group_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: repository_merge_requests fk_rails_8d84aef20e; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_merge_requests
    ADD CONSTRAINT fk_rails_8d84aef20e FOREIGN KEY (provider_id) REFERENCES public.repository_providers(id) ON DELETE CASCADE;


--
-- Name: work_package_version_journals fk_rails_8dfca613e3; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_version_journals
    ADD CONSTRAINT fk_rails_8dfca613e3 FOREIGN KEY (journal_id) REFERENCES public.journals(id);


--
-- Name: calculated_value_errors fk_rails_8e54029654; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calculated_value_errors
    ADD CONSTRAINT fk_rails_8e54029654 FOREIGN KEY (custom_field_id) REFERENCES public.custom_fields(id);


--
-- Name: work_package_journals fk_rails_8fea0643dc; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_package_journals
    ADD CONSTRAINT fk_rails_8fea0643dc FOREIGN KEY (sprint_id) REFERENCES public.sprints(id) ON DELETE SET NULL;


--
-- Name: custom_comments fk_rails_8ff02f1f5a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_comments
    ADD CONSTRAINT fk_rails_8ff02f1f5a FOREIGN KEY (custom_field_id) REFERENCES public.custom_fields(id);


--
-- Name: meeting_agenda_items fk_rails_9089886cdd; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_agenda_items
    ADD CONSTRAINT fk_rails_9089886cdd FOREIGN KEY (meeting_id) REFERENCES public.meetings(id);


--
-- Name: work_packages fk_rails_931ad309e8; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_packages
    ADD CONSTRAINT fk_rails_931ad309e8 FOREIGN KEY (project_id) REFERENCES public.projects(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: autologin_session_links fk_rails_93f00f2aab; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.autologin_session_links
    ADD CONSTRAINT fk_rails_93f00f2aab FOREIGN KEY (token_id) REFERENCES public.tokens(id) ON DELETE CASCADE;


--
-- Name: custom_fields_roles fk_rails_950d979cb9; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_fields_roles
    ADD CONSTRAINT fk_rails_950d979cb9 FOREIGN KEY (role_id) REFERENCES public.roles(id);


--
-- Name: jira_boards fk_rails_9560a5e056; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_boards
    ADD CONSTRAINT fk_rails_9560a5e056 FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: project_storages fk_rails_96ab713fe3; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_storages
    ADD CONSTRAINT fk_rails_96ab713fe3 FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: custom_fields_roles fk_rails_96bf507f15; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_fields_roles
    ADD CONSTRAINT fk_rails_96bf507f15 FOREIGN KEY (custom_field_id) REFERENCES public.custom_fields(id);


--
-- Name: projects fk_rails_9702eec5cd; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.projects
    ADD CONSTRAINT fk_rails_9702eec5cd FOREIGN KEY (template_id) REFERENCES public.projects(id);


--
-- Name: project_types fk_rails_98355250ac; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_types
    ADD CONSTRAINT fk_rails_98355250ac FOREIGN KEY (variant_id) REFERENCES public.type_variants(id) ON DELETE RESTRICT;


--
-- Name: cost_types_projects fk_rails_98dd8cde20; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cost_types_projects
    ADD CONSTRAINT fk_rails_98dd8cde20 FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: itsm_email_threads fk_rails_98ee43f06e; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_email_threads
    ADD CONSTRAINT fk_rails_98ee43f06e FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: user_auth_provider_links fk_rails_9a2c7082f0; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_auth_provider_links
    ADD CONSTRAINT fk_rails_9a2c7082f0 FOREIGN KEY (user_id) REFERENCES public.users(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: sprint_goals fk_rails_9a4c4e3002; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sprint_goals
    ADD CONSTRAINT fk_rails_9a4c4e3002 FOREIGN KEY (sprint_id) REFERENCES public.sprints(id) ON DELETE CASCADE;


--
-- Name: meeting_agenda_item_journals fk_rails_9b6296a185; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_agenda_item_journals
    ADD CONSTRAINT fk_rails_9b6296a185 FOREIGN KEY (presenter_id) REFERENCES public.users(id);


--
-- Name: repository_commits fk_rails_9e313fc3ae; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_commits
    ADD CONSTRAINT fk_rails_9e313fc3ae FOREIGN KEY (provider_id) REFERENCES public.repository_providers(id) ON DELETE CASCADE;


--
-- Name: type_variants fk_rails_9ec736b558; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.type_variants
    ADD CONSTRAINT fk_rails_9ec736b558 FOREIGN KEY (pdf_export_source_id) REFERENCES public.type_variants(id) ON DELETE RESTRICT;


--
-- Name: meeting_outcomes fk_rails_9f4a5cc9ed; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_outcomes
    ADD CONSTRAINT fk_rails_9f4a5cc9ed FOREIGN KEY (author_id) REFERENCES public.users(id);


--
-- Name: recurring_meetings fk_rails_a0ea141e10; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_meetings
    ADD CONSTRAINT fk_rails_a0ea141e10 FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: webhooks_events fk_rails_a166925c91; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_events
    ADD CONSTRAINT fk_rails_a166925c91 FOREIGN KEY (webhooks_webhook_id) REFERENCES public.webhooks_webhooks(id);


--
-- Name: file_links fk_rails_a29c1fb981; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_links
    ADD CONSTRAINT fk_rails_a29c1fb981 FOREIGN KEY (storage_id) REFERENCES public.storages(id) ON DELETE CASCADE;


--
-- Name: persisted_views fk_rails_a2b170e589; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.persisted_views
    ADD CONSTRAINT fk_rails_a2b170e589 FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: workflows_status_transitions fk_rails_a7407e0a99; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflows_status_transitions
    ADD CONSTRAINT fk_rails_a7407e0a99 FOREIGN KEY (workflow_id) REFERENCES public.workflows(id) ON DELETE CASCADE;


--
-- Name: user_working_hours fk_rails_a8fd8c1a6c; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_working_hours
    ADD CONSTRAINT fk_rails_a8fd8c1a6c FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: user_auth_provider_links fk_rails_aae2d958bb; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_auth_provider_links
    ADD CONSTRAINT fk_rails_aae2d958bb FOREIGN KEY (auth_provider_id) REFERENCES public.auth_providers(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: jira_issue_types fk_rails_ab3c6622a8; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_issue_types
    ADD CONSTRAINT fk_rails_ab3c6622a8 FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: automation_triggers fk_rails_ab433484eb; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automation_triggers
    ADD CONSTRAINT fk_rails_ab433484eb FOREIGN KEY (automation_id) REFERENCES public.automations(id);


--
-- Name: backlog_excluded_types fk_rails_abe8a31e8f; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.backlog_excluded_types
    ADD CONSTRAINT fk_rails_abe8a31e8f FOREIGN KEY (type_id) REFERENCES public.types(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: wiki_page_links fk_rails_ac654ceda2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_page_links
    ADD CONSTRAINT fk_rails_ac654ceda2 FOREIGN KEY (author_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: form_configuration_groups fk_rails_ac8202a088; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configuration_groups
    ADD CONSTRAINT fk_rails_ac8202a088 FOREIGN KEY (query_id) REFERENCES public.queries(id) ON DELETE CASCADE;


--
-- Name: tokens fk_rails_ac8a5d0441; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tokens
    ADD CONSTRAINT fk_rails_ac8a5d0441 FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: project_storages fk_rails_acca00a591; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_storages
    ADD CONSTRAINT fk_rails_acca00a591 FOREIGN KEY (creator_id) REFERENCES public.users(id);


--
-- Name: resource_allocation_scheduled_entries fk_rails_ad93a0f4f1; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocation_scheduled_entries
    ADD CONSTRAINT fk_rails_ad93a0f4f1 FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: llm_connections fk_rails_ae26490ab2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_connections
    ADD CONSTRAINT fk_rails_ae26490ab2 FOREIGN KEY (default_chat_model_id) REFERENCES public.llm_models(id) ON DELETE SET NULL;


--
-- Name: jira_import_job_cursors fk_rails_afe16225a4; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_import_job_cursors
    ADD CONSTRAINT fk_rails_afe16225a4 FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: subproject_template_assignments fk_rails_b15b68fdaf; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.subproject_template_assignments
    ADD CONSTRAINT fk_rails_b15b68fdaf FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: jira_imports fk_rails_b173c878dd; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_imports
    ADD CONSTRAINT fk_rails_b173c878dd FOREIGN KEY (jira_id) REFERENCES public.jiras(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: llm_feature_bindings fk_rails_b2fc230914; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.llm_feature_bindings
    ADD CONSTRAINT fk_rails_b2fc230914 FOREIGN KEY (semantic_index_id) REFERENCES public.semantic_indexes(id);


--
-- Name: jira_versions fk_rails_b4544f6521; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_versions
    ADD CONSTRAINT fk_rails_b4544f6521 FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: workflows_status_transitions fk_rails_b4628cffdf; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.workflows_status_transitions
    ADD CONSTRAINT fk_rails_b4628cffdf FOREIGN KEY (old_status_id) REFERENCES public.statuses(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: oauth_access_grants fk_rails_b4b53e07b8; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_access_grants
    ADD CONSTRAINT fk_rails_b4b53e07b8 FOREIGN KEY (application_id) REFERENCES public.oauth_applications(id);


--
-- Name: custom_comment_journals fk_rails_b555b490bf; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_comment_journals
    ADD CONSTRAINT fk_rails_b555b490bf FOREIGN KEY (journal_id) REFERENCES public.journals(id);


--
-- Name: type_variants fk_rails_b57f8881c5; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.type_variants
    ADD CONSTRAINT fk_rails_b57f8881c5 FOREIGN KEY (workflow_id) REFERENCES public.workflows(id) ON DELETE RESTRICT;


--
-- Name: backlog_excluded_types fk_rails_b6a3d17c0d; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.backlog_excluded_types
    ADD CONSTRAINT fk_rails_b6a3d17c0d FOREIGN KEY (project_id) REFERENCES public.projects(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: repository_commits fk_rails_b7317327f6; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_commits
    ADD CONSTRAINT fk_rails_b7317327f6 FOREIGN KEY (repository_user_id) REFERENCES public.repository_users(id) ON DELETE SET NULL;


--
-- Name: itsm_outgoing_mailboxes fk_rails_b7352e622a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_outgoing_mailboxes
    ADD CONSTRAINT fk_rails_b7352e622a FOREIGN KEY (mail_server_id) REFERENCES public.itsm_mail_servers(id) ON DELETE RESTRICT;


--
-- Name: reminders fk_rails_b7e32f12b2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reminders
    ADD CONSTRAINT fk_rails_b7e32f12b2 FOREIGN KEY (creator_id) REFERENCES public.users(id);


--
-- Name: repository_pipelines fk_rails_bae161f41a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_pipelines
    ADD CONSTRAINT fk_rails_bae161f41a FOREIGN KEY (repository_user_id) REFERENCES public.repository_users(id) ON DELETE SET NULL;


--
-- Name: jira_versions fk_rails_bae92bda55; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_versions
    ADD CONSTRAINT fk_rails_bae92bda55 FOREIGN KEY (jira_project_id) REFERENCES public.jira_projects(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: time_entry_activities_projects fk_rails_bc6c409022; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_entry_activities_projects
    ADD CONSTRAINT fk_rails_bc6c409022 FOREIGN KEY (activity_id) REFERENCES public.enumerations(id);


--
-- Name: resource_allocation_journals fk_rails_bcaf3160c9; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocation_journals
    ADD CONSTRAINT fk_rails_bcaf3160c9 FOREIGN KEY (principal_id) REFERENCES public.users(id);


--
-- Name: user_non_working_times fk_rails_bcba7363b5; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_non_working_times
    ADD CONSTRAINT fk_rails_bcba7363b5 FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: work_packages fk_rails_c069c849fc; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_packages
    ADD CONSTRAINT fk_rails_c069c849fc FOREIGN KEY (sprint_id) REFERENCES public.sprints(id) ON DELETE SET NULL;


--
-- Name: automation_actions fk_rails_c0caeb759a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.automation_actions
    ADD CONSTRAINT fk_rails_c0caeb759a FOREIGN KEY (automation_id) REFERENCES public.automations(id);


--
-- Name: persisted_views fk_rails_c0ce56206c; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.persisted_views
    ADD CONSTRAINT fk_rails_c0ce56206c FOREIGN KEY (principal_id) REFERENCES public.users(id);


--
-- Name: hierarchical_items fk_rails_c67cbe751d; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.hierarchical_items
    ADD CONSTRAINT fk_rails_c67cbe751d FOREIGN KEY (custom_field_id) REFERENCES public.custom_fields(id);


--
-- Name: custom_field_project_specific_value_restrictions fk_rails_ca9147c34c; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_field_project_specific_value_restrictions
    ADD CONSTRAINT fk_rails_ca9147c34c FOREIGN KEY (custom_option_id) REFERENCES public.custom_options(id) ON DELETE CASCADE;


--
-- Name: jira_projects fk_rails_cb054e1117; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_projects
    ADD CONSTRAINT fk_rails_cb054e1117 FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: ai_text_action_types fk_rails_cba94f5cbf; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_text_action_types
    ADD CONSTRAINT fk_rails_cba94f5cbf FOREIGN KEY (ai_text_action_id) REFERENCES public.ai_text_actions(id) ON DELETE CASCADE;


--
-- Name: oauth_applications fk_rails_cc886e315a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_applications
    ADD CONSTRAINT fk_rails_cc886e315a FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: recurring_meeting_interim_responses fk_rails_cd46c27804; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_meeting_interim_responses
    ADD CONSTRAINT fk_rails_cd46c27804 FOREIGN KEY (recurring_meeting_id) REFERENCES public.recurring_meetings(id);


--
-- Name: oidc_group_memberships fk_rails_cefb836ee1; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oidc_group_memberships
    ADD CONSTRAINT fk_rails_cefb836ee1 FOREIGN KEY (auth_provider_id) REFERENCES public.auth_providers(id) ON DELETE CASCADE;


--
-- Name: jira_attachments fk_rails_cfe8207e10; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_attachments
    ADD CONSTRAINT fk_rails_cfe8207e10 FOREIGN KEY (jira_issue_id) REFERENCES public.jira_issues(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: favorites fk_rails_d15744e438; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.favorites
    ADD CONSTRAINT fk_rails_d15744e438 FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: sprint_goals fk_rails_d18b03b2a1; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sprint_goals
    ADD CONSTRAINT fk_rails_d18b03b2a1 FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: jira_priorities fk_rails_d236f4efb2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_priorities
    ADD CONSTRAINT fk_rails_d236f4efb2 FOREIGN KEY (jira_import_id) REFERENCES public.jira_imports(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: persisted_queries fk_rails_d2ead6efb1; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.persisted_queries
    ADD CONSTRAINT fk_rails_d2ead6efb1 FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: webhooks_projects fk_rails_d7ea5de5b8; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_projects
    ADD CONSTRAINT fk_rails_d7ea5de5b8 FOREIGN KEY (webhooks_webhook_id) REFERENCES public.webhooks_webhooks(id);


--
-- Name: meeting_outcomes fk_rails_d89fa3e833; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_outcomes
    ADD CONSTRAINT fk_rails_d89fa3e833 FOREIGN KEY (meeting_agenda_item_id) REFERENCES public.meeting_agenda_items(id);


--
-- Name: backlog_buckets fk_rails_d8ed92265a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.backlog_buckets
    ADD CONSTRAINT fk_rails_d8ed92265a FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: custom_field_project_specific_value_restrictions fk_rails_dac692ce18; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_field_project_specific_value_restrictions
    ADD CONSTRAINT fk_rails_dac692ce18 FOREIGN KEY (hierarchy_item_id) REFERENCES public.hierarchical_items(id) ON DELETE CASCADE;


--
-- Name: repository_issue_work_packages fk_rails_db5b277bf8; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_issue_work_packages
    ADD CONSTRAINT fk_rails_db5b277bf8 FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: subproject_template_assignments fk_rails_db83d82aa3; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.subproject_template_assignments
    ADD CONSTRAINT fk_rails_db83d82aa3 FOREIGN KEY (template_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: storages_file_links_journals fk_rails_e007095e78; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.storages_file_links_journals
    ADD CONSTRAINT fk_rails_e007095e78 FOREIGN KEY (journal_id) REFERENCES public.journals(id);


--
-- Name: ical_token_query_assignments fk_rails_e0ecbb71e6; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ical_token_query_assignments
    ADD CONSTRAINT fk_rails_e0ecbb71e6 FOREIGN KEY (ical_token_id) REFERENCES public.tokens(id) ON DELETE CASCADE;


--
-- Name: wiki_page_links fk_rails_e3041921e1; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wiki_page_links
    ADD CONSTRAINT fk_rails_e3041921e1 FOREIGN KEY (provider_id) REFERENCES public.wiki_providers(id) ON DELETE CASCADE;


--
-- Name: resource_allocations fk_rails_e3640d2cc4; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocations
    ADD CONSTRAINT fk_rails_e3640d2cc4 FOREIGN KEY (principal_assigned_by_id) REFERENCES public.users(id);


--
-- Name: custom_fields_projects fk_rails_e51cefe60d; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_fields_projects
    ADD CONSTRAINT fk_rails_e51cefe60d FOREIGN KEY (custom_field_id) REFERENCES public.custom_fields(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: work_packages fk_rails_e5591391e7; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_packages
    ADD CONSTRAINT fk_rails_e5591391e7 FOREIGN KEY (backlog_bucket_id) REFERENCES public.backlog_buckets(id) ON DELETE SET NULL;


--
-- Name: recurring_meeting_historic_schedules fk_rails_e6500fff9a; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_meeting_historic_schedules
    ADD CONSTRAINT fk_rails_e6500fff9a FOREIGN KEY (recurring_meeting_id) REFERENCES public.recurring_meetings(id);


--
-- Name: sprints fk_rails_e8206c9686; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sprints
    ADD CONSTRAINT fk_rails_e8206c9686 FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: webhooks_projects fk_rails_e978b5e3d7; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks_projects
    ADD CONSTRAINT fk_rails_e978b5e3d7 FOREIGN KEY (project_id) REFERENCES public.projects(id);


--
-- Name: ordered_work_packages fk_rails_e99c4d5dfe; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ordered_work_packages
    ADD CONSTRAINT fk_rails_e99c4d5dfe FOREIGN KEY (query_id) REFERENCES public.queries(id) ON DELETE CASCADE;


--
-- Name: itsm_incoming_mailboxes fk_rails_e9a191c7f0; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_incoming_mailboxes
    ADD CONSTRAINT fk_rails_e9a191c7f0 FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: auth_providers fk_rails_e9bd348863; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.auth_providers
    ADD CONSTRAINT fk_rails_e9bd348863 FOREIGN KEY (creator_id) REFERENCES public.users(id);


--
-- Name: message_work_packages fk_rails_ea63c99a25; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_work_packages
    ADD CONSTRAINT fk_rails_ea63c99a25 FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: project_types fk_rails_ebe6f714e2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.project_types
    ADD CONSTRAINT fk_rails_ebe6f714e2 FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: custom_field_project_specific_value_restrictions fk_rails_eded5a8a68; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.custom_field_project_specific_value_restrictions
    ADD CONSTRAINT fk_rails_eded5a8a68 FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: views fk_rails_ef3c430897; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.views
    ADD CONSTRAINT fk_rails_ef3c430897 FOREIGN KEY (query_id) REFERENCES public.queries(id);


--
-- Name: labelings fk_rails_f0265aa0ad; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.labelings
    ADD CONSTRAINT fk_rails_f0265aa0ad FOREIGN KEY (label_id) REFERENCES public.labels(id) ON DELETE CASCADE;


--
-- Name: work_packages fk_rails_f2a8977aa1; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.work_packages
    ADD CONSTRAINT fk_rails_f2a8977aa1 FOREIGN KEY (type_id) REFERENCES public.types(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: itsm_outgoing_mailboxes fk_rails_f3341dbcb8; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itsm_outgoing_mailboxes
    ADD CONSTRAINT fk_rails_f3341dbcb8 FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: resource_allocation_journals fk_rails_f3d51af541; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resource_allocation_journals
    ADD CONSTRAINT fk_rails_f3d51af541 FOREIGN KEY (requested_by_id) REFERENCES public.users(id);


--
-- Name: jira_issues fk_rails_f5dae00480; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jira_issues
    ADD CONSTRAINT fk_rails_f5dae00480 FOREIGN KEY (jira_project_id) REFERENCES public.jira_projects(id) ON UPDATE CASCADE ON DELETE CASCADE;


--
-- Name: ical_token_query_assignments fk_rails_f5e934437b; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ical_token_query_assignments
    ADD CONSTRAINT fk_rails_f5e934437b FOREIGN KEY (query_id) REFERENCES public.queries(id) ON DELETE CASCADE;


--
-- Name: time_entry_journals fk_rails_f6e3d60ab5; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_entry_journals
    ADD CONSTRAINT fk_rails_f6e3d60ab5 FOREIGN KEY (logged_by_id) REFERENCES public.users(id);


--
-- Name: form_configuration_attributes fk_rails_f80319ebf8; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configuration_attributes
    ADD CONSTRAINT fk_rails_f80319ebf8 FOREIGN KEY (custom_field_id) REFERENCES public.custom_fields(id) ON DELETE CASCADE;


--
-- Name: repository_commit_work_packages fk_rails_f8a6e917e2; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_commit_work_packages
    ADD CONSTRAINT fk_rails_f8a6e917e2 FOREIGN KEY (commit_id) REFERENCES public.repository_commits(id) ON DELETE CASCADE;


--
-- Name: type_variants fk_rails_f95f06071b; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.type_variants
    ADD CONSTRAINT fk_rails_f95f06071b FOREIGN KEY (project_id) REFERENCES public.projects(id) ON DELETE CASCADE;


--
-- Name: repository_commit_work_packages fk_rails_f9b16e08ea; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_commit_work_packages
    ADD CONSTRAINT fk_rails_f9b16e08ea FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: bcf_viewpoints fk_rails_fa5c88e5be; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bcf_viewpoints
    ADD CONSTRAINT fk_rails_fa5c88e5be FOREIGN KEY (issue_id) REFERENCES public.bcf_issues(id) ON DELETE CASCADE;


--
-- Name: ordered_work_packages fk_rails_fe038e4e03; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ordered_work_packages
    ADD CONSTRAINT fk_rails_fe038e4e03 FOREIGN KEY (work_package_id) REFERENCES public.work_packages(id) ON DELETE CASCADE;


--
-- Name: form_configuration_groups fk_rails_fe67502434; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.form_configuration_groups
    ADD CONSTRAINT fk_rails_fe67502434 FOREIGN KEY (form_configuration_id) REFERENCES public.form_configurations(id) ON DELETE CASCADE;


--
-- PostgreSQL database dump complete
--

SET search_path TO "$user", public;

INSERT INTO "schema_migrations" (version) VALUES
('20261117090000'),
('20261116120000'),
('20261113101140'),
('20261111120000'),
('20261111093742'),
('20261110120000'),
('20261110082955'),
('20261109232225'),
('20261109104602'),
('20261106165823'),
('20261105092030'),
('20261102130117'),
('20261102040000'),
('20261101120015'),
('20261101094500'),
('20261029120000'),
('20261028145933'),
('20261028133701'),
('20261028133700'),
('20261028124032'),
('20261028123133'),
('20261028100000'),
('20261027160000'),
('20261027090000'),
('20261025180000'),
('20261025160000'),
('20261025140000'),
('20261023100000'),
('20261023073211'),
('20261021120000'),
('20261021090000'),
('20261020153413'),
('20261020122520'),
('20261020100000'),
('20261019140000'),
('20261019124852'),
('20261016101539'),
('20261015143700'),
('20261015133706'),
('20261015133705'),
('20261015133704'),
('20261015133703'),
('20261015133702'),
('20261015133701'),
('20261015133700'),
('20261015120000'),
('20261015110000'),
('20261015100000'),
('20261014090000'),
('20261012120000'),
('20261012110000'),
('20261012100000'),
('20261012090000'),
('20261012073000'),
('20261012070000'),
('20261010100000'),
('20261010080000'),
('20261010070000'),
('20261009120000'),
('20261009100000'),
('20261007145959'),
('20261006120000'),
('20261006100000'),
('20261001120000'),
('20261001110000'),
('20260930185012'),
('20260930090428'),
('20260930084536'),
('20260925150300'),
('20260925150100'),
('20260925150000'),
('20260925100000'),
('20260924120000'),
('20260921100000'),
('20260918125750'),
('20260917165643'),
('20260917120000'),
('20260917101500'),
('20260917100000'),
('20260916140000'),
('20260916133701'),
('20260916133700'),
('20260916130000'),
('20260916120000'),
('20260914194914'),
('20260914193956'),
('20260914120000'),
('20260911142735'),
('20260911130000'),
('20260911112833'),
('20260910151136'),
('20260910120000'),
('20260909140000'),
('20260909090000'),
('20260907120000'),
('20260907113318'),
('20260903120000'),
('20260903090000'),
('20260902070000'),
('20260831130000'),
('20260831120000'),
('20260831100000'),
('20260825103847'),
('20260824132100'),
('20260820101159'),
('20260820101109'),
('20260819140000'),
('20260819090000'),
('20260817133900'),
('20260814153519'),
('20260814140153'),
('20260813120000'),
('20260807145327'),
('20260807135349'),
('20260807133805'),
('20260807133737'),
('20260807100000'),
('20260805122307'),
('20260804123449'),
('20260804123419'),
('20260730133700'),
('20260728120000'),
('20260727142001'),
('20260727111530'),
('20260720142222'),
('20260708203257'),
('20260708100831'),
('20260707120000'),
('20260707090000'),
('20260706104133'),
('20260706065400'),
('20260703151859'),
('20260629134641'),
('20260626095803'),
('20260625120000'),
('20260625073349'),
('20260624135703'),
('20260624120000'),
('20260624101934'),
('20260623113435'),
('20260622151245'),
('20260622150055'),
('20260622144833'),
('20260622100448'),
('20260617073806'),
('20260615120000'),
('20260615000000'),
('20260609120000'),
('20260608120000'),
('20260605094204'),
('20260604120001'),
('20260603112259'),
('20260602170908'),
('20260602152807'),
('20260602152805'),
('20260601081943'),
('20260528144922'),
('20260528120000'),
('20260519144514'),
('20260519120000'),
('20260518000000'),
('20260515101148'),
('20260514103000'),
('20260513120000'),
('20260507141533'),
('20260506120000'),
('20260506000001'),
('20260505143851'),
('20260505143850'),
('20260505143849'),
('20260505143848'),
('20260505143847'),
('20260505143846'),
('20260505143845'),
('20260505143844'),
('20260505143843'),
('20260505143842'),
('20260505120000'),
('20260504154415'),
('20260428133700'),
('20260427125502'),
('20260424094655'),
('20260422131715'),
('20260422110809'),
('20260422084824'),
('20260422081810'),
('20260422081417'),
('20260420160236'),
('20260416213200'),
('20260415211819'),
('20260415182824'),
('20260410100001'),
('20260410100000'),
('20260402094525'),
('20260330134729'),
('20260330100000'),
('20260328120000'),
('20260327140627'),
('20260325133701'),
('20260325101553'),
('20260319120000'),
('20260318133702'),
('20260318133701'),
('20260313170000'),
('20260313164539'),
('20260313163122'),
('20260313120000'),
('20260312121938'),
('20260311183000'),
('20260309130829'),
('20260309105243'),
('20260305175656'),
('20260304164536'),
('20260304160505'),
('20260303172517'),
('20260302152255'),
('20260227164504'),
('20260227143618'),
('20260226133700'),
('20260223142025'),
('20260219152519'),
('20260219151850'),
('20260218122039'),
('20260218095806'),
('20260212145213'),
('20260212133700'),
('20260203171223'),
('20260202150253'),
('20260202150252'),
('20260202093215'),
('20260130105057'),
('20260129120330'),
('20260122130916'),
('20260108111032'),
('20260106151226'),
('20260105101148'),
('20260105100429'),
('20260105095917'),
('20260105095152'),
('20260105093729'),
('20260105093056'),
('20260105092419'),
('20260105090537'),
('20251218133700'),
('20251218100741'),
('20251218100721'),
('20251218100700'),
('20251217105019'),
('20251216155857'),
('20251211160744'),
('20251209191935'),
('20251204134427'),
('20251204100529'),
('20251126092758'),
('20251125000000'),
('20251121073442'),
('20251119095014'),
('20251117102116'),
('20251115211211'),
('20251114174545'),
('20251111155829'),
('20251111145039'),
('20251107114245'),
('20251105082430'),
('20251105082400'),
('20251104151450'),
('20251103123548'),
('20251031071403'),
('20251029134217'),
('20251024140438'),
('20251022114752'),
('20251021133601'),
('20251017111720'),
('20250929070310'),
('20250927215339'),
('20250923142124'),
('20250923120330'),
('20250923064241'),
('20250910085916'),
('20250908172222'),
('20250908151957'),
('20250908072653'),
('20250905204438'),
('20250821092618'),
('20250820131839'),
('20250819152511'),
('20250818133654'),
('20250811102200'),
('20250808062421'),
('20250806132912'),
('20250804133700'),
('20250804083253'),
('20250731144436'),
('20250714142919'),
('20250711133700'),
('20250710133700'),
('20250709103506'),
('20250709090813'),
('20250703143214'),
('20250627121119'),
('20250624155721'),
('20250613141234'),
('20250612133700'),
('20250610111413'),
('20250605133700'),
('20250526155610'),
('20250512114003'),
('20250428135623'),
('20250428114958'),
('20250423123519'),
('20250422072119'),
('20250416141059'),
('20250416112143'),
('20250416095154'),
('20250411104802'),
('20250404061503'),
('20250404060850'),
('20250403150639'),
('20250210184018'),
('20241202112409'),
('20241202112408'),
('20241202112407'),
('20241202112406'),
('20240306154737'),
('20240306154736'),
('20240306154735'),
('20240306154734'),
('20240123151252'),
('20240123151251'),
('20240123151250'),
('20240123151249'),
('20240123151248'),
('20240123151247'),
('20240123151246'),
('1022015'),
('1021015'),
('1020015'),
('1019015'),
('1018016'),
('1017016'),
('1016015'),
('1015015'),
('1014015'),
('1013015'),
('1012015'),
('1011015'),
('1010015'),
('1009016'),
('1008015'),
('1007015'),
('1006015'),
('1005015'),
('1004015'),
('1003016'),
('1002015'),
('1001015'),
('1000016');

