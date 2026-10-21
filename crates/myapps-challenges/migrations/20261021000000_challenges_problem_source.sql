-- A deck that cites a source per item (data-science, bundle format 4) brings
-- its own source_url per problem; this is the link text for it. Null for the
-- datasets whose source is the dataset itself, which show the dataset's name.
ALTER TABLE challenges_problems ADD COLUMN source_title TEXT;
