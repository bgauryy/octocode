---
title: Log Structured Errors, Not Raw ZodError
impact: HIGH
description: Choose explicit error fields and correlation IDs for the log consumer; formatting alone does not redact sensitive values.
tags: observability, logging, errors, flattenError, structured-logging
---

# Log Structured Errors, Not Raw ZodError

## Problem

Logging an Error object depends on the logger's serializer and may omit useful fields. Select an explicit shape for the consumer. `flattenError` groups by top-level field; use issue paths or `treeifyError` when nested detail matters. Formatting does not sanitize custom messages or user-controlled keys.

## Incorrect

```typescript
// Fragile: behavior depends on the logger's Error serializer
app.post("/api/users", (req, res) => {
  const result = UserSchema.safeParse(req.body)
  if (!result.success) {
    logger.error("Validation failed", { error: result.error })
    // Check which fields the serializer actually retains
    return res.status(400).json({ error: "Invalid input" })
  }
})
```

```typescript
// BUG: console.log — no structure, no correlation
if (!result.success) {
  console.log("Validation failed:", result.error)
}
```

## Correct

```typescript
app.post("/api/users", (req, res) => {
  const result = UserSchema.safeParse(req.body)
  if (!result.success) {
    const flat = z.flattenError(result.error)
    logger.warn("validation_failed", {
      requestId: req.id,
      schema: "UserSchema",
      path: req.path,
      formErrors: flat.formErrors,
      fieldErrors: flat.fieldErrors,
    })
    return res.status(400).json({ errors: flat.fieldErrors })
  }
})
```

```typescript
// Reusable helper for consistent structured logging
function logValidationError(
  logger: Logger,
  opts: {
    requestId: string
    schema: string
    error: z.ZodError
    path?: string
  }
) {
  const flat = z.flattenError(opts.error)
  logger.warn("validation_failed", {
    requestId: opts.requestId,
    schema: opts.schema,
    path: opts.path,
    formErrors: flat.formErrors,
    fieldErrors: flat.fieldErrors,
    fieldCount: Object.keys(flat.fieldErrors).length,
  })
}
```

## Why

Structured logs with `z.flattenError()` are compact (field name → error messages), queryable (search by schema name or failing field), and correlatable (request ID ties the error to a specific request). This enables dashboards that show which schemas and fields fail most often, alerting on validation spike rates, and debugging specific user requests.
