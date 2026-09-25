-- Existing direct rows keep action compatibility; existing fanout is passive.
ALTER TABLE messages ADD COLUMN wake TEXT NOT NULL DEFAULT 'action' CHECK(wake IN ('action','passive'));
UPDATE messages SET wake='passive' WHERE target='*' OR topic IS NOT NULL;
CREATE TRIGGER messages_wake_immutable BEFORE UPDATE OF wake ON messages
WHEN NEW.wake IS NOT OLD.wake BEGIN
  SELECT RAISE(ABORT,'wake is immutable; create a new message');
END;
DROP TRIGGER audit_message_insert;
CREATE TRIGGER audit_message_insert AFTER INSERT ON messages BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.sender,'message.created',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('target',NEW.target,'topic',NEW.topic,'key',NEW.key,'reasoning',NEW.reasoning,'wake',NEW.wake));
END;
PRAGMA user_version=4;
