--
-- PostgreSQL database dump
--

SET statement_timeout = 0;
SELECT pg_catalog.set_config('search_path', '', false);

CREATE TYPE public.order_status AS ENUM ('pending', 'paid', 'it''s done');

CREATE TABLE public.users (
    id integer NOT NULL,
    email character varying(255) NOT NULL,
    name text,
    balance numeric(10,2) DEFAULT 0.00 NOT NULL,
    active boolean DEFAULT true,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    tags text[],
    uid uuid DEFAULT gen_random_uuid(),
    CONSTRAINT users_balance_check CHECK ((balance >= (0)::numeric))
);

CREATE TABLE public.orders (
    id bigint GENERATED ALWAYS AS IDENTITY,
    user_id integer NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    status public.order_status DEFAULT 'pending'::public.order_status NOT NULL,
    note character varying(50) DEFAULT 'n/a'::character varying
);

CREATE TABLE public.items (id serial PRIMARY KEY, order_id integer, sku text, qty integer DEFAULT -1, UNIQUE (order_id, sku), FOREIGN KEY (order_id) REFERENCES public.orders(id) ON UPDATE RESTRICT);

ALTER TABLE ONLY public.users ADD CONSTRAINT users_pkey PRIMARY KEY (id);
ALTER TABLE ONLY public.orders ADD CONSTRAINT orders_pkey PRIMARY KEY (id);
ALTER TABLE ONLY public.users OWNER TO postgres;

CREATE UNIQUE INDEX users_email_key ON public.users USING btree (email);
CREATE INDEX idx_lower_name ON public.users USING btree (lower(name), id DESC);

COMMENT ON TABLE public.users IS 'Application users';
COMMENT ON COLUMN public.users.email IS 'Login e-mail';

CREATE VIEW public.v AS SELECT 1;
CREATE FUNCTION f() RETURNS int LANGUAGE sql AS $$ SELECT 1; $$;
