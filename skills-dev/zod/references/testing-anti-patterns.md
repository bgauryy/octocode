Load when reviewing schema tests for weak assertions or missing cases.

# Zod Testing Anti-Patterns

## Table of Contents

- Testing schema internals instead of behavior
- Not testing error paths
- Not asserting the chosen parsing contract
- Not testing boundary values
- Relying only on generated fixtures
- Duplicating schema logic in assertions
- Testing transform output without testing input validation
- Not testing optional/nullable combinations
- Snapshot testing raw ZodError
- Not seeding random data generators
- Not testing at boundaries
- No snapshot regression testing
- Testing schema shape but not error observability
- No drift detection workflow

## Testing schema internals instead of behavior

```typescript
// BAD: testing internal structure — breaks on refactors
it("has correct shape", () => {
  expect(UserSchema.shape.name).toBeDefined()
  expect(UserSchema.shape.email).toBeDefined()
  expect(UserSchema._def.typeName).toBe("ZodObject")
})

// GOOD: test parse behavior — stable across refactors
it("accepts valid user", () => {
  const result = UserSchema.safeParse({
    name: "Alice",
    email: "alice@example.com",
  })
  expect(result.success).toBe(true)
})

it("rejects missing name", () => {
  const result = UserSchema.safeParse({ email: "alice@example.com" })
  expect(result.success).toBe(false)
})
```

## Not testing error paths

```typescript
// BAD: only testing valid inputs
describe("UserSchema", () => {
  it("accepts valid user", () => {
    expect(UserSchema.safeParse(validUser).success).toBe(true)
  })
  // No tests for invalid data — regressions go unnoticed
})

// GOOD: test both valid and invalid
describe("UserSchema", () => {
  it("accepts valid user", () => {
    expect(UserSchema.safeParse(validUser).success).toBe(true)
  })

  it("rejects empty name", () => {
    expect(
      UserSchema.safeParse({ ...validUser, name: "" }).success
    ).toBe(false)
  })

  it("rejects invalid email", () => {
    expect(
      UserSchema.safeParse({ ...validUser, email: "not-email" }).success
    ).toBe(false)
  })

  it("rejects negative age", () => {
    expect(
      UserSchema.safeParse({ ...validUser, age: -1 }).success
    ).toBe(false)
  })
})
```

## Not asserting the chosen parsing contract

```typescript
// Valid when the throwing API is the contract under test
it("rejects invalid data", () => {
  expect(() => UserSchema.parse(invalidData)).toThrow(z.ZodError)
  // The assertion fails if parsing stops throwing.
})

// BAD: try/catch obscures the test intent
it("rejects invalid data", () => {
  try {
    UserSchema.parse(invalidData)
    fail("should have thrown")
  } catch (e) {
    expect(e).toBeInstanceOf(z.ZodError)
  }
})

// GOOD: clean, predictable assertions
it("rejects invalid data", () => {
  const result = UserSchema.safeParse(invalidData)
  expect(result.success).toBe(false)
  if (!result.success) {
    expect(result.error.issues).toHaveLength(2)
  }
})
```

## Not testing boundary values

```typescript
const AgeSchema = z.number().min(0).max(150)

// BAD: only testing middle values
it("validates age", () => {
  expect(AgeSchema.safeParse(25).success).toBe(true)
  expect(AgeSchema.safeParse(-5).success).toBe(false)
})

// GOOD: testing boundaries
it.each([
  [0, true],    // minimum boundary
  [150, true],  // maximum boundary
  [-1, false],  // one below minimum
  [151, false], // one above maximum
  [75, true],   // middle value
])("age %i → %s", (age, expected) => {
  expect(AgeSchema.safeParse(age).success).toBe(expected)
})
```

## Relying only on generated fixtures

A hand-authored fixture failing after a required field is added is a visible regression signal, not a silent failure. Keep examples derived from the consumer contract. Generated fixtures supplement them but cannot prove a schema correct when the same schema determines both generator and validator.

```typescript
// Useful independent contract case
it("rejects a missing required role", () => {
  expect(UserSchema.safeParse({ name: "Alice", email: "alice@example.com", age: 25 }).success).toBe(false)
})

// Supplemental volume: verify generator/version support and seed it
beforeEach(() => seed(42))
it("accepts generated user", () => {
  expect(UserSchema.safeParse(fake(UserSchema)).success).toBe(true)
})
```

## Duplicating schema logic in assertions

```typescript
// BAD: re-implementing validation in the test
it("validates email format", () => {
  const email = "test@example.com"
  const result = EmailSchema.safeParse(email)
  expect(result.success).toBe(true)
  // Duplicating the schema's own regex check in the test
  expect(email).toMatch(/^[^\s@]+@[^\s@]+\.[^\s@]+$/)
})

// GOOD: trust the schema — test behavior, not implementation
it("validates email format", () => {
  expect(EmailSchema.safeParse("test@example.com").success).toBe(true)
  expect(EmailSchema.safeParse("not-an-email").success).toBe(false)
})
```

## Testing transform output without testing input validation

```typescript
const DateString = z.string()
  .refine((s) => !isNaN(Date.parse(s)), { error: "Invalid date" })
  .transform((s) => new Date(s))

// BAD: only testing the transform output
it("transforms to Date", () => {
  const result = DateString.parse("2024-01-01")
  expect(result).toBeInstanceOf(Date)
})

// GOOD: test both validation and transform
it("transforms valid date string", () => {
  const result = DateString.safeParse("2024-01-01")
  expect(result.success).toBe(true)
  if (result.success) {
    expect(result.data).toBeInstanceOf(Date)
    expect(result.data.getUTCFullYear()).toBe(2024)
  }
})

it("rejects invalid date string", () => {
  const result = DateString.safeParse("not-a-date")
  expect(result.success).toBe(false)
})
```

