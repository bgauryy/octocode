Load when customizing or formatting validation failures for a consumer.

# Error Handling Reference

## ZodError

```typescript
const result = schema.safeParse(data)
if (!result.success) {
  result.error           // ZodError instance
  result.error.issues    // array of ZodIssue
  result.error.message   // JSON string of issues
  result.error.toString() // same as .message
}
```

## Issue Structure

```typescript
interface ZodIssue {
  code: string        // issue type code
  message: string     // human-readable message
  path: PropertyKey[]  // path to the field
  input?: unknown     // raw input (only if reportInput: true)
  // ... additional fields depending on code
}
```

## Issue Codes

| Code | When | Extra Fields |
|------|------|-------------|
| `invalid_type` | Wrong type | `expected` |
| `too_small` | Below minimum | `minimum`, `inclusive`, `origin` |
| `too_big` | Above maximum | `maximum`, `inclusive`, `origin` |
| `invalid_format` | String format failure | `format` |
| `custom` | Custom refinement | — |
| `invalid_value` | Not an allowed enum/literal value | `values` |
| `unrecognized_keys` | Unknown keys (strict) | `keys` |
| `invalid_union` | No branch matched | `errors` |
| `not_multiple_of` | Number is not a multiple | `divisor`, `origin` |
| `invalid_key` | Invalid record/map key | `issues`, `origin` |
| `invalid_element` | Invalid map/set element | `issues`, `origin` |

## Error Formatting Functions

### z.flattenError(error)

Flat structure for simple forms.

```typescript
const flat = z.flattenError(result.error)
// {
//   formErrors: ["Root-level error"],
//   fieldErrors: {
//     email: ["Invalid email"],
//     age: ["Must be positive", "Must be integer"],
//   }
// }
```

### z.treeifyError(error)

Nested tree matching schema shape. For deeply nested forms.

```typescript
const tree = z.treeifyError(result.error)
// {
//   errors: [],
//   properties: {
//     address: {
//       errors: [],
//       properties: {
//         zip: { errors: ["Required"] }
//       }
//     }
//   }
// }
```

### z.prettifyError(error)

Human-readable string for logging/debugging.

```typescript
const pretty = z.prettifyError(result.error)
// "✖ Invalid email at «email»
//  ✖ Required at «address.zip»"
```

### z.formatError()

Still exported; returns the older `_errors` tree shape. Prefer `flattenError` for flat forms or `treeifyError` for nested consumers. The instance method `error.format()` is deprecated; do not confuse it with the exported function.

## Error Customization

### Schema Level

```typescript
// String shorthand
z.string({ error: "Must be a string" })
z.number().min(18, { error: "Must be 18+" })
z.number().min(18, "Must be 18+") // shorthand

// Function form — dynamic messages
z.string({
  error: (issue) => {
    if (issue.input === undefined) return "Required"
    return "Must be a string"
  },
})
```

### Parse Level

```typescript
schema.safeParse(data, {
  error: (issue) => {
    // Override error for this parse call only
    return `Validation failed: ${issue.code}`
  },
})
```

### Global Level

```typescript
z.config({
  customError: (issue) => {
    // Global default error messages
    if (issue.code === "invalid_type") {
      return `Expected ${issue.expected}`
    }
  },
})
```

## Error Precedence

1. **Schema-level** `error` — highest priority
2. **Parse-level** `error` — middle priority
3. **Global** `z.config({ customError })` — lowest priority

A schema-level callback can return `undefined` to defer to lower-priority customization. Locale messages provide the fallback after global custom errors.

## reportInput Option

Includes raw input values in error issues. Keep it off for real user or secret data in every environment. An environment name is not a privacy boundary.

```typescript
// Explicit inspection of a fabricated, non-sensitive fixture only
const result = schema.safeParse(fixture, { reportInput: true })
if (!result.success) {
  result.error.issues[0].input // contains the raw value
}
```

Leaks passwords, tokens, PII into logs and error monitoring.

## i18n / Localization

Use the error function form for localized messages.

```typescript
const t = getTranslation(locale)

const NameSchema = z.string({
  error: (issue) => {
    if (issue.input === undefined) return t("field.required")
    return t("field.invalid_string")
  },
}).min(1, { error: t("field.too_short") })
```

Or use global config for app-wide localization:

```typescript
z.config({
  customError: (issue) => {
    const t = getTranslation(currentLocale)
    switch (issue.code) {
      case "invalid_type": return t("validation.invalid_type")
      case "too_small": return t("validation.too_small", { min: issue.minimum })
      default: return t("validation.invalid")
    }
  },
})
```
