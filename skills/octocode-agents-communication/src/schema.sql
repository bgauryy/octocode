PRAGMA application_id=1329678147;
PRAGMA user_version=1;

CREATE TABLE sessions (
  id TEXT PRIMARY KEY, workspace TEXT NOT NULL, name TEXT NOT NULL, vendor TEXT NOT NULL,
  vendorSession TEXT, expiresAt INTEGER NOT NULL,
  task TEXT NOT NULL DEFAULT '' CHECK(length(task)<=256), status TEXT NOT NULL DEFAULT 'unknown'
  CHECK(status IN ('available','busy','blocked','unknown')));

CREATE TABLE subscriptions (
  session TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE, topic TEXT NOT NULL,
  PRIMARY KEY(session, topic)
  );

CREATE TABLE leases (
  id INTEGER PRIMARY KEY AUTOINCREMENT, workspace TEXT NOT NULL, path TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('file','tree')), owner TEXT NOT NULL REFERENCES sessions(id),
  acquiredAt INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec')*1000 AS INTEGER)),
  refreshedAt INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec')*1000 AS INTEGER)),
  expiresAt INTEGER NOT NULL CHECK(expiresAt<=refreshedAt+600000),
  reasoning TEXT NOT NULL, pathKey TEXT NOT NULL);

CREATE TABLE messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT, sender TEXT NOT NULL REFERENCES sessions(id),
  target TEXT NOT NULL, topic TEXT, body TEXT NOT NULL, key TEXT NOT NULL,
  replyRequired INTEGER NOT NULL DEFAULT 1 CHECK(replyRequired IN (0,1)),
  expiresAt INTEGER NOT NULL, reasoning TEXT NOT NULL, wake TEXT NOT NULL DEFAULT 'action' CHECK(wake IN ('action','passive')), conversationId TEXT
  CHECK(conversationId IS NULL OR (typeof(conversationId)='text' AND length(conversationId) BETWEEN 1 AND 128 AND conversationId NOT GLOB '*[^A-Za-z0-9._:-]*')), replyTo INTEGER REFERENCES messages(id)
  CHECK(replyTo IS NULL OR (typeof(replyTo)='integer' AND replyTo BETWEEN 1 AND 9007199254740991)), ttlMs INTEGER NOT NULL DEFAULT 3600000
  CHECK(typeof(ttlMs)='integer' AND ttlMs BETWEEN 1000 AND 86400000), UNIQUE(sender, key)
  );

