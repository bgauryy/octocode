Load when using codecs, brands, JSON Schema or registries.

# Advanced Features Reference

Some APIs below were added after 4.3. Check the installed exports and types before using a later 4.x feature; the skill's version range does not make every example available in every supported version.

## Codecs

Bidirectional transforms — decode (parse) and encode (serialize).

```typescript
const DateCodec = z.codec(z.iso.datetime(), z.date(), {
  decode: (s) => new Date(s),           // string → Date
  encode: (d) => d.toISOString(),       // Date → string
})

const parsed = DateCodec.parse("2024-01-01T00:00:00Z") // Date
const serialized = DateCodec.encode(parsed)              // "2024-01-01T00:00:00.000Z"
```

### Compose a Datetime Codec

```typescript
// ISO datetime string schema: validation only, no Date conversion
z.iso.datetime()

// Use with codec for custom decode/encode
z.codec(z.iso.datetime(), z.date(), {
  decode: (s) => new Date(s),
  encode: (d) => d.toISOString(),
})
```

### When to Use Codecs vs Transforms

| | `.transform()` | `z.codec()` |
|---|---|---|
| Direction | One-way (input → output) | Bidirectional |
| Use when | Only parsing | Round-trip (parse + serialize) |
| Encoding | Not supported | `schema.encode(value)` |

## Branded Types

Nominal typing — prevents mixing structurally identical types.

```typescript
const USD = z.number().brand<"USD">()
const EUR = z.number().brand<"EUR">()

type USD = z.infer<typeof USD> // number & z.$brand<"USD">
type EUR = z.infer<typeof EUR> // number & z.$brand<"EUR">

// TypeScript prevents mixing
function pay(amount: USD) { /* ... */ }
const euros = EUR.parse(100)
pay(euros) // TypeScript error!
```

### Common Use Cases

```typescript
// Prevent ID mixing
const UserId = z.string().brand<"UserId">()
const PostId = z.string().brand<"PostId">()

// Type-safe units
const Meters = z.number().brand<"Meters">()
const Feet = z.number().brand<"Feet">()

// Validated strings
const Email = z.email().brand<"Email">()
const Slug = z.string().regex(/^[a-z0-9-]+$/).brand<"Slug">()
```

## .readonly()

Output type becomes `Readonly<T>`.

```typescript
const Config = z.object({
  host: z.string(),
  port: z.number(),
}).readonly()

type Config = z.infer<typeof Config>
// Readonly<{ host: string; port: number }>
```

## Metadata and Registries

### .meta()

Attach arbitrary metadata to schemas.

```typescript
const UserSchema = z.object({
  name: z.string().meta({ label: "Full Name", placeholder: "Enter name" }),
  email: z.email().meta({ label: "Email Address" }),
})
```

### Registries

```typescript
// Global registry
z.globalRegistry.add(UserSchema, {
  id: "User",
  description: "User account schema",
})

// Custom typed registry
const uiRegistry = z.registry<{ label: string; widget: string }>()
uiRegistry.add(UserSchema.shape.name, {
  label: "Name",
  widget: "text-input",
})
```

## JSON Schema

### z.toJSONSchema(schema)

Convert Zod schema to JSON Schema.

```typescript
const jsonSchema = z.toJSONSchema(UserSchema)
// {
//   type: "object",
//   properties: {
//     name: { type: "string" },
//     email: { type: "string", format: "email" },
//   },
//   required: ["name", "email"]
// }
```

### z.fromJSONSchema(jsonSchema)

Convert JSON Schema to Zod schema.

```typescript
const zodSchema = z.fromJSONSchema({
  type: "object",
  properties: {
    name: { type: "string" },
    age: { type: "integer", minimum: 0 },
  },
  required: ["name"],
})
```

## z.function()

Validate function arguments and return type.

```typescript
const MyFunc = z.function({
  input: [z.string(), z.number()],
  output: z.boolean(),
})
const checked = MyFunc.implement((text, count) => text.length > count)

type MyFunc = typeof checked
// (arg0: string, arg1: number) => boolean
```

## z.instanceof()

Check if value is an instance of a class.

```typescript
const ErrorSchema = z.instanceof(Error)
ErrorSchema.parse(new Error("test")) // passes
ErrorSchema.parse("not an error")    // fails
```

## Template Literals

```typescript
const UserId = z.templateLiteral([z.literal("user_"), z.string()])
// Matches: "user_abc", "user_123"
// Rejects: "abc", "admin_123"

const Route = z.templateLiteral([
  z.literal("/api/"),
  z.enum(["users", "posts"]),
  z.literal("/"),
  z.string(),
])
// Matches: "/api/users/123", "/api/posts/abc"
```

## Standard Schema

Zod schemas implement the Standard Schema interface, making them compatible with any library that supports it.

```typescript
import type { StandardSchemaV1 } from "@standard-schema/spec"

function validate(schema: StandardSchemaV1, data: unknown) {
  return schema["~standard"].validate(data)
}

// Works with Zod schemas
validate(UserSchema, data)
```

## Top-level codec functions

`z.codec()` gives a schema `.decode()` and `.encode()`, but the standalone
functions are often what you want when the codec is applied somewhere generic:

```ts
const DateField = z.codec(z.iso.datetime(), z.date(), {
  decode: (s) => new Date(s),
  encode: (d) => d.toISOString(),
})

z.decode(DateField, "2024-01-01T00:00:00Z")  // Date
z.encode(DateField, new Date())              // string

// Swap the direction without rewriting the codec.
const Inverted = z.invertCodec(DateField)
```

`z.invertCodec()` matters when the same transform is needed in both directions
at different boundaries — reading from an API and writing back to it — and
saves defining the mirror image by hand, which is where the two drift apart.

## String formats added after 4.0

```ts
z.guid()            // any GUID/UUID shape, looser than z.uuid()
z.hash("sha256")    // also "sha1", "sha384", "sha512", "md5"
```

`z.hash()` validates the hex length for the named algorithm, so a truncated
digest fails at the boundary rather than downstream.

## Strict optional properties

```ts
// `.optional()` accepts an explicit `undefined`; `.exactOptional()` does not —
// the key must be absent rather than present-and-undefined.
z.object({ nickname: z.string().exactOptional() })

// `.nonoptional()` removes optionality again, which is useful after a
// `.partial()` when one field must come back.
```

The distinction matters against JSON, where `{"a": undefined}` cannot be
expressed, and against `exactOptionalPropertyTypes` in TypeScript.
