# Cache expiration versus refresh

## Goal

Investigate the following scenario: A frequently accessed cache entry becomes eligible for refresh while its loader is slow.

## Questions

- How do refresh and expiration interact when a Guava cache loader is slow or fails?
- When do callers receive an old value, block, or observe a missing entry?
- How do cleanup and access patterns affect these outcomes?
