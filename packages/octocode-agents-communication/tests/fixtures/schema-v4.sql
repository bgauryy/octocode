PRAGMA application_id=1329678147;
PRAGMA user_version=4;

CREATE TABLE workspaces (
  workspace TEXT PRIMARY KEY, coordinationScope TEXT NOT NULL
);
CREATE INDEX workspaces_scope ON workspaces(coordinationScope,workspace);
CREATE TRIGGER workspaces_immutable BEFORE UPDATE ON workspaces BEGIN
  SELECT RAISE(ABORT,'Workspace coordination scope is immutable');
END;

CREATE TABLE sessions (
  id TEXT PRIMARY KEY, workspace TEXT NOT NULL, name TEXT NOT NULL, vendor TEXT NOT NULL,
  vendorSession TEXT, branch TEXT, expiresAt INTEGER NOT NULL,
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

CREATE TABLE records (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  path TEXT NOT NULL CHECK(length(trim(path))>0),
  "from" TEXT NOT NULL REFERENCES sessions(id), "to" TEXT,
  type TEXT NOT NULL CHECK(length(trim(type))>0),
  timestamp INTEGER NOT NULL CHECK(typeof(timestamp)='integer' AND timestamp>=0),
  branch TEXT, data TEXT NOT NULL CHECK(json_valid(data)),
  entityId TEXT, key TEXT, UNIQUE("from",type,key)
);

CREATE TABLE "attachments" (
  session TEXT PRIMARY KEY REFERENCES sessions(id),
  transport TEXT NOT NULL CHECK(transport IN ('raw','claude','codex','opencode','grok')),
  endpoint TEXT, updatedAt INTEGER NOT NULL
);

CREATE TABLE documents (
  id INTEGER PRIMARY KEY REFERENCES records(id),
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

CREATE INDEX records_from ON records(path,"from",id);
CREATE INDEX records_to ON records(path,"to",id);
CREATE INDEX records_type ON records(path,type,id);
CREATE INDEX records_time ON records(path,timestamp,id);
CREATE INDEX records_branch ON records(path,branch,type,id);
CREATE INDEX records_entity ON records(type,entityId,id);
CREATE INDEX records_dispatch ON records(path,"from",entityId,id) WHERE type GLOB 'dispatch.*';
CREATE VIRTUAL TABLE records_search USING fts5(text, tokenize='unicode61');

CREATE INDEX dispatches_state ON dispatches(recipient,state,message);

CREATE INDEX messages_conversation ON messages(conversationId,id) WHERE conversationId IS NOT NULL;

CREATE INDEX messages_reply ON messages(replyTo,id) WHERE replyTo IS NOT NULL;

CREATE INDEX leases_path ON leases(workspace,pathKey);

CREATE INDEX leases_owner ON leases(owner);

CREATE INDEX sessions_scope ON sessions(workspace,expiresAt);

CREATE INDEX sessions_host ON sessions(workspace,vendorSession) WHERE vendorSession IS NOT NULL;

CREATE INDEX subscriptions_topic ON subscriptions(topic);

CREATE INDEX documents_scan ON documents(workspace,id);

CREATE TRIGGER records_no_update BEFORE UPDATE ON records BEGIN
  SELECT RAISE(ABORT,'Records are append-only');
END;

CREATE TRIGGER records_no_delete BEFORE DELETE ON records BEGIN
  SELECT RAISE(ABORT,'Records are append-only');
END;

CREATE TRIGGER message_immutable BEFORE UPDATE ON messages
WHEN NEW.sender IS NOT OLD.sender OR NEW.target IS NOT OLD.target OR NEW.topic IS NOT OLD.topic OR NEW.body IS NOT OLD.body OR NEW.key IS NOT OLD.key OR NEW.ttlMs IS NOT OLD.ttlMs OR NEW.replyRequired IS NOT OLD.replyRequired BEGIN
  SELECT RAISE(ABORT,'Message content is immutable');
END;

CREATE TRIGGER record_claim_update AFTER UPDATE OF claimedBy ON deliveries
WHEN NEW.claimedBy IS NOT NULL AND NEW.claimedBy IS NOT OLD.claimedBy BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.recipient),NEW.recipient,(SELECT sender FROM messages WHERE id=NEW.message),'delivery.claimed',CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=NEW.recipient),json_set(json_object('owner',NEW.claimedBy),'$.messageId',NEW.message),CAST(NEW.message AS TEXT),NULL);
END;

CREATE TRIGGER record_delivery_insert AFTER INSERT ON deliveries BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.recipient),NEW.recipient,(SELECT sender FROM messages WHERE id=NEW.message),'delivery.created',CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=NEW.recipient),json_set('{}','$.messageId',NEW.message),CAST(NEW.message AS TEXT),NULL);
END;

