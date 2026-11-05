-- Opaque local-vault references only. Never store passwords or key material here.
CREATE TABLE IF NOT EXISTS native_secret_references (
    purpose TEXT NOT NULL CHECK (purpose IN ('EMAIL_PASSWORD')),
    identity_hash TEXT NOT NULL,
    item_ref TEXT NOT NULL UNIQUE,
    PRIMARY KEY (purpose, identity_hash)
);
