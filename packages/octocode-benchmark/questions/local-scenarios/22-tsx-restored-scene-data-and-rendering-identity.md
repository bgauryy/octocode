# Restored scene data and rendering identity

## Goal

Investigate the following scenario: An imported scene contains stale or malformed element references and renders incorrectly.

## Questions

- How does Excalidraw handle imported scenes with invalid element bindings or stale references?
- Which invariants are repaired before rendering, and which are assumed?
- How can restoration problems differ from stale component state or memoization problems?
