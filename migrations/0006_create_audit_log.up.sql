CREATE TABLE audit_log (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    at TIMESTAMPTZ NOT NULL DEFAULT now(),
    actor TEXT NOT NULL,
    client_id TEXT,
    entity TEXT NOT NULL CHECK (entity IN ('subject', 'memory', 'task')),
    entity_key TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('insert', 'update', 'delete')),
    before JSONB
);

-- newest-first listing, after_id paging, and the retention sweep
CREATE INDEX audit_log_at_idx ON audit_log (at, id);

-- history of a single row
CREATE INDEX audit_log_entity_idx ON audit_log (entity, entity_key);

CREATE FUNCTION audit_row_change() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    current_actor TEXT := nullif(current_setting('pando.actor', true), '');
    changed_row JSONB := CASE TG_OP WHEN 'DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
    row_key TEXT;
BEGIN
    IF current_actor IS NULL THEN
        RAISE EXCEPTION 'pando.actor is not set; refusing unattributed % on %', TG_OP, TG_TABLE_NAME;
    END IF;

    row_key := CASE TG_ARGV[0]
        WHEN 'subject' THEN changed_row ->> 'slug'
        WHEN 'memory' THEN (changed_row ->> 'scope') || '/' || (changed_row ->> 'key')
            || coalesce('@' || (changed_row ->> 'subject_id'), '')
        WHEN 'task' THEN changed_row ->> 'id'
    END;

    INSERT INTO audit_log (actor, client_id, entity, entity_key, action, before)
    VALUES (
        current_actor,
        nullif(current_setting('pando.client_id', true), ''),
        TG_ARGV[0],
        row_key,
        lower(TG_OP),
        CASE TG_OP WHEN 'INSERT' THEN NULL ELSE to_jsonb(OLD) END
    );

    -- 30-day retention, see DESIGN.md #5
    DELETE FROM audit_log WHERE at < now() - interval '30 days';

    RETURN NULL;
END;
$$;

CREATE TRIGGER subjects_audit AFTER INSERT OR UPDATE OR DELETE ON subjects
    FOR EACH ROW EXECUTE FUNCTION audit_row_change('subject');

CREATE TRIGGER memory_entries_audit AFTER INSERT OR UPDATE OR DELETE ON memory_entries
    FOR EACH ROW EXECUTE FUNCTION audit_row_change('memory');

CREATE TRIGGER tasks_audit AFTER INSERT OR UPDATE OR DELETE ON tasks
    FOR EACH ROW EXECUTE FUNCTION audit_row_change('task');
