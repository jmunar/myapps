-- Add archived flag to accounts (default false)
ALTER TABLE accounts ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;
