# Revalidation and stale response races

## Goal

Investigate the following scenario: A cached route is invalidated while concurrent requests read or regenerate it.

## Questions

- What happens when Next.js cache invalidation races with reads or regeneration?
- How do cache keys, invalidation scope, and concurrent regeneration determine which response is served?
- When is a stale response expected, and when does it indicate missed invalidation?
