CREATE TABLE memory_entries (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    scope TEXT NOT NULL CHECK (scope IN ('profile', 'topic', 'preference')),
    key TEXT NOT NULL,
    subject_id TEXT REFERENCES subjects(slug) ON DELETE CASCADE,
    description TEXT NOT NULL,
    content TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by TEXT NOT NULL
);

-- at most one global default per (scope, key)
CREATE UNIQUE INDEX memory_entries_global_key ON memory_entries (scope, key) WHERE subject_id IS NULL;

-- at most one override per (scope, key) per subject
CREATE UNIQUE INDEX memory_entries_subject_key ON memory_entries (scope, key, subject_id) WHERE subject_id IS NOT NULL;

-- fast lookup of everything hung off a given subject
CREATE INDEX memory_entries_subject_idx ON memory_entries (subject_id) WHERE subject_id IS NOT NULL;