CREATE TABLE deliveries (
  message INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  recipient TEXT NOT NULL REFERENCES sessions(id), acknowledgedAt INTEGER,
  claimedBy TEXT, claimUntil INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(message, recipient)
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

CREATE TABLE "attachments" (
  session TEXT PRIMARY KEY REFERENCES sessions(id),
  transport TEXT NOT NULL CHECK(transport IN ('raw','claude','codex','opencode','grok')),
  endpoint TEXT, updatedAt INTEGER NOT NULL
);

CREATE TABLE documents (
  id INTEGER PRIMARY KEY REFERENCES audit(id),
  workspace TEXT NOT NULL, name TEXT NOT NULL,
  UNIQUE(workspace,name)
);

CREATE TABLE peer_revisions (workspace TEXT PRIMARY KEY, revision INTEGER NOT NULL);

CREATE TABLE peer_views (
  session TEXT NOT NULL REFERENCES sessions(id), consumer TEXT NOT NULL,
  generation TEXT NOT NULL, snapshot TEXT NOT NULL CHECK(json_valid(snapshot)),
  PRIMARY KEY(session,consumer)
);

CREATE INDEX leases_scope ON leases(workspace, expiresAt);

CREATE INDEX deliveries_inbox ON deliveries(recipient, acknowledgedAt, message);

CREATE INDEX audit_session ON audit(session,id);

CREATE INDEX dispatches_state ON dispatches(recipient,state,message);

CREATE INDEX messages_conversation ON messages(conversationId,id) WHERE conversationId IS NOT NULL;

CREATE INDEX messages_reply ON messages(replyTo,id) WHERE replyTo IS NOT NULL;

CREATE INDEX leases_path ON leases(workspace,pathKey);

CREATE INDEX leases_owner ON leases(owner);

CREATE INDEX sessions_scope ON sessions(workspace,expiresAt);

CREATE INDEX sessions_host ON sessions(workspace,vendorSession) WHERE vendorSession IS NOT NULL;

CREATE INDEX subscriptions_topic ON subscriptions(topic);

CREATE INDEX documents_scan ON documents(workspace,id);

CREATE TRIGGER audit_no_update BEFORE UPDATE ON audit BEGIN
  SELECT RAISE(ABORT,'Audit is append-only');
END;

CREATE TRIGGER audit_no_delete BEFORE DELETE ON audit BEGIN
  SELECT RAISE(ABORT,'Audit is append-only');
END;

CREATE TRIGGER message_immutable BEFORE UPDATE ON messages
WHEN NEW.sender IS NOT OLD.sender OR NEW.target IS NOT OLD.target OR NEW.topic IS NOT OLD.topic OR NEW.body IS NOT OLD.body OR NEW.key IS NOT OLD.key OR NEW.ttlMs IS NOT OLD.ttlMs OR NEW.replyRequired IS NOT OLD.replyRequired BEGIN
  SELECT RAISE(ABORT,'Message content is immutable');
END;

CREATE TRIGGER audit_claim_update AFTER UPDATE OF claimedBy ON deliveries
WHEN NEW.claimedBy IS NOT NULL AND NEW.claimedBy IS NOT OLD.claimedBy BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.recipient,'delivery.claimed',CAST(NEW.message AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('owner',NEW.claimedBy));
END;

CREATE TRIGGER audit_delivery_insert AFTER INSERT ON deliveries BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.recipient,'delivery.created',CAST(NEW.message AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),'{}');
END;

CREATE TRIGGER audit_delivery_ack AFTER UPDATE OF acknowledgedAt ON deliveries
WHEN OLD.acknowledgedAt IS NULL AND NEW.acknowledgedAt IS NOT NULL BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.recipient,'delivery.acknowledged',CAST(NEW.message AS TEXT),NEW.acknowledgedAt,'{}');
END;

CREATE TRIGGER audit_subscription_insert AFTER INSERT ON subscriptions BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(NEW.session,'subscription.added',CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('topic',NEW.topic));
END;

CREATE TRIGGER audit_subscription_delete AFTER DELETE ON subscriptions BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(OLD.session,'subscription.removed',CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('topic',OLD.topic));
END;

CREATE TRIGGER audit_dispatch_insert AFTER INSERT ON dispatches BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.recipient,'dispatch.staged',CAST(NEW.message AS TEXT),NEW.attemptedAt,json_object('token',NEW.token,'transport',NEW.transport));
END;

CREATE TRIGGER audit_dispatch_update AFTER UPDATE ON dispatches BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.recipient,'dispatch.'||NEW.state,CAST(NEW.message AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('token',NEW.token,'error',NEW.error));
END;

CREATE TRIGGER messages_reasoning_required BEFORE INSERT ON messages
WHEN NOT coalesce(typeof(NEW.reasoning)='text' AND length(trim(NEW.reasoning,char(9,10,11,12,13,32,133,160,5760,8192,8193,8194,8195,8196,8197,8198,8199,8200,8201,8202,8232,8233,8239,8287,12288)))>0 AND length(CAST(NEW.reasoning AS BLOB))<=512,0) BEGIN
  SELECT RAISE(ABORT,'reasoning is required (nonblank, at most 512 UTF-8 bytes)');
END;

CREATE TRIGGER messages_reasoning_immutable BEFORE UPDATE OF reasoning ON messages
WHEN NEW.reasoning IS NOT OLD.reasoning BEGIN
  SELECT RAISE(ABORT,'reasoning is immutable; create a new message or lease');
END;

