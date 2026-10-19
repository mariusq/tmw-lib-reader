-- Additive repair for early schema-22 catalogs; retain activity and its epoch.
CREATE TABLE IF NOT EXISTS activity_reset_receipts(id TEXT PRIMARY KEY);
