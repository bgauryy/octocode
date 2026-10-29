---
title: Co-locate Schemas with Their Boundary Layer
impact: HIGH
description: Place schemas near their owning boundary; derive types without making domain modules import API modules.
tags: architecture, organization, co-location, project-structure
---

# Co-locate Schemas with Their Boundary Layer

## Problem

Dumping all schemas into a single `schemas/` folder creates a grab bag of unrelated schemas with no indication of where they're used. Schemas for API routes, form validation, and env parsing all mix together, making it unclear which boundary owns which schema.

## Incorrect

```
// BUG: everything in one folder — no layering, unclear ownership
src/
  schemas/
    user.ts          // used by API AND form AND env?
    order.ts
    config.ts
    form-profile.ts
    api-response.ts
    db-result.ts
```

```typescript
// BUG: domain module imports and re-exports raw schemas
// src/domain/user.ts
import { UserSchema } from "../schemas/user"
export { UserSchema } // leaking parsing into domain
```

## Correct

```
src/
  api/
    users/
      route.ts
      schemas.ts       // API request/response schemas for /users
    orders/
      route.ts
      schemas.ts       // API schemas for /orders
  features/
    profile/
      form-schema.ts   // form validation schema
      ProfileForm.tsx
  config/
    env.ts             // env var schema, parsed at startup
  domain/
    types.ts           // domain-owned types and invariants
```

```typescript
// api/users/schemas.ts — co-located with the route
import { z } from "zod"

export const CreateUserBody = z.object({
  name: z.string().min(1),
  email: z.email(),
})

export const UserResponse = z.object({
  id: z.string(),
  name: z.string(),
  email: z.email(),
})

// api/users/schemas.ts — expose boundary input/output types with the schema
export type CreateUserBody = z.input<typeof CreateUserBody>
export type UserResponse = z.output<typeof UserResponse>

// Domain modules do not import from api/users/schemas.ts.
```

## Why

Co-location makes ownership clear: the API route handler and its schemas live in the same directory. Derive boundary types from their schemas, including `z.input` when transforms make input and output differ. A domain type may have a separate owner and invariants; map validated boundary output into it at the adapter. Do not reverse the dependency by importing API types into the domain.
