Load when writing a schema assertion, fixture or test helper.

# Zod Testing API Reference

## Schema Correctness Testing

### safeParse() for Tests

Use `safeParse()` to inspect success, transformed data and validation issues. Use a throwing assertion when the contract under test is `parse()`; both forms can give clear failures.

```typescript
// GOOD: assertion-based
const result = schema.safeParse(data)
expect(result.success).toBe(true)
// or
expect(result.success).toBe(false)

// Also valid: assert the throwing API's contract
expect(() => schema.parse(invalidData)).toThrow(z.ZodError)
```

### Testing Valid Data

```typescript
describe("UserSchema", () => {
  const validUsers = [
    { name: "Alice", email: "alice@example.com", age: 30 },
    { name: "B", email: "b@c.co", age: 0 },
    { name: "Zed", email: "zed+tag@domain.org", age: 150 },
  ]

  it.each(validUsers)("accepts valid user: $name", (user) => {
    const result = UserSchema.safeParse(user)
    expect(result.success).toBe(true)
    if (result.success) {
      expect(result.data).toEqual(user)
    }
  })
})
```

### Testing Invalid Data

```typescript
const validUser = { name: "Alice", email: "alice@example.com", age: 30 }
const invalidInputs = [
  { input: {}, desc: "empty object" },
  { input: { ...validUser, name: "" }, desc: "empty name" },
  { input: { ...validUser, email: "bad" }, desc: "invalid email" },
  { input: { ...validUser, age: -1 }, desc: "negative age" },
  { input: { ...validUser, age: 151 }, desc: "age over max" },
]

it.each(invalidInputs)("rejects $desc", ({ input }) => {
  expect(UserSchema.safeParse(input).success).toBe(false)
})
```

### Boundary Value Testing

```typescript
const NumberSchema = z.number().min(0).max(100)

const boundaries = [
  { value: 0, expected: true, desc: "minimum" },
  { value: 100, expected: true, desc: "maximum" },
  { value: -1, expected: false, desc: "below minimum" },
  { value: 101, expected: false, desc: "above maximum" },
  { value: -0.001, expected: false, desc: "fractionally below min" },
  { value: 100.001, expected: false, desc: "fractionally above max" },
]

it.each(boundaries)("$desc ($value) → $expected", ({ value, expected }) => {
  expect(NumberSchema.safeParse(value).success).toBe(expected)
})
```

### Optional and Nullable Testing

```typescript
const Schema = z.object({
  required: z.string(),
  optional: z.string().optional(),
  nullable: z.string().nullable(),
  both: z.string().optional().nullable(),
})

const valid = { required: "a", nullable: null }
it("accepts optional/nullable combinations", () => {
  expect(Schema.safeParse(valid).success).toBe(true)
  expect(Schema.safeParse({ ...valid, optional: undefined }).success).toBe(true)
  expect(Schema.safeParse({ ...valid, both: null }).success).toBe(true)
  expect(Schema.safeParse({ ...valid, both: undefined }).success).toBe(true)
})

it("rejects null for non-nullable optional", () => {
  expect(Schema.safeParse({ ...valid, optional: null }).success).toBe(false)
})

it("requires the nullable field to be present", () => {
  expect(Schema.safeParse({ required: "a" }).success).toBe(false)
})

it("rejects undefined for required", () => {
  expect(Schema.safeParse({ ...valid, required: undefined }).success).toBe(false)
})
```

## Error Assertion Patterns

### Using z.flattenError()

```typescript
it("reports field-level errors", () => {
  const result = UserSchema.safeParse({ name: "", email: "bad", age: -1 })
  expect(result.success).toBe(false)
  if (!result.success) {
    const flat = z.flattenError(result.error)
    expect(flat.fieldErrors).toHaveProperty("email")
    expect(flat.fieldErrors).toHaveProperty("age")
  }
})
```

### Asserting Error Messages

```typescript
it("shows custom error message", () => {
  const Schema = z.string({ error: "Name is required" })
  const result = Schema.safeParse(undefined)
  expect(result.success).toBe(false)
  if (!result.success) {
    expect(result.error.issues[0].message).toBe("Name is required")
  }
})
```

### Asserting Error Codes

```typescript
it("produces invalid_type for wrong type", () => {
  const result = z.number().safeParse("string")
  expect(result.success).toBe(false)
  if (!result.success) {
    expect(result.error.issues[0].code).toBe("invalid_type")
  }
})

it("produces too_small for under minimum", () => {
  const result = z.number().min(5).safeParse(3)
  expect(result.success).toBe(false)
  if (!result.success) {
    expect(result.error.issues[0].code).toBe("too_small")
  }
})
```

### Asserting Error Paths