CREATE TRIGGER record_delivery_ack AFTER UPDATE OF acknowledgedAt ON deliveries
WHEN OLD.acknowledgedAt IS NULL AND NEW.acknowledgedAt IS NOT NULL BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.recipient),NEW.recipient,(SELECT sender FROM messages WHERE id=NEW.message),'delivery.acknowledged',NEW.acknowledgedAt,(SELECT branch FROM sessions WHERE id=NEW.recipient),json_set('{}','$.messageId',NEW.message),CAST(NEW.message AS TEXT),NULL);
END;

CREATE TRIGGER record_subscription_insert AFTER INSERT ON subscriptions BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.session),NEW.session,NULL,'subscription.added',CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=NEW.session),json_object('topic',NEW.topic),NULL,NULL);
END;

CREATE TRIGGER record_subscription_delete AFTER DELETE ON subscriptions BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=OLD.session),OLD.session,NULL,'subscription.removed',CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=OLD.session),json_object('topic',OLD.topic),NULL,NULL);
END;

CREATE TRIGGER record_dispatch_insert AFTER INSERT ON dispatches BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.recipient),NEW.recipient,(SELECT sender FROM messages WHERE id=NEW.message),'dispatch.staged',NEW.attemptedAt,(SELECT branch FROM sessions WHERE id=NEW.recipient),json_set(json_object('token',NEW.token,'transport',NEW.transport),'$.messageId',NEW.message),CAST(NEW.message AS TEXT),NULL);
END;

CREATE TRIGGER record_dispatch_update AFTER UPDATE ON dispatches BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.recipient),NEW.recipient,(SELECT sender FROM messages WHERE id=NEW.message),'dispatch.'||NEW.state,CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=NEW.recipient),json_set(json_object('token',NEW.token,'transport',NEW.transport,'error',NEW.error),'$.messageId',NEW.message),CAST(NEW.message AS TEXT),NULL);
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

CREATE TRIGGER record_lease_insert AFTER INSERT ON leases BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.owner),NEW.owner,NULL,'lease.acquired',CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=NEW.owner),json_set(json_object('path',NEW.path,'kind',NEW.kind,'expiresAt',NEW.expiresAt,'reasoning',NEW.reasoning),'$.leaseId',CAST(CAST(NEW.id AS TEXT) AS INTEGER)),CAST(NEW.id AS TEXT),NULL);
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
  WHERE parent.id=NEW.replyTo AND coalesce((SELECT coordinationScope FROM workspaces WHERE workspace=author.workspace),author.workspace)=coalesce((SELECT coordinationScope FROM workspaces WHERE workspace=sender.workspace),sender.workspace)
    AND (parent.sender=NEW.sender OR EXISTS (SELECT 1 FROM deliveries d WHERE d.message=parent.id AND d.recipient=NEW.sender))
    AND NEW.conversationId IS parent.conversationId
) BEGIN
  SELECT RAISE(ABORT,'Reply requires a visible parent in this repository and its conversationId');
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

CREATE TRIGGER record_attachment_insert AFTER INSERT ON attachments BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.session),NEW.session,NULL,'attachment.created',NEW.updatedAt,(SELECT branch FROM sessions WHERE id=NEW.session),json_object('transport',NEW.transport,'endpoint',NEW.endpoint),NULL,NULL);
END;

CREATE TRIGGER record_attachment_update AFTER UPDATE ON attachments BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.session),NEW.session,NULL,'attachment.updated',NEW.updatedAt,(SELECT branch FROM sessions WHERE id=NEW.session),json_object('transport',NEW.transport,'endpoint',NEW.endpoint),NULL,NULL);
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

CREATE TRIGGER documents_register AFTER INSERT ON records WHEN NEW.type='document' BEGIN
  INSERT INTO documents(id,workspace,name) SELECT NEW.id,workspace,NEW.key FROM sessions WHERE id=NEW."from";
END;

CREATE TRIGGER documents_no_update BEFORE UPDATE ON documents BEGIN
  SELECT RAISE(ABORT,'Documents are immutable');
END;

CREATE TRIGGER documents_no_delete BEFORE DELETE ON documents BEGIN
  SELECT RAISE(ABORT,'Documents are immutable');
END;

CREATE TRIGGER record_session_update AFTER UPDATE ON sessions
WHEN NEW.branch IS NOT OLD.branch OR NEW.name IS NOT OLD.name OR NEW.vendorSession IS NOT OLD.vendorSession OR NEW.branch IS NOT OLD.branch OR NEW.expiresAt<OLD.expiresAt
  OR (OLD.expiresAt<=CAST(unixepoch('subsec')*1000 AS INTEGER) AND NEW.expiresAt>OLD.expiresAt) BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.id),NEW.id,NULL,CASE WHEN NEW.expiresAt<OLD.expiresAt AND NEW.expiresAt<=CAST(unixepoch('subsec')*1000 AS INTEGER) THEN 'coordinate.out' ELSE 'coordinate.update' END,CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=NEW.id),json_patch('{}',json_object('name',NEW.name,'vendor',NEW.vendor,'vendorSession',NEW.vendorSession,'task',NEW.task,'status',NEW.status,'expiresAt',NEW.expiresAt)),NEW.id,NULL);
