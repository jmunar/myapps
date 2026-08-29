-- Indexa Capital provider: per-user API token (AES-256-GCM encrypted, same
-- scheme as enable_banking_key) and a marker for Indexa-backed accounts.
--
-- Indexa accounts reuse leanfin_accounts with account_type = 'indexa':
--   account_uid          → Indexa account_number
--   session_expires_at   → 9999-12-31 sentinel (personal tokens do not expire,
--                          unlike PSD2 consents, so there is nothing to reauth)
ALTER TABLE leanfin_user_settings ADD COLUMN indexa_token BLOB;