```typescript
it("targets the correct field", () => {
  const result = FormSchema.safeParse({
    password: "12345678",
    confirm: "different",
  })
  expect(result.success).toBe(false)
  if (!result.success) {
    const confirmError = result.error.issues.find(
      (i) => i.path.join(".") === "confirm"
    )
    expect(confirmError).toBeDefined()
    expect(confirmError!.message).toBe("Passwords don't match")
  }
})
```

### Counting Errors

```typescript
it("reports all validation errors at once", () => {
  const result = UserSchema.safeParse({})
  expect(result.success).toBe(false)
  if (!result.success) {
    // Assert expected independent issues; earlier type failures may skip refinements
    expect(result.error.issues.length).toBeGreaterThanOrEqual(3)
  }
})
```

## Mock Data Generation

Check the generator’s peer-version support before installing it. These are optional tools, not requirements. Generated fixtures supplement independent contract cases and may not satisfy arbitrary custom refinements.

### Using zod-schema-faker

```bash
npm install -D zod-schema-faker
```

```typescript
import { install, fake, seed } from "zod-schema-faker"
import { z } from "zod"

// Call once in test setup (beforeAll or setupFiles)
install(z)

const UserSchema = z.object({
  name: z.string().min(1).max(50),
  email: z.email(),
  age: z.number().int().min(0).max(150),
  role: z.enum(["admin", "user", "guest"]),
})

describe("with generated data", () => {
  beforeEach(() => seed(42)) // deterministic

  it("schema accepts its own generated data", () => {
    const user = fake(UserSchema)
    expect(UserSchema.safeParse(user).success).toBe(true)
  })

  it("generates array of valid items", () => {
    const users = Array.from({ length: 10 }, () => fake(UserSchema))
    users.forEach((u) => {
      expect(UserSchema.safeParse(u).success).toBe(true)
    })
  })
})
```

### Using @anatine/zod-mock

```bash
npm install -D @anatine/zod-mock @faker-js/faker
```

```typescript
import { generateMock } from "@anatine/zod-mock"

it("generates valid mock", () => {
  const mock = generateMock(UserSchema)
  expect(UserSchema.safeParse(mock).success).toBe(true)
})

// With seed for deterministic output
it("generates consistent mock", () => {
  const mock = generateMock(UserSchema, { seed: 123 })
  expect(mock.name).toBeDefined()
})
```

## Snapshot Testing

### JSON Schema Snapshots

```typescript
import { z } from "zod"

it("schema shape matches snapshot", () => {
  const jsonSchema = z.toJSONSchema(UserSchema)
  expect(jsonSchema).toMatchSnapshot()
})
```

Update snapshots when schema changes are intentional:
```bash
vitest --update  # or jest --updateSnapshot
```

### Formatted Error Snapshots

```typescript
it("error output matches snapshot", () => {
  const result = UserSchema.safeParse({ name: 123, email: "bad" })
  expect(result.success).toBe(false)
  if (!result.success) {
    const flat = z.flattenError(result.error)
    expect(flat).toMatchSnapshot()
  }
})
```

## Integration Testing

### Express/Fastify API Handler

```typescript
import request from "supertest"

it("validates request body", async () => {
  const res = await request(app)
    .post("/api/users")
    .send({ name: "", email: "invalid" })
    .expect(400)

  expect(res.body.errors.fieldErrors).toHaveProperty("email")
})

it("accepts valid request", async () => {
  const res = await request(app)
    .post("/api/users")
    .send({ name: "Alice", email: "alice@example.com", age: 30 })
    .expect(201)

  expect(res.body.id).toBeDefined()
})
```

### Form Validation

```typescript
it("produces form-compatible errors", () => {
  const result = FormSchema.safeParse(formData)
  expect(result.success).toBe(false)
  if (!result.success) {
    const errors = z.flattenError(result.error)
    // Structure matches what form libraries expect
    expect(errors).toEqual({
      formErrors: expect.any(Array),
      fieldErrors: expect.objectContaining({
        email: expect.any(Array),
      }),
    })
  }
})
```

### Database Layer

```typescript
it("validates before insert", async () => {
  const invalidUser = { name: "", email: "bad" }
  db.insert.mockClear()
  // Exercise the application entry point that owns parsing and persistence.
  // This example assumes its contract returns a validation result.
  const result = await createUser(invalidUser)
  expect(result.success).toBe(false)
  expect(db.insert).not.toHaveBeenCalled()
})
```

## Property-Based Testing

### With fast-check

The first example checks generator/schema consistency, not the application contract. Also generate inputs independently of the schema, such as negative integers for a documented nonnegative age, and assert rejection while all other fields remain valid.

```bash
npm install -D fast-check
```

```typescript
import fc from "fast-check"
import { fake, seed } from "zod-schema-faker"

it("schema always accepts its own generated data", () => {
  fc.assert(
    fc.property(fc.integer({ min: 0, max: 10000 }), (s) => {
      seed(s)
      const data = fake(UserSchema)
      return UserSchema.safeParse(data).success
    }),
    { numRuns: 200 }
  )
})
```

