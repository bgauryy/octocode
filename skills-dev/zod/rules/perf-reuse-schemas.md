---
title: Reuse Schemas with Composition
impact: MEDIUM
description: Define schemas once. Use .pick(), .omit(), .partial() to derive variants.
tags: composition, pick, omit, partial, DRY
---

# Reuse Schemas with Composition

## Problem

Duplicating field definitions across related schemas (Create, Update, Response) leads to drift. When a field changes, only some schemas get updated.

## Incorrect

```typescript
// BAD: fields duplicated across schemas
const UserCreate = z.object({
  name: z.string(),
  email: z.email(),
  password: z.string().min(8),
})

const UserUpdate = z.object({
  name: z.string().optional(),
  email: z.email().optional(),
  // Shared name/email constraints must be kept in sync manually
})

const UserResponse = z.object({
  id: z.string(),
  name: z.string(),
  email: z.email(),
  createdAt: z.date(),
})
```

## Correct

```typescript
// GOOD: single source, derive variants
const User = z.object({
  id: z.string(),
  name: z.string(),
  email: z.email(),
  password: z.string().min(8),
  createdAt: z.date(),
})

const UserCreate = User.omit({ id: true, createdAt: true })
const UserUpdate = User.pick({ name: true, email: true }).partial()
const UserResponse = User.pick({ id: true, name: true, email: true, createdAt: true })
```

## Why

`pick()`, `omit()`, `partial()`, and `required()` derive variants when field semantics are shared. Keep update permissions explicit; excluding password may be intentional. Allowlist response fields with `pick()` so adding another secret to the base does not expose it through an omission-based response.
