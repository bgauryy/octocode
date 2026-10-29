---
title: "v4: Unified z.enum()"
impact: MEDIUM
description: Use unified z.enum() which now accepts TypeScript enums directly. z.nativeEnum() is deprecated.
tags: migration, v4, enum, nativeEnum
---

# v4: Unified z.enum()

## Problem

Zod v3 had separate `z.enum()` (string arrays) and `z.nativeEnum()` (TypeScript/JS enums). In v4, `z.enum()` handles both. `z.nativeEnum()` is deprecated.

## Incorrect

```typescript
// BAD: z.nativeEnum() is deprecated in v4
enum Role {
  Admin = "admin",
  User = "user",
  Guest = "guest",
}

const RoleSchema = z.nativeEnum(Role) // still works; deprecated in v4
```

## Correct

```typescript
enum Role {
  Admin = "admin",
  User = "user",
  Guest = "guest",
}

// GOOD: z.enum() now accepts TypeScript enums directly
const RoleSchema = z.enum(Role)

// Also still works with string arrays
const StatusSchema = z.enum(["active", "inactive", "pending"])
```

## Why

Zod v4 unifies enum handling. `z.enum()` accepts both string literal arrays and TypeScript enums (string or numeric). Prefer it for new v4 code; the deprecated `z.nativeEnum()` remains available.
