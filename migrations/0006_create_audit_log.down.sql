DROP TRIGGER tasks_audit ON tasks;
DROP TRIGGER memory_entries_audit ON memory_entries;
DROP TRIGGER subjects_audit ON subjects;
DROP FUNCTION audit_row_change();
DROP TABLE audit_log;
