-- User-deleted local delivery feedback is not a native transcript deletion.
ALTER TABLE watch_replies ADD COLUMN removed_at INTEGER;
ALTER TABLE pi_commands ADD COLUMN removed_at INTEGER;
UPDATE watch_replies SET removed_at=updated_at,text='',options_json='{}' WHERE status='CANCELLED';
UPDATE pi_commands SET removed_at=updated_at,input_json='{}'
 WHERE status='NOT_SENT' AND error_code IN ('PI_ABANDONED_BEFORE_NATIVE_SUBMISSION','PI_CANCELLED_BEFORE_NATIVE_SUBMISSION');