CREATE TRIGGER leases_reasoning_required BEFORE INSERT ON leases
WHEN NOT coalesce(typeof(NEW.reasoning)='text' AND length(trim(NEW.reasoning,char(9,10,11,12,13,32,133,160,5760,8192,8193,8194,8195,8196,8197,8198,8199,8200,8201,8202,8232,8233,8239,8287,12288)))>0 AND length(CAST(NEW.reasoning AS BLOB))<=512,0) BEGIN
  SELECT RAISE(ABORT,'reasoning is required (nonblank, at most 512 UTF-8 bytes)');
END;

CREATE TRIGGER leases_reasoning_immutable BEFORE UPDATE OF reasoning ON leases
WHEN NEW.reasoning IS NOT OLD.reasoning BEGIN
  SELECT RAISE(ABORT,'reasoning is immutable; create a new message or lease');
END;

CREATE TRIGGER audit_lease_insert AFTER INSERT ON leases BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.owner,'lease.acquired',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_object('path',NEW.path,'kind',NEW.kind,'expiresAt',NEW.expiresAt,'reasoning',NEW.reasoning));
END;

CREATE TRIGGER messages_wake_immutable BEFORE UPDATE OF wake ON messages
WHEN NEW.wake IS NOT OLD.wake BEGIN
  SELECT RAISE(ABORT,'wake is immutable; create a new message');
END;

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

CREATE TRIGGER messages_reply_policy BEFORE INSERT ON messages
WHEN NEW.replyTo IS NOT NULL AND (NEW.replyRequired<>0 OR EXISTS (
  SELECT 1 FROM messages WHERE id=NEW.replyTo AND replyRequired=0
)) BEGIN
  SELECT RAISE(ABORT,'Informational messages accept no replies; start a new request for new work');
END;

CREATE TRIGGER completion_requires_answer BEFORE UPDATE OF acknowledgedAt ON deliveries
WHEN OLD.acknowledgedAt IS NULL AND NEW.acknowledgedAt IS NOT NULL
AND EXISTS (SELECT 1 FROM messages WHERE id=OLD.message AND replyRequired=1)
AND NOT EXISTS (
  SELECT 1 FROM messages WHERE sender=OLD.recipient AND replyTo=OLD.message
    AND key='complete:'||OLD.message AND replyRequired=0
) BEGIN
  SELECT RAISE(ABORT,'Required answer missing; complete with message and reply, or leave unfinished work pending');
END;

CREATE TRIGGER audit_attachment_insert AFTER INSERT ON attachments BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(NEW.session,'attachment.created',NEW.updatedAt,json_object('transport',NEW.transport,'endpoint',NEW.endpoint));
END;

CREATE TRIGGER audit_attachment_update AFTER UPDATE ON attachments BEGIN
  INSERT INTO audit(session,kind,at,data) VALUES(NEW.session,'attachment.updated',NEW.updatedAt,json_object('transport',NEW.transport,'endpoint',NEW.endpoint));
END;

CREATE TRIGGER leases_path_key_required BEFORE INSERT ON leases
WHEN typeof(NEW.pathKey)<>'text' OR substr(NEW.pathKey,1,1)<>'/' BEGIN
  SELECT RAISE(ABORT,'pathKey is required: "/" + NFD(casefold(NFD(component))) per path component');
END;

CREATE TRIGGER leases_path_immutable BEFORE UPDATE OF workspace,path,kind,owner,pathKey,acquiredAt ON leases
WHEN NEW.workspace IS NOT OLD.workspace OR NEW.path IS NOT OLD.path OR NEW.kind IS NOT OLD.kind
  OR NEW.owner IS NOT OLD.owner OR NEW.pathKey IS NOT OLD.pathKey OR NEW.acquiredAt IS NOT OLD.acquiredAt BEGIN
  SELECT RAISE(ABORT,'Lease scope is immutable; acquire a new lease');
END;

CREATE TRIGGER documents_register AFTER INSERT ON audit WHEN NEW.kind='document.created' BEGIN
  INSERT INTO documents(id,workspace,name) SELECT NEW.id,workspace,NEW.key FROM sessions WHERE id=NEW.session;
END;

