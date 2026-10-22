CREATE TABLE claude_child_contexts(
 key TEXT PRIMARY KEY,root TEXT NOT NULL,generation INTEGER NOT NULL,cwd TEXT NOT NULL,
 label TEXT NOT NULL,valid_until INTEGER NOT NULL
);
