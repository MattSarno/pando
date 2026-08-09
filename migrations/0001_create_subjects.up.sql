CREATE TABLE subjects (
    slug TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('project', 'focus', 'relationship')),
    status TEXT,
    summary TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by TEXT NOT NULL,
    -- status only applies to work-like subjects, and its legal values depend on which kind:
    -- projects can finish, focuses are open-ended by definition, relationships have no lifecycle at all
    CONSTRAINT status_matches_category CHECK (
        (category = 'project' AND status IN ('active', 'paused', 'finished'))
        OR
        (category = 'focus' AND status IN ('active', 'paused'))
        OR
        (category = 'relationship' AND status IS NULL)
    )
);
