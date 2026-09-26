-- Indexed lease namespace, document registry and lookup indexes; history is preserved.
-- pathKey is the lease comparison key: "/" + NFD(casefold(NFD(component))) per path
-- component. Writers compute it; `db migrate` backfills existing leases natively.
ALTER TABLE leases ADD COLUMN pathKey TEXT;
CREATE INDEX leases_path ON leases(workspace,pathKey);
CREATE INDEX leases_owner ON leases(owner);
CREATE TRIGGER leases_path_key_required BEFORE INSERT ON leases
WHEN typeof(NEW.pathKey)<>'text' OR substr(NEW.pathKey,1,1)<>'/' BEGIN
  SELECT RAISE(ABORT,'pathKey is required: "/" + NFD(casefold(NFD(component))) per path component');
END;
CREATE TRIGGER leases_path_immutable BEFORE UPDATE OF workspace,path,kind,owner,pathKey ON leases
WHEN NEW.workspace IS NOT OLD.workspace OR NEW.path IS NOT OLD.path OR NEW.kind IS NOT OLD.kind
  OR NEW.owner IS NOT OLD.owner OR (OLD.pathKey IS NOT NULL AND NEW.pathKey IS NOT OLD.pathKey) BEGIN
  SELECT RAISE(ABORT,'Lease scope is immutable; acquire a new lease');
END;
-- Requested message TTL, part of key-retry equality; null for history.
ALTER TABLE messages ADD COLUMN ttlMs INTEGER
  CHECK(ttlMs IS NULL OR (typeof(ttlMs)='integer' AND ttlMs BETWEEN 1000 AND 86400000));
CREATE INDEX sessions_scope ON sessions(workspace,expiresAt);
CREATE INDEX sessions_host ON sessions(workspace,vendorSession) WHERE vendorSession IS NOT NULL;
CREATE INDEX subscriptions_topic ON subscriptions(topic);
-- One immutable document name per workspace; rows mirror document.created audit.
CREATE TABLE documents (
  id INTEGER PRIMARY KEY REFERENCES audit(id),
  workspace TEXT NOT NULL, name TEXT NOT NULL,
  UNIQUE(workspace,name)
);
CREATE INDEX documents_scan ON documents(workspace,id);
INSERT OR IGNORE INTO documents(id,workspace,name)
  SELECT a.id,s.workspace,a.key FROM audit a JOIN sessions s ON s.id=a.session
  WHERE a.kind='document.created' AND a.key IS NOT NULL ORDER BY a.id;
CREATE TRIGGER documents_register AFTER INSERT ON audit WHEN NEW.kind='document.created' BEGIN
  INSERT INTO documents(id,workspace,name) SELECT NEW.id,workspace,NEW.key FROM sessions WHERE id=NEW.session;
END;
CREATE TRIGGER documents_no_update BEFORE UPDATE ON documents BEGIN
  SELECT RAISE(ABORT,'Documents are immutable');
END;
CREATE TRIGGER documents_no_delete BEFORE DELETE ON documents BEGIN
  SELECT RAISE(ABORT,'Documents are immutable');
END;
-- Audit snapshots omit null members; the session row already carries its workspace.
DROP TRIGGER audit_session_insert;
CREATE TRIGGER audit_session_insert AFTER INSERT ON sessions BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.id,'session.created',NEW.id,CAST(unixepoch('subsec')*1000 AS INTEGER),json_patch('{}',json_object('name',NEW.name,'vendor',NEW.vendor,'vendorSession',NEW.vendorSession)));
END;
DROP TRIGGER audit_session_update;
CREATE TRIGGER audit_session_update AFTER UPDATE ON sessions
WHEN NEW.name IS NOT OLD.name OR NEW.vendorSession IS NOT OLD.vendorSession OR NEW.expiresAt<OLD.expiresAt BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.id,'session.updated',NEW.id,CAST(unixepoch('subsec')*1000 AS INTEGER),json_patch('{}',json_object('name',NEW.name,'vendorSession',NEW.vendorSession,'expiresAt',NEW.expiresAt)));
END;
DROP TRIGGER audit_message_insert;
CREATE TRIGGER audit_message_insert AFTER INSERT ON messages BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.sender,'message.created',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_patch('{}',json_object('target',NEW.target,'topic',NEW.topic,'key',NEW.key,'reasoning',NEW.reasoning,'wake',NEW.wake,'conversationId',NEW.conversationId,'replyTo',NEW.replyTo)));
END;
-- Only expiry changes are renewals; a scope backfill is not an audited event.
DROP TRIGGER audit_lease_update;
CREATE TRIGGER audit_lease_update AFTER UPDATE OF expiresAt ON leases
WHEN NEW.expiresAt IS NOT OLD.expiresAt BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.owner,'lease.renewed',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_patch('{}',json_object('expiresAt',NEW.expiresAt,'reasoning',NEW.reasoning)));
END;
DROP TRIGGER audit_lease_delete;
CREATE TRIGGER audit_lease_delete AFTER DELETE ON leases BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(OLD.owner,'lease.removed',CAST(OLD.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_patch('{}',json_object('reasoning',OLD.reasoning)));
END;
PRAGMA user_version=7;
