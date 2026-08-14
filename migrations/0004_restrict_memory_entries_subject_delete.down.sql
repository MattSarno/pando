ALTER TABLE memory_entries DROP CONSTRAINT memory_entries_subject_id_fkey;
ALTER TABLE memory_entries ADD CONSTRAINT memory_entries_subject_id_fkey
    FOREIGN KEY (subject_id) REFERENCES subjects(slug) ON DELETE CASCADE;
