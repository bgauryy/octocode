-- Historical rows retain NULL: their intent was not recorded. New writes require it.
ALTER TABLE messages ADD COLUMN reasoning TEXT;
CREATE TRIGGER messages_reasoning_required BEFORE INSERT ON messages
WHEN NOT coalesce(typeof(NEW.reasoning)='text' AND length(trim(NEW.reasoning,char(9,10,11,12,13,32,133,160,5760,8192,8193,8194,8195,8196,8197,8198,8199,8200,8201,8202,8232,8233,8239,8287,12288)))>0 AND length(CAST(NEW.reasoning AS BLOB))<=512,0) BEGIN
  SELECT RAISE(ABORT,'reasoning is required (nonblank, at most 512 UTF-8 bytes)');
END;
CREATE TRIGGER messages_reasoning_immutable BEFORE UPDATE OF reasoning ON messages
WHEN NEW.reasoning IS NOT OLD.reasoning BEGIN
  SELECT RAISE(ABORT,'reasoning is immutable; create a new message or lease');
END;
ALTER TABLE leases ADD COLUMN reasoning TEXT;
CREATE TRIGGER leases_reasoning_required BEFORE INSERT ON leases
WHEN NOT coalesce(typeof(NEW.reasoning)='text' AND length(trim(NEW.reasoning,char(9,10,11,12,13,32,133,160,5760,8192,8193,8194,8195,8196,8197,8198,8199,8200,8201,8202,8232,8233,8239,8287,12288)))>0 AND length(CAST(NEW.reasoning AS BLOB))<=512,0) BEGIN
  SELECT RAISE(ABORT,'reasoning is required (nonblank, at most 512 UTF-8 bytes)');
END;
CREATE TRIGGER leases_reasoning_immutable BEFORE UPDATE OF reasoning ON leases
WHEN NEW.reasoning IS NOT OLD.reasoning BEGIN
  SELECT RAISE(ABORT,'reasoning is immutable; create a new message or lease');
END;
DROP TRIGGER audit_message_insert;
CREATE TRIGGER audit_message_insert AFTER INSERT ON messages BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.sender,'message.created',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('target',NEW.target,'topic',NEW.topic,'key',NEW.key,'reasoning',NEW.reasoning));
END;
DROP TRIGGER audit_lease_insert;
CREATE TRIGGER audit_lease_insert AFTER INSERT ON leases BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.owner,'lease.acquired',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('path',NEW.path,'kind',NEW.kind,'expiresAt',NEW.expiresAt,'reasoning',NEW.reasoning));
END;
DROP TRIGGER audit_lease_update;
CREATE TRIGGER audit_lease_update AFTER UPDATE ON leases BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.owner,'lease.renewed',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('expiresAt',NEW.expiresAt,'reasoning',NEW.reasoning));
END;
DROP TRIGGER audit_lease_delete;
CREATE TRIGGER audit_lease_delete AFTER DELETE ON leases BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(OLD.owner,'lease.removed',CAST(OLD.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('reasoning',OLD.reasoning));
END;
