-- Normalized keys are regenerable app data. Original JMdict display fields stay untouched.
ALTER TABLE dictionary_entries ADD COLUMN term_normalized TEXT NOT NULL DEFAULT '';
ALTER TABLE dictionary_entries ADD COLUMN reading_normalized TEXT;
CREATE INDEX idx_dictionary_entries_term_normalized ON dictionary_entries(term_normalized);
CREATE INDEX idx_dictionary_entries_reading_normalized ON dictionary_entries(reading_normalized);
