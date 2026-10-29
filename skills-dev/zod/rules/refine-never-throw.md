---
title: Return Issues for Expected Validation Failures
impact: HIGH
description: Return false or add issues for expected validation failures in refinements and transforms; unexpected callback exceptions still propagate.
tags: refine, superRefine, throw, error
---

# Return Issues for Expected Validation Failures

## Problem

Throwing for an expected invalid value inside `.refine()` or `.transform()` bypasses Zod's validation result. The exception is not wrapped in a `ZodError`; even `safeParse()` will propagate it. An unexpected dependency failure is a separate application error and should not be disguised as invalid input.

## Incorrect

```typescript
// BUG: throwing inside refine bypasses Zod error handling
const PositiveNumber = z.number().refine((n) => {
  if (n <= 0) throw new Error("Must be positive") // crashes parse
  return true
})
```

## Correct

```typescript
// GOOD: return boolean from refine
const PositiveNumber = z.number().refine(
  (n) => n > 0,
  { error: "Must be positive" }
)

// GOOD: use superRefine with ctx.addIssue for complex logic
const Password = z.string().superRefine((val, ctx) => {
  if (val.length < 8) {
    ctx.addIssue({
      code: "custom",
      message: "Password must be at least 8 characters",
    })
  }
  if (!/[A-Z]/.test(val)) {
    ctx.addIssue({
      code: "custom",
      message: "Password must contain an uppercase letter",
    })
  }
})
```

## Why

`.refine()` expects a boolean return (or a Promise<boolean> for async). `.superRefine()` with `ctx.addIssue()` allows multiple issues per field. Use these for values the consumer may normally reject. Handle unexpected callback or dependency exceptions at the application boundary.
