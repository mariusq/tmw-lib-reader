-- Derived search data is regenerable.  This marker lets startup distinguish a
-- current index from one that needs a migration/repair rebuild.
CREATE TABLE search_index_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  schema_version INTEGER NOT NULL,
  completed_at INTEGER NOT NULL
);
