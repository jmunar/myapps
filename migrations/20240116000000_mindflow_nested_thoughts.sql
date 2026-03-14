-- Allow thoughts to nest under other thoughts, forming a tree structure.
ALTER TABLE mindflow_thoughts ADD COLUMN parent_thought_id INTEGER
    REFERENCES mindflow_thoughts(id) ON DELETE SET NULL;

CREATE INDEX idx_mindflow_thoughts_parent ON mindflow_thoughts(parent_thought_id);
