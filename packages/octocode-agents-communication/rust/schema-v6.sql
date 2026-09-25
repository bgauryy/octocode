-- Add native Grok delivery without rewriting messages or audit history.
DROP TRIGGER audit_attachment_insert;
DROP TRIGGER audit_attachment_update;
CREATE TABLE attachments_v6 (
  session TEXT PRIMARY KEY REFERENCES sessions(id),
  transport TEXT NOT NULL CHECK(transport IN ('raw','claude','codex','opencode','grok')),
  endpoint TEXT, updatedAt INTEGER NOT NULL
);
INSERT INTO attachments_v6 SELECT session,transport,endpoint,updatedAt FROM attachments;
DROP TABLE attachments;
ALTER TABLE attachments_v6 RENAME TO attachments;
CREATE TRIGGER audit_attachment_insert AFTER INSERT ON attachments BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(NEW.session,'attachment.created',NEW.updatedAt,json_object('transport',NEW.transport,'endpoint',NEW.endpoint));
END;
CREATE TRIGGER audit_attachment_update AFTER UPDATE ON attachments BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(NEW.session,'attachment.updated',NEW.updatedAt,json_object('transport',NEW.transport,'endpoint',NEW.endpoint));
END;
PRAGMA user_version=6;
