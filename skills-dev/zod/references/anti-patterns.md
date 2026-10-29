Load when reviewing a suspected Zod API or schema-design mistake.

# Zod Anti-Patterns

## Table of Contents

- Using parse() with try/catch instead of safeParse()
- Sync parse with async refinements
- Manual type definitions alongside schemas
- Deprecated string format chaining (v4)
- Using z.nativeEnum() (v4)
- Using required_error/invalid_type_error (v4)
- Assuming error formatters have the same shape
- Throwing inside refinements or transforms
- z.coerce.boolean() for string booleans
- Assuming z.object() preserves unknown keys
- z.union() for tagged objects
- reportInput in production
- z.lazy() for recursive schemas (v4)
- Duplicating field definitions
- Hand-rolling what Zod 4.1–4.4 now does natively

## Using parse() with try/catch instead of safeParse()

```typescript
// BAD: verbose, catches unrelated errors
try {
  const user = UserSchema.parse(data)
  return { success: true, data: user }
} catch (e) {
  if (e instanceof z.ZodError) {
    return { success: false, errors: e.issues }
  }
  throw e
}

// GOOD: discriminated result
const result = UserSchema.safeParse(data)
if (result.success) {
  return { success: true, data: result.data }
} else {
  return { success: false, errors: result.error.issues }
}
```

## Sync parse with async refinements

```typescript
// BAD: throws because refinement is async
const Schema = z.email().refine(async (e) => !(await db.exists(e)))
Schema.safeParse(data) // throws

// GOOD: use safeParseAsync
await Schema.safeParseAsync(data)
```

## Manual type definitions alongside schemas

```typescript
// BAD: types drift from schema
interface User {
  name: string
  email: string
}

const UserSchema = z.object({
  name: z.string(),
  email: z.email(),
  age: z.number(), // added to schema, forgot interface
})

// GOOD: infer from schema
type User = z.infer<typeof UserSchema>
```

## Deprecated string format chaining (v4)

```typescript
// BAD: deprecated in v4
z.string().email()
z.string().url()
z.string().uuid()

// GOOD: top-level format functions
z.email()
z.url()
z.uuid()
```

## Using z.nativeEnum() (v4)

```typescript
enum Status { Active = "active", Inactive = "inactive" }

// Deprecated in v4; still exported
z.nativeEnum(Status)

// GOOD: unified z.enum()
z.enum(Status)
```

## Using required_error/invalid_type_error (v4)

```typescript
// BAD: removed in v4
z.string({ required_error: "Required", invalid_type_error: "Not a string" })
// Still supported but deprecated:
z.number().min(5, { message: "Too small" })

// GOOD: unified error parameter
z.string({ error: "Required" })
z.number().min(5, { error: "Too small" })
z.number().min(5, "Too small") // shorthand
```

## Assuming error formatters have the same shape

```typescript
// Different shape: _errors keys, not treeifyError properties/errors
z.formatError(error)

// GOOD: use the right formatter
z.flattenError(error)   // for flat forms
z.treeifyError(error)   // for nested structures
z.prettifyError(error)  // for logging
```

## Throwing inside refinements or transforms

```typescript
// BAD: bypasses Zod error handling
z.number().refine((n) => {
  if (n <= 0) throw new Error("Must be positive")
  return true
})

z.string().transform((val) => {
  const n = parseInt(val)
  if (isNaN(n)) throw new Error("Not a number")
  return n
})

// GOOD: return boolean from refine
z.number().refine((n) => n > 0, { error: "Must be positive" })

// GOOD: validate the whole decimal integer, not a parseInt prefix
z.string().regex(/^[+-]?\d+$/)
  .transform(Number)
  .pipe(z.int())

// BEST: pipe for staged parsing
z.string().pipe(z.coerce.number()).pipe(z.number().positive())
```

## z.coerce.boolean() for string booleans

```typescript
// BAD: Boolean("false") === true
z.coerce.boolean().parse("false") // true
z.coerce.boolean().parse("0")     // true

// GOOD: z.stringbool() handles string booleans correctly
z.stringbool().parse("false") // false
z.stringbool().parse("0")     // false
```

