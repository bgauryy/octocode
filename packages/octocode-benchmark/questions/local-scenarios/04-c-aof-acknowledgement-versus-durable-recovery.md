# AOF acknowledgement versus durable recovery

## Goal

Investigate the following scenario: A process or host crashes immediately after a successful write reply.

## Questions

- Which acknowledged Redis writes survive a process crash or power loss under each AOF policy?
- How do append, flush, fsync, and rewrite affect that guarantee?
- Where can recovery diverge from the last reply observed by the client?
