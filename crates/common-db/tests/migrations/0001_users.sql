CREATE TABLE users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL CHECK (length(name) > 0),
    created_at TIMESTAMPTZ NOT NULL
);
