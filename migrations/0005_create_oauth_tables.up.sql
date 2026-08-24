CREATE TABLE oauth_clients (
    client_id TEXT PRIMARY KEY,
    redirect_uris TEXT[] NOT NULL CHECK (cardinality(redirect_uris) > 0),
    client_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE oauth_auth_codes (
    code TEXT PRIMARY KEY,
    -- no deletion path for clients yet; RESTRICT is a deliberate placeholder
    -- so a future client-delete feature can't silently orphan/cascade this
    client_id TEXT NOT NULL REFERENCES oauth_clients(client_id) ON DELETE RESTRICT,
    redirect_uri TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ
);

CREATE TABLE oauth_tokens (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES oauth_clients(client_id) ON DELETE RESTRICT,
    access_token_hash TEXT NOT NULL,
    refresh_token_hash TEXT NOT NULL,
    access_expires_at TIMESTAMPTZ NOT NULL,
    refresh_expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ
);

-- fast lookup of a token pair by whichever token came in on the wire
CREATE UNIQUE INDEX oauth_tokens_access_hash_idx ON oauth_tokens (access_token_hash);
CREATE UNIQUE INDEX oauth_tokens_refresh_hash_idx ON oauth_tokens (refresh_token_hash);

-- fast lookup of everything issued to a given client (e.g. revoke-all-for-client)
CREATE INDEX oauth_tokens_client_idx ON oauth_tokens (client_id);
