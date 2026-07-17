CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TYPE user_role AS ENUM ('user', 'superuser');
CREATE TYPE user_status AS ENUM ('pending', 'active', 'suspended');
CREATE TYPE repository_kind AS ENUM ('model', 'dataset');
CREATE TYPE repository_visibility AS ENUM ('public', 'private');

CREATE TABLE instance_settings (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    initialized_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    instance_name TEXT NOT NULL CHECK (char_length(instance_name) BETWEEN 1 AND 80),
    signup_policy TEXT NOT NULL CHECK (signup_policy IN ('disabled', 'immediate', 'approval')),
    default_visibility repository_visibility NOT NULL DEFAULT 'public',
    retention_days INTEGER NOT NULL DEFAULT 30 CHECK (retention_days BETWEEN 1 AND 3650)
);

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL UNIQUE CHECK (username ~ '^[a-z0-9][a-z0-9-]{1,38}[a-z0-9]$'),
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role user_role NOT NULL DEFAULT 'user',
    status user_status NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    id_hash TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE api_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    scopes TEXT[] NOT NULL,
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE repositories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id UUID NOT NULL REFERENCES users(id),
    kind repository_kind NOT NULL,
    name TEXT NOT NULL CHECK (name ~ '^[A-Za-z0-9][A-Za-z0-9._-]{0,95}$'),
    description TEXT NOT NULL DEFAULT '',
    visibility repository_visibility NOT NULL DEFAULT 'public',
    head_commit_id UUID,
    download_count BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    UNIQUE(owner_id, kind, name)
);

CREATE TABLE blobs (
    sha256 TEXT PRIMARY KEY CHECK (char_length(sha256) = 64),
    size BIGINT NOT NULL CHECK (size >= 0),
    storage_key TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE commits (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    parent_id UUID REFERENCES commits(id),
    author_id UUID NOT NULL REFERENCES users(id),
    message TEXT NOT NULL CHECK (char_length(message) BETWEEN 1 AND 500),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE repositories
    ADD CONSTRAINT repositories_head_commit_fk
    FOREIGN KEY (head_commit_id) REFERENCES commits(id);

CREATE TABLE commit_files (
    commit_id UUID NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    path TEXT NOT NULL CHECK (path <> '' AND path !~ '(^|/)\.\.(/|$)'),
    blob_sha256 TEXT NOT NULL REFERENCES blobs(sha256),
    size BIGINT NOT NULL,
    PRIMARY KEY (commit_id, path)
);

CREATE INDEX repositories_discovery_idx ON repositories(kind, visibility, updated_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX repositories_owner_idx ON repositories(owner_id) WHERE deleted_at IS NULL;
CREATE INDEX commit_files_blob_idx ON commit_files(blob_sha256);
CREATE INDEX sessions_expiry_idx ON sessions(expires_at);

