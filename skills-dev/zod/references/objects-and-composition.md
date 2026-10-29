Load when composing objects, containers, unions or recursive schemas.

# Objects and Composition Reference

## Object Variants

### z.object() — Strips Unknown Keys

```typescript
const User = z.object({
  name: z.string(),
  email: z.email(),
})

User.parse({ name: "Alice", email: "a@b.com", extra: true })
// { name: "Alice", email: "a@b.com" } — extra is stripped
```

### z.strictObject() — Rejects Unknown Keys

```typescript
const Config = z.strictObject({
  host: z.string(),
  port: z.number(),
})

Config.parse({ host: "localhost", port: 3000, debug: true })
// ZodError: Unrecognized key "debug"
```

### z.looseObject() — Preserves Unknown Keys

```typescript
const Proxy = z.looseObject({
  id: z.string(),
})

Proxy.parse({ id: "123", extra: true, nested: { a: 1 } })
// { id: "123", extra: true, nested: { a: 1 } }
```

## Object Methods

### .shape

Access the raw shape object for spreading.

```typescript
const User = z.object({ name: z.string(), email: z.email() })
User.shape // { name: ZodString, email: ZodEmail }

// Use for spreading
const Extended = z.object({ ...User.shape, age: z.number() })
```

### .keyof()

Returns a `z.enum()` of the object's keys.

```typescript
const UserKey = User.keyof()
// z.enum(["name", "email"])

UserKey.parse("name")  // "name"
UserKey.parse("age")   // ZodError
```

### .extend()

Add new fields to an object schema.

```typescript
const WithAge = User.extend({ age: z.number() })
```

### .safeExtend()

Extend while requiring overridden fields to remain assignable to the original types. Compatible refinements of an existing key are allowed; object refinements are retained.

```typescript
const WithName = User.safeExtend({ name: z.string().min(1) })
// Compatible override; changing name to z.number() is a TypeScript error
```

### .pick()

Select specific fields.

```typescript
const NameOnly = User.pick({ name: true })
// z.object({ name: z.string() })
```

### .omit()

Remove specific fields.

```typescript
const NameOnly = User.omit({ email: true })
```

### .partial()

Make all fields optional.

```typescript
const PartialUser = User.partial()
// { name?: string; email?: string }

// Partial specific fields
const PartialName = User.partial({ name: true })
// { name?: string; email: string }
```

### .required()

Make all fields required.

```typescript
const RequiredUser = PartialUser.required()
```

### .catchall(schema)

Validate unknown keys against a schema.

```typescript
const Config = z.object({ host: z.string() }).catchall(z.string())
// Known keys validated by their schemas, unknown keys must be strings
```

## Recursive Objects

Use the getter pattern for recursive objects in v4. `z.lazy()` remains available; some recursive definitions need explicit type annotations.

```typescript
const Category = z.object({
  name: z.string(),
  get children() {
    return z.array(Category).optional()
  },
})

type Category = z.infer<typeof Category>
// { name: string; children?: Category[] | undefined }
```

## Arrays

```typescript
z.array(z.string())           // string[]
z.array(z.string()).min(1)    // at least 1 element
z.array(z.string()).max(10)   // at most 10 elements
z.array(z.string()).length(5) // exactly 5 elements
z.array(z.string()).nonempty() // at least 1 at runtime; inferred type stays string[]
```

## Tuples

```typescript
// Fixed-length typed array
z.tuple([z.string(), z.number(), z.boolean()])
// [string, number, boolean]

// With rest element
z.tuple([z.string(), z.number()]).rest(z.boolean())
// [string, number, ...boolean[]]
```

## Records

```typescript
// Dictionary with string keys
z.record(z.string(), z.number())
// Record<string, number>

// Enum keys
z.record(z.enum(["a", "b"]), z.number())
// { a: number; b: number }
```

### z.partialRecord()

Keys may be absent; present values must satisfy the value schema.

```typescript
z.partialRecord(z.enum(["a", "b"]), z.number())
// { a?: number; b?: number }; { a: undefined } fails validation
```

### z.looseRecord()

Preserves extra keys.

```typescript
z.looseRecord(z.string(), z.number())
```

## Maps and Sets

```typescript
z.map(z.string(), z.number())       // Map<string, number>
z.set(z.string())                    // Set<string>
z.set(z.string()).min(1)             // at least 1 element
z.set(z.string()).max(10)            // at most 10 elements
z.set(z.string()).nonempty()         // non-empty set
```

## Unions

### z.union()

Sequential matching — tries each branch in order.

```typescript
z.union([z.string(), z.number()])
// string | number
```

### z.discriminatedUnion()

O(1) dispatch on a shared discriminator field.

```typescript
z.discriminatedUnion("type", [
  z.object({ type: z.literal("a"), value: z.string() }),
  z.object({ type: z.literal("b"), count: z.number() }),
])
```

### z.xor()

Exactly one must match.

```typescript
z.xor([
  z.object({ email: z.email() }),
  z.object({ phone: z.string() }),
])
// Must have email OR phone, not both
```

## Intersection

```typescript
z.intersection(
  z.object({ name: z.string() }),
  z.object({ age: z.number() }),
)
// { name: string; age: number }
```

Prefer `.extend()` or spread over intersection for object merging — intersection has edge cases with overlapping keys.
