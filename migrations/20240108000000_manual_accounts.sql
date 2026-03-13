-- Add manual account support.
-- New columns: account_type, account_name, asset_category.
-- Enable Banking columns (session_id, account_uid, session_expires_at) remain NOT NULL
-- in the schema for existing rows; manual accounts use placeholder values.

ALTER TABLE accounts ADD COLUMN account_type TEXT NOT NULL DEFAULT 'bank';
ALTER TABLE accounts ADD COLUMN account_name TEXT;
ALTER TABLE accounts ADD COLUMN asset_category TEXT;