## Assuming z.object() preserves unknown keys

```typescript
// BAD: unknown keys are silently stripped
const data = { name: "Alice", role: "admin", debug: true }
z.object({ name: z.string() }).parse(data)
// { name: "Alice" } — role and debug are gone

// GOOD: choose explicitly
z.strictObject({ name: z.string() }) // rejects unknown
z.looseObject({ name: z.string() })  // preserves unknown
```

## z.union() for tagged objects

```typescript
// BAD: sequential matching, poor error messages
z.union([
  z.object({ type: z.literal("a"), value: z.string() }),
  z.object({ type: z.literal("b"), count: z.number() }),
])

// GOOD: O(1) dispatch on discriminator
z.discriminatedUnion("type", [
  z.object({ type: z.literal("a"), value: z.string() }),
  z.object({ type: z.literal("b"), count: z.number() }),
])
```

## reportInput in production

```typescript
// BAD: leaks sensitive data into error logs
app.post("/login", (req, res) => {
  const result = schema.safeParse(req.body, { reportInput: true })
  if (!result.success) {
    logger.error(result.error.issues) // may contain passwords
  }
})

// GOOD: omit raw input in every environment handling real data
schema.safeParse(req.body)
```

## z.lazy() for recursive schemas (v4)

```typescript
// Supported: lazy recursion may need an explicit TypeScript annotation
const Tree = z.object({
  value: z.string(),
  children: z.lazy(() => z.array(Tree)),
})

// GOOD: getter pattern
const Tree = z.object({
  value: z.string(),
  get children() {
    return z.array(Tree).optional()
  },
})
```

## Duplicating field definitions

```typescript
// BAD: fields duplicated — will drift
const CreateUser = z.object({ name: z.string(), email: z.email() })
const UpdateUser = z.object({ name: z.string().optional(), email: z.email().optional() })

// GOOD: derive from base
const User = z.object({ name: z.string(), email: z.email() })
const CreateUser = User
const UpdateUser = User.partial()
```

## Hand-rolling what Zod 4.1–4.4 now does natively

These are optional replacements when the installed version supports them and the consumer needs the behavior. Working code does not require migration merely because a newer API exists.

### One-way transforms where the value has to make a round trip

```ts
// WORKS, but you own the inverse
const DateIn = z.iso.datetime().transform((s) => new Date(s))
const toWire = (d: Date) => d.toISOString()   // and this drifts from the above

// BETTER: one definition, both directions
const DateField = z.codec(z.iso.datetime(), z.date(), {
  decode: (s) => new Date(s),
  encode: (d) => d.toISOString(),
})
z.decode(DateField, wire)     // Date
z.encode(DateField, value)    // string
z.invertCodec(DateField)      // when the boundary runs the other way
```

Co-locating encode and decode makes drift easier to review. It does not prove they are inverses: test round trips and any intended normalization.

### Optional keys that must not be present-and-undefined

```ts
// WORKS, but accepts { nickname: undefined }
z.object({ nickname: z.string().optional() })

// BETTER when the key must be absent
z.object({ nickname: z.string().exactOptional() })
```

Matters against JSON, which cannot express an explicit `undefined`, and against
TypeScript's `exactOptionalPropertyTypes`.

### Mutually exclusive unions checked by hand

```ts
// Accepts an object satisfying either or both arms
z.union([EmailLogin, PhoneLogin])

// BETTER
z.xor([EmailLogin, PhoneLogin])
```

### JSON Schema written or parsed by hand

```ts
// BETTER, both directions
z.toJSONSchema(User)      // publish a contract
z.fromJSONSchema(spec)    // consume someone else's
```

Hand-written JSON Schema alongside a Zod schema is two sources of truth that
agree only until someone edits one of them.

### Format checks written as regex

```ts
// WORKS
z.string().regex(/^[0-9a-f]{64}$/i)

// BETTER — named, and correct per algorithm
z.hash("sha256")
z.guid()
```