CREATE TRIGGER documents_no_update BEFORE UPDATE ON documents BEGIN
  SELECT RAISE(ABORT,'Documents are immutable');
END;

CREATE TRIGGER documents_no_delete BEFORE DELETE ON documents BEGIN
  SELECT RAISE(ABORT,'Documents are immutable');
END;

CREATE TRIGGER audit_session_update AFTER UPDATE ON sessions
WHEN NEW.name IS NOT OLD.name OR NEW.vendorSession IS NOT OLD.vendorSession OR NEW.expiresAt<OLD.expiresAt BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.id,'session.updated',NEW.id,CAST(unixepoch('subsec')*1000 AS INTEGER),json_patch('{}',json_object('name',NEW.name,'vendorSession',NEW.vendorSession,'expiresAt',NEW.expiresAt)));
END;

CREATE TRIGGER audit_message_insert AFTER INSERT ON messages BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.sender,'message.created',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_patch('{}',json_object('target',NEW.target,'topic',NEW.topic,'key',NEW.key,'reasoning',NEW.reasoning,'wake',NEW.wake,'conversationId',NEW.conversationId,'replyTo',NEW.replyTo,'replyRequired',json(CASE NEW.replyRequired WHEN 1 THEN 'true' ELSE 'false' END))));
END;

CREATE TRIGGER audit_lease_update AFTER UPDATE OF expiresAt ON leases
WHEN NEW.expiresAt IS NOT OLD.expiresAt BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(NEW.owner,'lease.renewed',CAST(NEW.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_patch('{}',json_object('expiresAt',NEW.expiresAt,'reasoning',NEW.reasoning)));
END;

CREATE TRIGGER audit_lease_delete AFTER DELETE ON leases BEGIN
  INSERT INTO audit(session,kind,entityId,at,data) VALUES(OLD.owner,'lease.removed',CAST(OLD.id AS TEXT),CAST(unixepoch('subsec')*1000 AS INTEGER),json_patch('{}',json_object('reasoning',OLD.reasoning)));
END;

CREATE TRIGGER audit_session_insert AFTER INSERT ON sessions BEGIN
  INSERT INTO audit(session,kind,entityId,at,data)
  VALUES(NEW.id,'session.created',NEW.id,CAST(unixepoch('subsec')*1000 AS INTEGER),
    json_patch('{}',json_object('name',NEW.name,'vendor',NEW.vendor,'vendorSession',NEW.vendorSession,
      'task',NEW.task,'status',NEW.status)));
END;

CREATE TRIGGER audit_session_profile AFTER UPDATE OF task,status ON sessions
WHEN NEW.task IS NOT OLD.task OR NEW.status IS NOT OLD.status BEGIN
  INSERT INTO audit(session,kind,entityId,at,data)
  VALUES(NEW.id,'session.profile',NEW.id,CAST(unixepoch('subsec')*1000 AS INTEGER),
    json_object('task',NEW.task,'status',NEW.status));
END;

CREATE TRIGGER session_directory_insert AFTER INSERT ON sessions BEGIN
  INSERT INTO peer_revisions VALUES(NEW.workspace,1)
  ON CONFLICT(workspace) DO UPDATE SET revision=revision+1;
END;

CREATE TRIGGER session_directory_update AFTER UPDATE OF name,task,status,vendorSession,expiresAt ON sessions
WHEN NEW.name IS NOT OLD.name OR NEW.task IS NOT OLD.task OR NEW.status IS NOT OLD.status
  OR NEW.vendorSession IS NOT OLD.vendorSession OR NEW.expiresAt<OLD.expiresAt
  OR (OLD.expiresAt<=CAST(unixepoch('subsec')*1000 AS INTEGER) AND NEW.expiresAt>OLD.expiresAt) BEGIN
  UPDATE peer_revisions SET revision=revision+1 WHERE workspace=NEW.workspace;
END;

CREATE TRIGGER session_directory_delete AFTER DELETE ON sessions BEGIN
  UPDATE peer_revisions SET revision=revision+1 WHERE workspace=OLD.workspace;
END;
