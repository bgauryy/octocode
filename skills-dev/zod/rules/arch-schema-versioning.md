---
title: Evolve Schemas Without Breaking Consumers
impact: MEDIUM
description: Check input and output compatibility separately; additive fields or union members can still break strict or exhaustive consumers.
tags: architecture, versioning, backward-compatibility, api-evolution
---

# Evolve Schemas Without Breaking Consumers

## Problem

Removing or tightening fields in a Zod schema silently breaks consumers. Because `z.object()` strips unknown keys by default, removed fields vanish from parsed output without any error — consumers reading that field get `undefined` at runtime despite TypeScript showing no compile error if they haven't updated their types.

## Incorrect

```typescript
// v1 — consumers depend on `role`
const UserV1 = z.object({
  name: z.string(),
  email: z.email(),
  role: z.enum(["admin", "user"]),
  nickname: z.string(),
})

// v2 — BREAKING: removed `role`, consumers silently get undefined
const UserV2 = z.object({
  name: z.string(),
  email: z.email(),
  nickname: z.string(),
  // role is gone — consumers calling user.role get undefined
})
```

## Correct

```typescript
// Accepts old payloads: keep role, add displayName as optional, deprecate nickname
const UserV2 = z.object({
  name: z.string(),
  email: z.email(),
  role: z.enum(["admin", "user"]),         // kept — not removed
  nickname: z.string(),                     // kept — deprecate in docs, remove in v3
  displayName: z.string().optional(),       // new — optional so old payloads still parse
})

type UserV2 = z.infer<typeof UserV2>
```

## Decision Table: Schema Changes

| Change | Input acceptance | Consumer check |
|--------|-----------|---------------|
| Add optional field | Old payloads still pass | Old strict response parsers may reject the new field |
| Add required field | Old payloads may fail | Coordinate producers and consumers |
| Remove field | Parser may strip it or reject it | Output consumers may still require it |
| Tighten constraint | Previously valid data may fail | Check actual producer values |
| Loosen constraint | Existing input still passes | Downstream code may assume the old guarantee |
| Rename or change type | Existing payloads may fail | Use the project's migration/versioning policy |
| Add union member | Existing input still passes | Exhaustive consumers may not handle the new variant |
| Remove union member | Old variant fails | Migrate existing data and producers |

## Why

Compatibility has a direction: accepting old input does not prove the new output is safe for old consumers. Check both sides, including transforms, unknown-key policy and exhaustive handlers. Apply the project's release policy and migration contract; a major version number alone does not migrate consumers.
