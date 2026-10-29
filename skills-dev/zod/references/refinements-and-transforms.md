Load when adding validation, conversion, defaults or pipelines.

# Refinements and Transforms Reference

## .refine(fn, opts)

Custom validation that returns a boolean.

```typescript
const EvenNumber = z.number().refine((n) => n % 2 === 0, {
  error: "Must be even",
})

// Async refinement — must use parseAsync/safeParseAsync
const UniqueEmail = z.email().refine(
  async (email) => !(await db.exists(email)),
  { error: "Email taken" }
)
```

## .superRefine((val, ctx) => void)

Advanced validation with multiple issues and path targeting.

```typescript
const Password = z.string().superRefine((val, ctx) => {
  if (val.length < 8) {
    ctx.addIssue({
      code: "custom",
      message: "Must be at least 8 characters",
    })
  }
  if (!/[A-Z]/.test(val)) {
    ctx.addIssue({
      code: "custom",
      message: "Must contain uppercase letter",
    })
  }
  if (!/[0-9]/.test(val)) {
    ctx.addIssue({
      code: "custom",
      message: "Must contain a number",
    })
  }
})
```

### Cross-Field Validation

```typescript
const Form = z
  .object({
    password: z.string().min(8),
    confirm: z.string(),
  })
  .superRefine((data, ctx) => {
    if (data.password !== data.confirm) {
      ctx.addIssue({
        code: "custom",
        path: ["confirm"], // targets the confirm field
        message: "Passwords don't match",
      })
    }
  })
```

## .check(...checks)

Functional-style checks (also available in full Zod, required in Zod Mini).

```typescript
// Full Zod
z.string().check(
  z.minLength(8, "Too short"),
  z.maxLength(100, "Too long"),
)

// Zod Mini
import { z } from "zod/v4/mini"
z.string().check(z.minLength(8), z.maxLength(100))
```

## .transform(fn)

Converts the value to a new type (one-way).

```typescript
const StringToNumber = z.string().transform((s) => parseInt(s, 10))
// Input: string, Output: number

const Trimmed = z.string().transform((s) => s.trim())
// Input: string, Output: string (trimmed)
```

Never throw inside transforms — use `.refine()` first for validation.

## .pipe(schema)

Staged parsing — output of current schema becomes input of next.

```typescript
const PortNumber = z
  .string()
  .pipe(z.coerce.number())
  .pipe(z.int().min(1).max(65535))

// Stage 1: validate string
// Stage 2: coerce to number
// Stage 3: validate integer in range
```

## .preprocess(fn, schema)

Transform unknown input before parsing it with a schema. Use this when preprocessing must precede type validation; `.pipe()` instead consumes an already parsed output.

```typescript
const TrimmedString = z.preprocess(
  (val) => (typeof val === "string" ? val.trim() : val),
  z.string().min(1)
)
```

## .overwrite(fn)

Return a replacement value without changing the inferred type. This does not imply mutation of the original input.

```typescript
const NormalizedEmail = z.email().overwrite((email) => email.toLowerCase())
// Input: string, Output: string (lowercased)
```

## .default(value)

Returns an output-typed default immediately for undefined input, skipping the wrapped validation and transforms. Use `.prefault()` when the fallback must pass through parsing.

```typescript
const Port = z.number().default(3000)
Port.parse(undefined) // 3000
Port.parse(8080)      // 8080
```

Input type becomes optional:
```typescript
type Input = z.input<typeof Port>   // number | undefined
type Output = z.infer<typeof Port>  // number
```

## .prefault(value)

Provides default before validation.

```typescript
const Name = z.string().min(1).prefault("Anonymous")
Name.parse(undefined) // "Anonymous" (validated against min(1) first)
```

## .catch(value)

Fallback on validation failure. Exceptions thrown by user callbacks still propagate; a fallback callback can also throw.

```typescript
const SafeNumber = z.number().catch(0)
SafeNumber.parse("not a number") // 0
SafeNumber.parse(42)             // 42
```

## .apply(fn)

Apply a function that returns a schema.

```typescript
const Clamped = z.number().apply((schema) =>
  schema.min(0).max(100)
)
```

## Async Refinements and Transforms

When any refinement or transform is async, you must use `parseAsync()` or `safeParseAsync()`.

```typescript
const Schema = z.object({
  email: z.email().refine(
    async (email) => !(await db.exists(email)),
    { error: "Taken" }
  ),
  avatar: z.url().transform(
    async (url) => await downloadImage(url)
  ),
})

// REQUIRED: async parse
const result = await Schema.safeParseAsync(data)
```

## Issue Codes for ctx.addIssue()

| Code | Use When |
|------|----------|
| `"custom"` | Custom validation logic |
| `"invalid_type"` | Wrong input type |
| `"too_small"` | Below minimum (length, value, size) |
| `"too_big"` | Above maximum |
| `"invalid_format"` | String format validation |
| `"invalid_value"` | Value not in enum |
| `"unrecognized_keys"` | Unknown keys in strict object |
| `"invalid_union"` | No union branch matched |
