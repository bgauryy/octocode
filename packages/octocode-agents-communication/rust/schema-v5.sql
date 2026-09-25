-- Correlation is optional. It never grants access to another message or workspace.
ALTER TABLE messages ADD COLUMN conversationId TEXT
  CHECK(conversationId IS NULL OR (typeof(conversationId)='text' AND length(conversationId) BETWEEN 1 AND 128 AND conversationId NOT GLOB '*[^A-Za-z0-9._:-]*'));
ALTER TABLE messages ADD COLUMN replyTo INTEGER REFERENCES messages(id)
  CHECK(replyTo IS NULL OR (typeof(replyTo)='integer' AND replyTo BETWEEN 1 AND 9007199254740991));
CREATE INDEX messages_conversation ON messages(conversationId,id) WHERE conversationId IS NOT NULL;
CREATE INDEX messages_reply ON messages(replyTo,id) WHERE replyTo IS NOT NULL;
CREATE TRIGGER messages_correlation_immutable BEFORE UPDATE OF conversationId,replyTo ON messages
WHEN NEW.conversationId IS NOT OLD.conversationId OR NEW.replyTo IS NOT OLD.replyTo BEGIN
  SELECT RAISE(ABORT,'Message correlation is immutable');
END;
CREATE TRIGGER messages_reply_visible BEFORE INSERT ON messages
WHEN NEW.replyTo IS NOT NULL AND NOT EXISTS (
  SELECT 1 FROM messages parent JOIN sessions author ON author.id=parent.sender
  JOIN sessions sender ON sender.id=NEW.sender
  WHERE parent.id=NEW.replyTo AND author.workspace=sender.workspace
    AND (parent.sender=NEW.sender OR EXISTS (SELECT 1 FROM deliveries d WHERE d.message=parent.id AND d.recipient=NEW.sender))
    AND NEW.conversationId IS parent.conversationId
) BEGIN
  SELECT RAISE(ABORT,'Reply requires a visible parent in this workspace and its conversationId');
END;
DROP TRIGGER audit_message_insert;
CREATE TRIGGER audit_message_insert AFTER INSERT ON messages BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.sender,'message.created',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('target',NEW.target,'topic',NEW.topic,'key',NEW.key,'reasoning',NEW.reasoning,'wake',NEW.wake,'conversationId',NEW.conversationId,'replyTo',NEW.replyTo));
END;
-- Rebuild only the leaf attachment table; preserve the existing audit history.
DROP TRIGGER audit_attachment_insert;
DROP TRIGGER audit_attachment_update;
CREATE TABLE attachments_v5 (
  session TEXT PRIMARY KEY REFERENCES sessions(id),
  transport TEXT NOT NULL CHECK(transport IN ('raw','claude','codex','opencode')),
  endpoint TEXT, updatedAt INTEGER NOT NULL
);
INSERT INTO attachments_v5 SELECT session,transport,endpoint,updatedAt FROM attachments;
DROP TABLE attachments;
ALTER TABLE attachments_v5 RENAME TO attachments;
CREATE TRIGGER audit_attachment_insert AFTER INSERT ON attachments BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(NEW.session,'attachment.created',NEW.updatedAt,json_object('transport',NEW.transport,'endpoint',NEW.endpoint));
END;
CREATE TRIGGER audit_attachment_update AFTER UPDATE ON attachments BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(NEW.session,'attachment.updated',NEW.updatedAt,json_object('transport',NEW.transport,'endpoint',NEW.endpoint));
END;
PRAGMA user_version=5;
