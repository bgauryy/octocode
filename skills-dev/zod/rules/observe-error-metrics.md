---
title: Track Validation Error Rates per Schema and Field
impact: MEDIUM
description: Add bounded validation metrics when operations need them; use rates to investigate failures, not to infer that constraints are wrong.
tags: observability, metrics, monitoring, validation, production
---

# Track Validation Error Rates per Schema and Field

## Problem

When operations need validation trends, counters can reveal changes that individual logs miss. A high or low rate does not by itself tell you whether the schema is too strict or too loose.

## Incorrect

```typescript
// No monitoring — validation is a black box
app.post("/api/orders", (req, res) => {
  const result = OrderSchema.safeParse(req.body)
  if (!result.success) {
    return res.status(400).json({ error: "Invalid" })
  }
  // How often does this fail? Which fields? No one knows.
})
```

## Correct

```typescript
import { z } from "zod"

// Tracked safeParse wrapper
function trackedSafeParse<T extends z.ZodType>(
  schema: T,
  data: unknown,
  schemaName: string,
  knownFields: ReadonlySet<string>
): z.ZodSafeParseResult<z.output<T>> {
  const result = schema.safeParse(data)

  metrics.increment("zod.validation.attempt", { schema: schemaName })

  if (!result.success) {
    metrics.increment("zod.validation.failure", { schema: schemaName })

    // Track which fields fail
    const flat = z.flattenError(result.error)
    for (const field of Object.keys(flat.fieldErrors)) {
      metrics.increment("zod.validation.field_error", {
        schema: schemaName,
        field: knownFields.has(field) ? field : "other",
      })
    }
  }

  return result
}

// Stable labels derived from the application-owned object schema
const orderMetricFields = new Set(Object.keys(OrderSchema.shape))

// Usage
app.post("/api/orders", (req, res) => {
  const result = trackedSafeParse(OrderSchema, req.body, "OrderSchema", orderMetricFields)
  if (!result.success) {
    return res.status(400).json({
      errors: z.flattenError(result.error).fieldErrors,
    })
  }
  processOrder(result.data)
})
```

## What the Metrics Tell You

| Signal | Possible cause | Investigation |
|--------|---------|--------|
| High failure rate on a schema | Schema may be too strict | Review constraints, check if users send valid data that gets rejected |
| One field dominates failures | Format confusion, changed traffic or a producer bug | Inspect representative safe cases before changing coercion or constraints |
| Schema never fails | Valid traffic, upstream validation or missing checks | Verify boundary tests and instrumentation; zero failures alone proves none of these |
| Failure spike after deploy | Schema, producer or traffic changed | Compare deployments and evidence before rollback or relaxation |

## Why

Use metrics when operational questions need them. Keep schema names and `orderMetricFields` application-defined and bounded; do not turn arbitrary record keys or user values into metric labels. Rates describe observed traffic, not correctness. This wrapper is synchronous: async schemas need an awaited async counterpart and coverage for its failure path.
