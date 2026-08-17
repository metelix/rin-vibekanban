-- Self-hosted auth tables for the /v1/* API (local login, JWT sessions, identities).

CREATE TABLE IF NOT EXISTS users (
    id          BLOB PRIMARY KEY NOT NULL,
    email       TEXT NOT NULL UNIQUE,
    username    TEXT,
    first_name  TEXT,
    last_name   TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS organizations (
    id           BLOB PRIMARY KEY NOT NULL,
    name         TEXT NOT NULL,
    slug         TEXT NOT NULL UNIQUE,
    is_personal  INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS organization_members (
    organization_id  BLOB NOT NULL REFERENCES organizations (id),
    user_id          BLOB NOT NULL REFERENCES users (id),
    role             TEXT NOT NULL,
    created_at       TEXT NOT NULL,
    PRIMARY KEY (organization_id, user_id)
);

CREATE TABLE IF NOT EXISTS auth_sessions (
    id                 BLOB PRIMARY KEY NOT NULL,
    user_id            BLOB NOT NULL REFERENCES users (id),
    refresh_token_id   TEXT,
    created_at         TEXT NOT NULL,
    expires_at         TEXT NOT NULL,
    last_used_at       TEXT
);

CREATE INDEX IF NOT EXISTS idx_auth_sessions_user_id ON auth_sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_organization_members_user_id ON organization_members (user_id);