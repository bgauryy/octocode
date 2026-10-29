---
title: Coercion Pitfalls — Boolean Strings
impact: CRITICAL
description: z.coerce.boolean() uses JavaScript Boolean() which makes "false" → true. Use z.stringbool().
tags: coercion, boolean, forms, env-vars
---

# Coercion Pitfalls — Boolean Strings

## Problem

`z.coerce.boolean()` wraps the input in JavaScript's `Boolean()` constructor. `Boolean("false")` is `true` because any non-empty string is truthy. This silently corrupts form data and environment variables.

## Incorrect

```typescript
// BUG: Boolean("false") === true, Boolean("0") === true
const Settings = z.object({
  debug: z.coerce.boolean(),
  verbose: z.coerce.boolean(),
})

Settings.parse({ debug: "false", verbose: "0" })
// { debug: true, verbose: true } — both are wrong!
```

## Correct

```typescript
// GOOD: z.stringbool() handles string boolean representations correctly
const Settings = z.object({
  debug: z.stringbool(),
  verbose: z.stringbool(),
})

Settings.parse({ debug: "false", verbose: "0" })
// { debug: false, verbose: false }

// Defaults also accept "y"/"n" and "enabled"/"disabled" (case-insensitive).
// Set truthy/falsy lists when the consumer accepts a narrower vocabulary.
```

## Why

`z.stringbool()` is purpose-built for string-to-boolean conversion from forms, env vars, and query parameters. It understands common string representations of boolean values. `z.coerce.boolean()` should only be used when you actually want JavaScript truthiness semantics.
