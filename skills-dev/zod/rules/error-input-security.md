---
title: reportInput Leaks Sensitive Data
impact: HIGH
description: Keep reportInput disabled for real user or secret data in every environment; inspect only fabricated non-sensitive fixtures.
tags: security, reportInput, logging, production
---

# reportInput Leaks Sensitive Data

## Problem

Zod v4's `reportInput` option includes the raw input value in error issues. If enabled in production, sensitive data (passwords, tokens, PII) ends up in error logs, monitoring systems, and API responses.

## Incorrect

```typescript
// BUG: leaks passwords, tokens, PII into error messages
app.post("/register", (req, res) => {
  const result = UserSchema.safeParse(req.body, { reportInput: true })
  if (!result.success) {
    // error.issues[0].input could contain: "myP@ssw0rd123"
    logger.error(result.error.issues)
    res.status(400).json({ errors: result.error.issues })
  }
})
```

## Correct

```typescript
// GOOD: omit reportInput for real data, including development
app.post("/register", (req, res) => {
  const result = UserSchema.safeParse(req.body)
  if (!result.success) {
    // No raw input field; custom messages still need review for secrets
    const flat = z.flattenError(result.error)
    res.status(400).json({ errors: flat.fieldErrors })
  }
})
```

## Why

Error issues are logged, sent to monitoring and returned in API responses. Development can handle sensitive data too. Enable input reporting only for explicit inspection of fabricated, non-sensitive fixtures; review custom error messages because disabling reportInput does not sanitize text a callback constructs.
