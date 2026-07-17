CREATE TABLE IF NOT EXISTS blob_uploads (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    sha256 TEXT NOT NULL REFERENCES blobs(sha256) ON DELETE CASCADE,
    size BIGINT NOT NULL CHECK (size >= 0),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, sha256)
);

CREATE INDEX IF NOT EXISTS blob_uploads_expiry_idx ON blob_uploads(expires_at);
