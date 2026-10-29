---
title: Use Zod Mini for Client Bundles
impact: MEDIUM
description: Consider Zod Mini when measured client bundle constraints justify its functional API; compare the actual bundle and integrations.
tags: bundle-size, mini, client, tree-shaking
---

# Use Zod Mini for Client Bundles

## Problem

A bundle budget may justify a more tree-shakable API. Bundle size depends on the schema, version, imports and bundler; a published minimal example is not a guarantee for an application.

## Incorrect

```typescript
// Existing full-Zod form: keep it unless measurement justifies migration
import { z } from "zod"

const LoginForm = z.object({
  email: z.email(),
  password: z.string().min(8),
})
```

## Correct

```typescript
// Candidate: measure this Mini form with the same bundler and behavior tests
import { z } from "zod/v4/mini"

const LoginForm = z.object({
  email: z.email(),
  password: z.string().check(z.minLength(8)),
})
```

## Why

Zod Mini uses functions and `.check()` in place of most chainable methods. Check form-library compatibility and maintainability before switching. See the [official Mini guide](https://zod.dev/packages/mini); measure the production bundle rather than assigning a fixed size to the package.

| | Full Zod | Zod Mini |
|---|---|---|
| Bundle cost | Measure the actual imports and schema | Measure with the same configuration |
| API style | Method chaining (`.min()`, `.max()`) | Functional (`.check(z.minLength())`) |
| Composition | Chainable methods | Functional equivalents; JSON Schema conversion is available |
| Use when | Existing API and integrations fit | Measured bundle savings justify API changes |
