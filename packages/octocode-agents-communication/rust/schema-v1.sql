
        CREATE TABLE sessions (
          id TEXT PRIMARY KEY, workspace TEXT NOT NULL, name TEXT NOT NULL, vendor TEXT NOT NULL,
          vendorSession TEXT, expiresAt INTEGER NOT NULL
        );
        CREATE TABLE subscriptions (
          session TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE, topic TEXT NOT NULL,
          PRIMARY KEY(session, topic)
        );
        CREATE TABLE leases (
          id INTEGER PRIMARY KEY AUTOINCREMENT, workspace TEXT NOT NULL, path TEXT NOT NULL,
          kind TEXT NOT NULL CHECK(kind IN ('file','tree')), owner TEXT NOT NULL REFERENCES sessions(id),
          expiresAt INTEGER NOT NULL
        );
        CREATE INDEX leases_scope ON leases(workspace, expiresAt);
        CREATE TABLE messages (
          id INTEGER PRIMARY KEY AUTOINCREMENT, sender TEXT NOT NULL REFERENCES sessions(id),
          target TEXT NOT NULL, topic TEXT, body TEXT NOT NULL, key TEXT NOT NULL,
          expiresAt INTEGER NOT NULL, UNIQUE(sender, key)
        );
        CREATE TABLE deliveries (
          message INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
          recipient TEXT NOT NULL REFERENCES sessions(id), acknowledgedAt INTEGER,
          claimedBy TEXT, claimUntil INTEGER NOT NULL DEFAULT 0,
          PRIMARY KEY(message, recipient)
        );
        CREATE INDEX deliveries_inbox ON deliveries(recipient, acknowledgedAt, message);
