# Concurrent cache loading and invalidation

## Goal

Investigate the following scenario: Two threads request an absent key while another thread invalidates it.

## Questions

- What happens when two threads load the same missing Guava cache key while another invalidates it?
- Can a completed load become visible after invalidation?
- Which interleavings are covered by the cache guarantees?
