CREATE TABLE attachments (
  session TEXT PRIMARY KEY REFERENCES sessions(id),
  transport TEXT NOT NULL CHECK(transport IN ('raw','claude','codex')),
  endpoint TEXT, updatedAt INTEGER NOT NULL
);
CREATE TABLE dispatches (
  message INTEGER NOT NULL, recipient TEXT NOT NULL,
  token TEXT NOT NULL, transport TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('staged','submitted','uncertain','ready')),
  attemptedAt INTEGER NOT NULL, submittedAt INTEGER, error TEXT,
  PRIMARY KEY(message,recipient),
  FOREIGN KEY(message,recipient) REFERENCES deliveries(message,recipient)
);
CREATE TABLE audit (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session TEXT NOT NULL REFERENCES sessions(id),
  kind TEXT NOT NULL, entityId TEXT, at INTEGER NOT NULL,
  data TEXT NOT NULL CHECK(json_valid(data)), key TEXT,
  UNIQUE(session,kind,key)
);
CREATE INDEX audit_session ON audit(session,id);
CREATE INDEX dispatches_state ON dispatches(recipient,state,message);
CREATE TRIGGER audit_no_update BEFORE UPDATE ON audit BEGIN
  SELECT RAISE(ABORT,'Audit is append-only');
END;
CREATE TRIGGER audit_no_delete BEFORE DELETE ON audit BEGIN
  SELECT RAISE(ABORT,'Audit is append-only');
END;
CREATE TRIGGER message_immutable BEFORE UPDATE ON messages
WHEN NEW.sender IS NOT OLD.sender OR NEW.target IS NOT OLD.target OR NEW.topic IS NOT OLD.topic OR NEW.body IS NOT OLD.body OR NEW.key IS NOT OLD.key BEGIN
  SELECT RAISE(ABORT,'Message content is immutable');
END;
CREATE TRIGGER audit_claim_update AFTER UPDATE OF claimedBy ON deliveries
WHEN NEW.claimedBy IS NOT NULL AND NEW.claimedBy IS NOT OLD.claimedBy BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.recipient,'delivery.claimed',CAST(NEW.message AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('owner',NEW.claimedBy));
END;
CREATE TRIGGER audit_session_insert AFTER INSERT ON sessions BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.id,'session.created',NEW.id,CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('name',NEW.name,'vendor',NEW.vendor,'vendorSession',NEW.vendorSession,'workspace',NEW.workspace));
END;
CREATE TRIGGER audit_session_update AFTER UPDATE ON sessions
WHEN NEW.name IS NOT OLD.name OR NEW.vendorSession IS NOT OLD.vendorSession OR NEW.expiresAt<OLD.expiresAt BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.id,'session.updated',NEW.id,CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('name',NEW.name,'vendorSession',NEW.vendorSession,'expiresAt',NEW.expiresAt));
END;
CREATE TRIGGER audit_message_insert AFTER INSERT ON messages BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.sender,'message.created',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('target',NEW.target,'topic',NEW.topic,'key',NEW.key));
END;
CREATE TRIGGER audit_delivery_insert AFTER INSERT ON deliveries BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.recipient,'delivery.created',CAST(NEW.message AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),'{}');
END;
CREATE TRIGGER audit_delivery_ack AFTER UPDATE OF acknowledgedAt ON deliveries
WHEN OLD.acknowledgedAt IS NULL AND NEW.acknowledgedAt IS NOT NULL BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.recipient,'delivery.acknowledged',CAST(NEW.message AS TEXT),NEW.acknowledgedAt,'{}');
END;
CREATE TRIGGER audit_lease_insert AFTER INSERT ON leases BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.owner,'lease.acquired',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('path',NEW.path,'kind',NEW.kind,'expiresAt',NEW.expiresAt));
END;
CREATE TRIGGER audit_lease_update AFTER UPDATE ON leases BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.owner,'lease.renewed',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('expiresAt',NEW.expiresAt));
END;
CREATE TRIGGER audit_lease_delete AFTER DELETE ON leases BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(OLD.owner,'lease.removed',CAST(OLD.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),'{}');
END;
CREATE TRIGGER audit_subscription_insert AFTER INSERT ON subscriptions BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(NEW.session,'subscription.added',CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('topic',NEW.topic));
END;
CREATE TRIGGER audit_subscription_delete AFTER DELETE ON subscriptions BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(OLD.session,'subscription.removed',CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('topic',OLD.topic));
END;
CREATE TRIGGER audit_attachment_insert AFTER INSERT ON attachments BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(NEW.session,'attachment.created',NEW.updatedAt,json_object('transport',NEW.transport,'endpoint',NEW.endpoint));
END;
CREATE TRIGGER audit_attachment_update AFTER UPDATE ON attachments BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(NEW.session,'attachment.updated',NEW.updatedAt,json_object('transport',NEW.transport,'endpoint',NEW.endpoint));
END;
CREATE TRIGGER audit_dispatch_insert AFTER INSERT ON dispatches BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.recipient,'dispatch.staged',CAST(NEW.message AS TEXT),NEW.attemptedAt,json_object('token',NEW.token,'transport',NEW.transport));
END;
CREATE TRIGGER audit_dispatch_update AFTER UPDATE ON dispatches BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.recipient,'dispatch.'||NEW.state,CAST(NEW.message AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('token',NEW.token,'error',NEW.error));
END;
