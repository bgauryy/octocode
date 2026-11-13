# Suspense streaming and late server failures

## Goal

Investigate the following scenario: A Next.js app route starts streaming and then a nested server component fails.

## Questions

- What can Next.js still change when a server component fails after response streaming has started?
- How do Suspense boundaries and client recovery affect the visible result?
- How do ordinary errors differ from redirects and not-found outcomes?
