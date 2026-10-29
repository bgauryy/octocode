---
title: Spread Over Extend for Large Schemas
impact: MEDIUM
description: Use spread syntax with .shape over .extend() for large schemas to reduce compile-time cost.
tags: performance, extend, spread, typescript
---

# Spread Over Extend for Large Schemas

## Problem

`.extend()` creates a new schema type at the TypeScript level, which is expensive at compile time. Chaining multiple `.extend()` calls compounds this cost.

## Incorrect

```typescript
// BAD: each .extend() creates a new TS type — slow compile
const Base = z.object({ id: z.string(), created: z.date() })
const WithName = Base.extend({ name: z.string() })
const WithEmail = WithName.extend({ email: z.email() })
const WithRole = WithEmail.extend({ role: z.enum(["admin", "user"]) })
// 4 intermediate types for TypeScript to resolve
```

## Correct

```typescript
const Base = z.object({ id: z.string(), created: z.date() })
const Extra = { name: z.string(), email: z.email(), role: z.enum(["admin", "user"]) }

// Candidate: compose the shape without a chain of extensions
const Full = z.object({ ...Base.shape, ...Extra })
```

## Why

`.extend()` is fine for one-off extensions. For large chains, measure TypeScript checking before and after spreading `.shape`. A new `z.object` does not inherit object refinements, metadata or strict/loose/catchall behavior: preserve those deliberately, or use `.safeExtend()` when retaining refinements. Verify behavior before treating this as a performance-only change.
