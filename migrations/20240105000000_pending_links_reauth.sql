-- Track which account is being re-authorized (NULL for new links)
ALTER TABLE pending_links ADD COLUMN reauth_account_id INTEGER REFERENCES accounts(id) ON DELETE SET NULL;
