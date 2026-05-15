-- Title moves into the CRDT alongside the body. There's no Rust-side
-- markdown→prosemirror serializer yet, so we can't preserve existing notes
-- in the new shape; wipe and start fresh. Single-user app, no data to lose.

DELETE FROM notes_note_updates;
DELETE FROM notes_notes;
