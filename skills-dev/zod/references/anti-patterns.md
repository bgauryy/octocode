Load when a Zod API behaves unexpectedly or an existing v3 schema needs migration. Follow the rule for the observed symptom and verify its behavior with the installed version.

# Zod Anti-Patterns

## Using parse() with try/catch instead of safeParse()

Use a result for an expected rejection and preserve throwing when the caller relies on it. [Parsing contract](../rules/parse-use-safeParse.md)

## Sync parse with async refinements

A synchronous parser may start the callback before throwing. Await the async parser. [Async refinements and transforms](../rules/parse-async-required.md)

## Manual type definitions alongside schemas

Derive the input and output types from the same schema, especially across a transform. [Type inference](../rules/parse-infer-types.md)

## Deprecated string format chaining (v4)

When migrating chained format methods such as `z.string().email()`, use their v4 top-level counterparts. [String formats](../rules/migrate-string-formats.md)

## Using z.nativeEnum() (v4)

New v4 schemas can pass the TypeScript enum to `z.enum()`. The old API remains exported. [Enum migration](../rules/migrate-native-enum.md)

## Using required_error/invalid_type_error (v4)

Use v4's `error` parameter for the removed options. The old `message` option is deprecated. [Error migration and shorthand](../rules/migrate-error-api.md)

## Assuming error formatters have the same shape

Match the formatter to the consumer. A tree with `_errors` differs from one with `properties` and `errors`. [Formatter examples](../rules/error-formatting.md)

## Throwing inside refinements or transforms

For expected invalid input, emit a validation issue. When converting decimal strings, validate the entire input before converting because `parseInt` accepts prefixes. [Callback errors](../rules/refine-never-throw.md) · [Refine or transform](../rules/refine-vs-transform.md) · [Cross-field checks](../rules/refine-cross-field.md) · [Staged parsing](../rules/pattern-pipe.md)

## z.coerce.boolean() for string booleans

JavaScript treats the strings `"false"` and `"0"` as truthy. Use `z.stringbool()` when those inputs mean false. [Coercion pitfalls](../rules/schema-coercion-pitfalls.md)

## Assuming z.object() preserves unknown keys

Choose the object's unknown-key policy before forwarding or persisting parsed data. [Unknown keys](../rules/schema-object-unknowns.md)

## z.union() for tagged objects

When branches have a shared discriminator, use it to select a branch and target errors. [Tagged unions](../rules/schema-union-discriminated.md)

## reportInput in production

Omit raw input in every environment handling real data. [Input privacy](../rules/error-input-security.md)

## z.lazy() for recursive schemas (v4)

V4 supports recursive getters. `z.lazy()` remains available and can need a TypeScript annotation. [Recursive schemas and cyclic inputs](../rules/schema-recursive-types.md)

## Duplicating field definitions

Where field semantics agree, derive variants with an explicit choice of fields and optionality. [Schema reuse](../rules/perf-reuse-schemas.md)

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