### Codec Round-Trip Testing

Output-shape validation alone is not a round trip. Check the inverse property and separately test invalid input and intended normalization:

```typescript
const DateField = z.codec(z.iso.datetime(), z.date(), {
  decode: (s) => new Date(s),
  encode: (d) => d.toISOString(),
})
it("round-trips the timestamp", () => {
  fc.assert(fc.property(fc.integer({ min: 0, max: 2_000_000_000_000 }), (ms) => {
    const date = new Date(ms)
    return z.decode(DateField, z.encode(DateField, date)).getTime() === ms
  }), { seed: 42 })
})
```

## Async Schema Testing

```typescript
it("validates async refinements", async () => {
  const UniqueEmail = z.email().refine(
    async (email) => !(await db.exists(email)),
    { error: "Already registered" }
  )

  // Configure the db.exists fake to resolve true for this address.
  // Must use safeParseAsync
  const result = await UniqueEmail.safeParseAsync("taken@example.com")
  expect(result.success).toBe(false)
})
```

## Test Helpers

### reusable assertion helper

```typescript
function expectValid<T>(schema: z.ZodType<T>, data: unknown) {
  const result = schema.safeParse(data)
  if (!result.success) {
    throw new Error(
      `Expected valid but got errors:\n${z.prettifyError(result.error)}`
    )
  }
  return result.data
}

function expectInvalid(schema: z.ZodType, data: unknown) {
  const result = schema.safeParse(data)
  expect(result.success).toBe(false)
  if (!result.success) return result.error
  throw new Error("Expected invalid but data was accepted")
}

// Usage
it("accepts valid user", () => {
  const user = expectValid(UserSchema, validData)
  expect(user.name).toBe("Alice")
})

it("rejects invalid email", () => {
  const error = expectInvalid(UserSchema, { ...validData, email: "bad" })
  expect(error.issues[0].path).toEqual(["email"])
})
```

## Structural Testing

### Schema Dependency Direction

When the repository separates boundary schemas from domain code, inspect resolved imports with an AST/type-aware architecture rule. Distinguish type-only imports from runtime imports, handle aliases and re-exports, and test both permitted and forbidden edges. Grepping for `from.*schemas` misses multiline imports and aliases and is not proof of a boundary.

### Circular Import Detection

Use madge to detect circular dependencies between schema files:

```typescript
import madge from "madge"

describe("schema dependencies", () => {
  it("has no circular dependencies", async () => {
    const result = await madge("src/", {
      fileExtensions: ["ts"],
      tsConfigPath: "tsconfig.json",
    })
    const circular = result.circular()
    expect(circular).toHaveLength(0)
  })
})
```

## Drift Detection

### JSON Schema Snapshot Workflow

Use the [maintained exporter and CI example](linter-and-ci.md#ci-schema-snapshot-regression). Check changed, added and removed snapshot files, and verify the expected filename set. JSON Schema snapshots cover representable shape; custom refinements and transforms still need behavioral tests.

## Observable Error Testing

### Assert flattenError Structure

Test that validation errors produce the expected structured output for logging and monitoring.

```typescript
describe("error observability", () => {
  it("flattenError has expected field keys", () => {
    const result = UserSchema.safeParse({ name: "", email: "bad" })
    expect(result.success).toBe(false)
    if (!result.success) {
      const flat = z.flattenError(result.error)
      // Verify structure matches what logging expects
      expect(flat).toHaveProperty("formErrors")
      expect(flat).toHaveProperty("fieldErrors")
      expect(flat.fieldErrors).toHaveProperty("email")
    }
  })

  it("error messages are user-facing quality", () => {
    const Schema = z.object({
      email: z.email({ error: "Please enter a valid email" }),
      age: z.number({ error: "Age must be a number" }).min(0, "Age cannot be negative"),
    })

    const result = Schema.safeParse({ email: "bad", age: -1 })
    expect(result.success).toBe(false)
    if (!result.success) {
      const flat = z.flattenError(result.error)
      expect(flat.fieldErrors.email![0]).toBe("Please enter a valid email")
      expect(flat.fieldErrors.age![0]).toBe("Age cannot be negative")
    }
  })
})
```

## Performance Testing

Measure representative valid and invalid inputs with the repository's benchmark harness. Record version, machine and baseline; do not introduce a universal wall-clock threshold into unit tests. Require a regression budget only when repeatable measurements support it.

### With Vitest Bench

```typescript
import { bench, describe } from "vitest"

describe("schema benchmarks", () => {
  const validUser = { name: "Alice", email: "alice@example.com", age: 30 }

  bench("UserSchema.safeParse", () => {
    UserSchema.safeParse(validUser)
  })

  bench("UserSchema.safeParse (invalid)", () => {
    UserSchema.safeParse({ name: 123 })
  })
})
```

Run with:
```bash
vitest bench
```
