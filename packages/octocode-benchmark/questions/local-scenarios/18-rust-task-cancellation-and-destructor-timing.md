# Task cancellation and destructor timing

## Goal

Investigate the following scenario: A spawned Tokio task owns a resource and is aborted while waiting.

## Questions

- When does aborting a Tokio task destroy its future and release owned resources?
- How do abort, dropping a join handle, blocking work, and runtime shutdown differ?
- Which task states can delay or prevent cleanup?
