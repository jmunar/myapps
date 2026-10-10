-- Problems now arrive as prepared bundles (see src/bundle.rs), and a bundle is
-- the whole of its dataset: a problem a later bundle no longer has is retired
-- rather than deleted, since attempts and current problems point at it. A
-- retired problem is never drawn; a bundle that has it again brings it back.
ALTER TABLE challenges_problems ADD COLUMN retired_at TEXT;

-- challenges_imports now records the last bundle loaded into each dataset; it
-- no longer decides whether `serve` imports anything, since it imports nothing.
ALTER TABLE challenges_imports ADD COLUMN prepared_at TEXT;
ALTER TABLE challenges_imports ADD COLUMN prepared_by TEXT;
