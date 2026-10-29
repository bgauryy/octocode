---
title: Refine for Validation, Transform for Conversion
impact: HIGH
description: Use .refine() for validation (returns boolean), .transform() for shape conversion (returns new value).
tags: refine, transform, pipe, validation
---

# Refine for Validation, Transform for Conversion

## Problem

Using `.transform()` for validation (throwing on invalid input) or `.refine()` for type conversion mixes concerns and produces confusing errors.

## Incorrect

```typescript
// BUG: validation inside transform — throws instead of returning ZodError
const SafeInt = z.string().transform((val) => {
  const n = parseInt(val, 10)
  if (isNaN(n)) throw new Error("Not a number") // bypasses Zod errors
  return n
})
```

## Correct

```typescript
// GOOD: validate the whole decimal integer, then convert and check range
const SafeInt = z.string()
  .regex(/^[+-]?\d+$/, { error: "Must be a decimal integer" })
  .transform(Number)
  .pipe(z.int())

// Alternative: JavaScript numeric syntax, with empty strings rejected
const SafeInt = z.string()
  .trim().min(1)
  .pipe(z.coerce.number())
  .pipe(z.int())
```

## Why

`.refine()` = "is this valid?" (returns boolean, adds issue if false). `.transform()` = "convert this value" (returns the new value). `.pipe()` = "parse with schema A, then feed the result to schema B." Keeping them separate makes schemas readable, composable, and error messages accurate.
