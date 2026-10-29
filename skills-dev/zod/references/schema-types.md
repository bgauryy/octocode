Load when choosing primitive, format, enum or date syntax.

# Schema Types Reference

## Primitives

```typescript
z.string()     // string
z.number()     // number (int or float)
z.boolean()    // boolean
z.bigint()     // bigint
z.date()       // Date instance
z.symbol()     // symbol
z.undefined()  // undefined
z.null()       // null
z.void()       // void (undefined)
z.any()        // any — bypasses type checking
z.unknown()    // unknown — safer than any
z.never()      // never — always fails
```

## Strings

### Constraints

```typescript
z.string().min(5)           // minimum length
z.string().max(100)         // maximum length
z.string().length(10)       // exact length
z.string().regex(/^[a-z]+$/) // regex pattern
z.string().trim()           // trims whitespace (transform)
z.string().toLowerCase()    // lowercases (transform)
z.string().toUpperCase()    // uppercases (transform)
z.string().startsWith("https://")
z.string().endsWith(".com")
z.string().includes("@")
```

### Top-Level String Formats (v4)

```typescript
z.email()       // email address
z.url()         // URL
z.uuid()        // UUID; use z.uuid({ version: "v4" }) to require UUID v4
z.cuid()        // CUID
z.cuid2()       // CUID2
z.ulid()        // ULID
z.emoji()       // emoji character
z.nanoid()      // Nano ID
z.ipv4()        // IPv4 address
z.ipv6()        // IPv6 address
z.cidrv4()      // CIDR v4 notation
z.cidrv6()      // CIDR v6 notation
z.jwt()         // JSON Web Token
z.base64()      // Base64 string
z.base64url()   // Base64url string
```

### ISO Date/Time Strings

```typescript
z.iso.date()      // "2024-01-15"
z.iso.time()      // "13:45:30"
z.iso.datetime()  // "2024-01-15T13:45:30Z"
z.iso.duration()  // "P3Y6M4DT12H30M5S"
```

### Template Literals

```typescript
// Validates strings matching a template pattern
const UserID = z.templateLiteral([z.literal("user_"), z.string()])
// Matches: "user_abc123", "user_xyz"
// Rejects: "abc123", "admin_xyz"
```

## Numbers

```typescript
z.number()                // finite number; rejects NaN and Infinity
z.int()                   // integer only
z.float32()               // finite number within float32 bounds
z.float64()               // finite number within float64 bounds

// Constraints
z.number().min(0)         // >= 0
z.number().max(100)       // <= 100
z.number().positive()     // > 0
z.number().negative()     // < 0
z.number().nonnegative()  // >= 0
z.number().nonpositive()  // <= 0
z.number().multipleOf(5)  // divisible by 5
z.number().finite()       // not Infinity
z.number().safe()         // deprecated alias for integer validation; prefer z.int()
```

## BigInt

```typescript
z.bigint()
z.bigint().min(0n)
z.bigint().max(100n)
z.bigint().positive()
z.bigint().negative()
z.bigint().nonnegative()
z.bigint().nonpositive()
z.bigint().multipleOf(5n)
```

## Boolean

```typescript
z.boolean()        // true or false
z.literal(true)    // only true
z.literal(false)   // only false
```

## Date

```typescript
z.date()                           // Date instance
z.date().min(new Date("2020-01-01")) // after date
z.date().max(new Date("2030-01-01")) // before date
```

## Enums

```typescript
// String literal array
z.enum(["active", "inactive", "pending"])

// TypeScript enum (v4 — nativeEnum is deprecated; z.enum handles both)
enum Status {
  Active = "active",
  Inactive = "inactive",
}
z.enum(Status)

// Numeric enum
enum Priority {
  Low = 0,
  Medium = 1,
  High = 2,
}
z.enum(Priority)
```

## Stringbool

Converts string boolean representations to actual booleans.

```typescript
z.stringbool()
// Accepts: "true"/"false", "1"/"0", "yes"/"no", "on"/"off"
// Returns: boolean

z.stringbool().parse("true")   // true
z.stringbool().parse("false")  // false
z.stringbool().parse("1")      // true
z.stringbool().parse("0")      // false
z.stringbool().parse("yes")    // true
z.stringbool().parse("no")     // false
```

## Literals

```typescript
z.literal("hello")   // exactly "hello"
z.literal(42)        // exactly 42
z.literal(true)      // exactly true
z.literal(null)      // exactly null
z.literal(undefined) // exactly undefined
z.literal(100n)      // exactly 100n (bigint)
```

## Files

```typescript
z.file()                         // File instance
z.file().min(1024)               // minimum size in bytes
z.file().max(5 * 1024 * 1024)    // maximum size (5MB)
z.file().mime("image/png")       // MIME type
z.file().mime(["image/png", "image/jpeg"]) // accepted MIME types
```

## JSON

```typescript
// Validates an already decoded JSON-compatible value
z.json()

z.json().parse({ name: "Alice" }) // { name: "Alice" }
z.json().parse("invalid json")   // "invalid json": strings are JSON values
// Decode JSON text with JSON.parse (handle syntax errors), then validate
// with the application's schema. z.json() does not decode strings.
```

## Custom Types

```typescript
// Custom type with validation function
const NonEmptyString = z.custom<string>(
  (val) => typeof val === "string" && val.length > 0,
  { error: "Must be a non-empty string" }
)
```

## Optional, Nullable, Nullish

```typescript
z.string().optional()   // string | undefined
z.string().nullable()   // string | null
z.string().nullish()    // string | null | undefined
```

## Coercion

```typescript
z.coerce.string()   // String(input)
z.coerce.number()   // Number(input)
z.coerce.boolean()  // Boolean(input) — careful: Boolean("false") === true
z.coerce.bigint()   // BigInt(input)
z.coerce.date()     // new Date(input)
```
