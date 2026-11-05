CREATE TABLE IF NOT EXISTS watch_group_removals (
 parent_id TEXT NOT NULL, child_id TEXT PRIMARY KEY
);
-- Repair only children already present when their displayed parent was removed.
-- A current owner-assigned manager supersedes the immutable original Pi parent.
INSERT OR IGNORE INTO watch_group_removals(parent_id,child_id)
SELECT r.id,w.thread_id FROM watch_removed_items r
JOIN codex_watches parent ON parent.thread_id=r.id
JOIN pi_tasks p JOIN codex_watches w ON w.thread_id='pi:'||p.native_session_id
LEFT JOIN management_members m ON m.thread_id=w.thread_id
LEFT JOIN management_zones z ON z.id=m.zone_id
LEFT JOIN watch_visibility v ON v.thread_id=w.thread_id
WHERE r.kind='WATCH' AND w.thread_id!=r.id
AND COALESCE(v.shown_at,0)<=r.removed_at
AND NOT EXISTS(SELECT 1 FROM watch_removed_items gone WHERE gone.kind='WATCH' AND gone.id=w.thread_id)
AND ((m.thread_id IS NOT NULL AND z.manager_id=r.id) OR
 (m.thread_id IS NULL AND p.parent_cwd=parent.cwd AND
 CASE p.parent_provider WHEN 'CLAUDE_CODE' THEN 'claude_code:'||p.parent_external_id WHEN 'CODEX' THEN p.parent_external_id END=r.id));
INSERT OR IGNORE INTO watch_removed_items(kind,id,was_enabled,removed_at)
SELECT 'WATCH',g.child_id,w.enabled,r.removed_at FROM watch_group_removals g
JOIN codex_watches w ON w.thread_id=g.child_id
JOIN watch_removed_items r ON r.kind='WATCH' AND r.id=g.parent_id;
UPDATE codex_watches SET enabled=0,generation=generation+1
WHERE thread_id IN(SELECT child_id FROM watch_group_removals);