END;

CREATE TRIGGER record_message_insert AFTER INSERT ON messages BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.sender),NEW.sender,CASE WHEN NEW.topic IS NULL THEN NEW.target ELSE 'topic:'||NEW.topic END,'message',CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=NEW.sender),json_set(json_patch('{}',json_object('topic',NEW.topic,'key',NEW.key,'reasoning',NEW.reasoning,'wake',NEW.wake,'conversationId',NEW.conversationId,'replyTo',NEW.replyTo,'replyRequired',json(CASE NEW.replyRequired WHEN 1 THEN 'true' ELSE 'false' END))),'$.messageId',NEW.id),CAST(NEW.id AS TEXT),NULL);
END;

CREATE TRIGGER record_lease_update AFTER UPDATE OF expiresAt ON leases
WHEN NEW.expiresAt IS NOT OLD.expiresAt BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.owner),NEW.owner,NULL,'lease.renewed',CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=NEW.owner),json_set(json_object('path',NEW.path,'kind',NEW.kind,'expiresAt',NEW.expiresAt,'reasoning',NEW.reasoning),'$.leaseId',CAST(CAST(NEW.id AS TEXT) AS INTEGER)),CAST(NEW.id AS TEXT),NULL);
END;

CREATE TRIGGER record_lease_delete AFTER DELETE ON leases BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=OLD.owner),OLD.owner,NULL,'lease.removed',CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=OLD.owner),json_set(json_object('path',OLD.path,'kind',OLD.kind,'expiresAt',OLD.expiresAt,'reasoning',OLD.reasoning),'$.leaseId',CAST(CAST(OLD.id AS TEXT) AS INTEGER)),CAST(OLD.id AS TEXT),NULL);
END;

CREATE TRIGGER record_session_insert AFTER INSERT ON sessions BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.id),NEW.id,NULL,'coordinate.in',CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=NEW.id),json_patch('{}',json_object('name',NEW.name,'vendor',NEW.vendor,'vendorSession',NEW.vendorSession,
      'task',NEW.task,'status',NEW.status,'expiresAt',NEW.expiresAt)),NEW.id,NULL);
END;

CREATE TRIGGER record_session_profile AFTER UPDATE OF task,status ON sessions
WHEN NEW.task IS NOT OLD.task OR NEW.status IS NOT OLD.status BEGIN
  INSERT INTO records(path,"from","to",type,timestamp,branch,data,entityId,key) VALUES((SELECT workspace FROM sessions WHERE id=NEW.id),NEW.id,NULL,'coordinate.profile',CAST(unixepoch('subsec')*1000 AS INTEGER),(SELECT branch FROM sessions WHERE id=NEW.id),json_patch('{}',json_object('name',NEW.name,'vendor',NEW.vendor,'vendorSession',NEW.vendorSession,'task',NEW.task,'status',NEW.status,'expiresAt',NEW.expiresAt)),NEW.id,NULL);
END;

CREATE TRIGGER session_directory_insert AFTER INSERT ON sessions BEGIN
  INSERT INTO peer_revisions VALUES(coalesce((SELECT coordinationScope FROM workspaces WHERE workspace=NEW.workspace),NEW.workspace),1)
  ON CONFLICT(workspace) DO UPDATE SET revision=revision+1;
END;

CREATE TRIGGER session_directory_update AFTER UPDATE OF name,task,status,vendorSession,branch,expiresAt ON sessions
WHEN NEW.name IS NOT OLD.name OR NEW.task IS NOT OLD.task OR NEW.status IS NOT OLD.status
  OR NEW.vendorSession IS NOT OLD.vendorSession OR NEW.branch IS NOT OLD.branch OR NEW.expiresAt<OLD.expiresAt
  OR (OLD.expiresAt<=CAST(unixepoch('subsec')*1000 AS INTEGER) AND NEW.expiresAt>OLD.expiresAt) BEGIN
  UPDATE peer_revisions SET revision=revision+1 WHERE workspace=coalesce((SELECT coordinationScope FROM workspaces WHERE workspace=NEW.workspace),NEW.workspace);
END;

CREATE TRIGGER session_directory_delete AFTER DELETE ON sessions BEGIN
  UPDATE peer_revisions SET revision=revision+1 WHERE workspace=coalesce((SELECT coordinationScope FROM workspaces WHERE workspace=OLD.workspace),OLD.workspace);
END;

CREATE TRIGGER records_scope BEFORE INSERT ON records
WHEN NOT EXISTS(SELECT 1 FROM sessions WHERE id=NEW."from" AND workspace=NEW.path) BEGIN
  SELECT RAISE(ABORT,'Record path must match its author workspace');
END;

CREATE TRIGGER records_index AFTER INSERT ON records BEGIN
  INSERT INTO records_search(rowid,text) VALUES(NEW.id,
    NEW.data || ' ' || coalesce((SELECT body FROM messages WHERE NEW.type='message' AND id=CAST(NEW.entityId AS INTEGER)),''));
END;