## Not testing optional/nullable combinations

```typescript
const Schema = z.object({
  name: z.string(),
  bio: z.string().optional(),
  avatar: z.string().nullable(),
})

// BAD: only testing with all fields present
it("accepts user", () => {
  expect(Schema.safeParse({
    name: "Alice",
    bio: "Hello",
    avatar: "https://example.com/img.png",
  }).success).toBe(true)
})

// GOOD: test all optional/nullable variants
it("accepts without optional field", () => {
  expect(Schema.safeParse({ name: "Alice", avatar: null }).success).toBe(true)
})

it("accepts with null nullable", () => {
  expect(Schema.safeParse({ name: "Alice", bio: "Hello", avatar: null }).success).toBe(true)
})

it("rejects null for non-nullable optional", () => {
  expect(Schema.safeParse({ name: "Alice", bio: null, avatar: null }).success).toBe(false)
})

it("rejects undefined for non-optional nullable", () => {
  expect(Schema.safeParse({ name: "Alice" }).success).toBe(false)
  // avatar is nullable but not optional — must be explicitly provided
})
```

## Snapshot testing raw ZodError

```typescript
// BAD: raw ZodError snapshots are noisy and brittle
it("error matches snapshot", () => {
  const result = schema.safeParse(invalid)
  if (!result.success) {
    expect(result.error).toMatchSnapshot() // huge, includes internals
  }
})

// GOOD: assert rejection before snapshotting formatted output
it("error matches snapshot", () => {
  const result = schema.safeParse(invalid)
  expect(result.success).toBe(false)
  if (!result.success) {
    expect(z.flattenError(result.error)).toMatchSnapshot()
    // Clean: { formErrors: [], fieldErrors: { email: ["Invalid email"] } }
  }
})

// GOOD: snapshot JSON Schema for schema shape
it("schema shape matches snapshot", () => {
  expect(z.toJSONSchema(schema)).toMatchSnapshot()
})
```

## Not seeding random data generators

```typescript
// BAD: non-deterministic — test passes sometimes, fails others
it("accepts generated data", () => {
  const user = fake(UserSchema) // different every run
  expect(UserSchema.safeParse(user).success).toBe(true)
  // If this fails, you can't reproduce it
})

// GOOD: seeded — deterministic and reproducible
beforeEach(() => seed(42))

it("accepts generated data", () => {
  const user = fake(UserSchema) // same every run
  expect(UserSchema.safeParse(user).success).toBe(true)
})
```

## Not testing at boundaries

```typescript
// BAD: schema unit tests pass but handler never calls safeParse
// tests/user-schema.test.ts
it("rejects invalid email", () => {
  expect(UserSchema.safeParse({ email: "bad" }).success).toBe(false)
})
// But the actual handler does:
app.post("/users", (req, res) => {
  // No validation! req.body goes straight to the database
  db.users.create(req.body)
})

// GOOD: integration test verifies the boundary actually validates
it("API rejects invalid input with field errors", async () => {
  const res = await request(app)
    .post("/api/users")
    .send({ name: "", email: "bad" })
    .expect(400)

  expect(res.body.errors.fieldErrors).toHaveProperty("email")
})

it("API accepts valid input", async () => {
  const res = await request(app)
    .post("/api/users")
    .send({ name: "Alice", email: "alice@example.com", age: 30 })
    .expect(201)
})
```

## No snapshot regression testing

Use shape snapshots when consumers rely on an exported contract. Custom refinements and transforms still need behavior tests; JSON Schema does not represent every Zod behavior.

```typescript
// BAD: field removal goes unnoticed
// Someone removes `role` from UserSchema — no test catches it
const UserSchema = z.object({
  name: z.string(),
  email: z.email(),
  // role was here — removed without anyone noticing
})

// GOOD: JSON Schema snapshot catches schema changes
it("schema shape has not changed", () => {
  const jsonSchema = z.toJSONSchema(UserSchema)
  expect(jsonSchema).toMatchSnapshot()
  // If `role` is removed, snapshot diff shows it clearly in code review
})
```

## Testing schema shape but not error observability

```typescript
// BAD: tests verify schema accepts/rejects but never check error structure
it("rejects invalid input", () => {
  const result = UserSchema.safeParse({ email: "bad" })
  expect(result.success).toBe(false)
  // Never checks what the error looks like — logging could be broken
})

// GOOD: verify flattenError produces queryable structure
it("produces structured errors for logging", () => {
  const result = UserSchema.safeParse({ email: "bad" })
  expect(result.success).toBe(false)
  if (!result.success) {
    const flat = z.flattenError(result.error)
    expect(flat.fieldErrors).toHaveProperty("email")
    expect(flat.fieldErrors.email![0]).toBeTruthy()
    // Verifies the formatter output; test the logging call separately
  }
})
```

## No drift detection workflow

When schema snapshots are part of the consumer contract, CI must notice changed, added and removed snapshots. `git diff` alone misses untracked files; also check the expected filename set so a removed schema cannot leave an obsolete snapshot. Use the maintained [lint and CI workflow](linter-and-ci.md#ci-schema-snapshot-regression), then review intentional changes before updating snapshots.
