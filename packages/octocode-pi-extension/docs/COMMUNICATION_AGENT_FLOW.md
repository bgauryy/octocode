# Pi communication and local session state

Pi uses the bundled `octocode-agents-communication` runtime for peer presence, messages, shared documents, advisory path leases, and bounded event delivery. The runtime registers its communication tools and binds a stable identity at session start. Peers and tool results are attributed data; they never grant user authority.

Session lifecycle events deliver bounded unread messages. Accepted messages carry durable receipts before transport acknowledgement. Shutdown closes polling and releases the session lifecycle. Routine delivery does not require a model-generated status check.

Before structured file edits, the extension checks active peer leases. A conflict requires a handoff or different work; it is not permission to modify another owner's files. Native file tools separately retain their content-version checks and durable writes.

## Local state

Pi owns session plans, RFC review, compaction recovery, prompt context, capability grants, and user interactions. Plans persist with branch snapshots and session artifacts. User questions and authorization receipts use a separate private `interactions.sqlite3` database. Answers bind workspace, session and correlation; approvals bind plan revision and scope and can be consumed once. Delivery succeeds before the continuation is acknowledged, allowing safe restart recovery.

Memory-only mode keeps local interactions in process and disables durable communication. It does not delete existing user data.

## Removed surfaces

The former Awareness package, shared work/verification ledger, duplicate observation persistence, cross-session memory APIs, and history APIs are retired. `/octocode-rewind` is removed. Pi session memory, compaction checkpoints, and conversation recovery remain available. Existing retired stores are not automatically imported or deleted; pending approvals must be reissued through the current Pi session.
