ALTER TABLE users
    ADD COLUMN theme TEXT NOT NULL DEFAULT 'light' CHECK (theme IN ('light', 'dark'));
