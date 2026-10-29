Load when designing schema correctness and integration tests.

<!-- Testing guidance adapted from the former anivar/zod-testing skill. -->

# Zod Schema Testing Guide

Use the installed Zod version and consumer contract as the authority. The [test API reference](testing-api.md) owns assertion and fixture examples; [test anti-patterns](testing-anti-patterns.md) owns failure cases. Do not load both in full when one targeted section answers the question.

## Choose the evidence

1. Read the schema, producer and consumer. State accepted input, transformed output, unknown-key policy and failure shape before writing tests.
2. Choose independent valid, invalid and boundary examples. For numeric bounds include the endpoints and values just outside; for optional/nullable fields distinguish missing, undefined and null. Keep other fields valid so a rejection tests the intended rule.
3. Assert the API the consumer uses: inspect `safeParse` results or assert that `parse` throws. Use the async parser and await assertions for async refinements/transforms. Callback exceptions are separate from validation failures.
4. Verify transformed values and error paths, not only success. Integration tests must call the actual handler/service and verify relevant side effects; parsing in the test and asserting an untouched database mock proves nothing about the handler.
5. Run the relevant suite. Report what it proves and what remains untested; formatting an error does not prove the logging pipeline or form UI rendered it.

```typescript
// A throwing assertion fails if the schema starts accepting this input.
expect(() => schema.parse(invalidData)).toThrow(z.ZodError)

// For a result contract, assert rejection before conditional issue checks.
const result = schema.safeParse(invalidData)
expect(result.success).toBe(false)
if (!result.success) {
  expect(result.error.issues[0].path).toEqual(["email"])
}
```

## Load the needed example

| When | Read | Check |
|---|---|---|
| Basic acceptance, boundary or nullable cases | [Correctness](testing-api.md#schema-correctness-testing) | Independent expected behavior; isolate the changed field |
| Error shape or custom messages | [Error assertions](testing-api.md#error-assertion-patterns) | Expected code/path/message; assertions must run |
| Many fixtures | [Generation](testing-api.md#mock-data-generation) | Installed-version support, seed and custom-refinement limits |
| Exported contract shape | [Snapshots](testing-api.md#snapshot-testing), [CI owner](linter-and-ci.md#ci-schema-snapshot-regression) | Review added/changed/removed files; JSON Schema cannot represent every check |
| Handler, form or persistence behavior | [Integration](testing-api.md#integration-testing) | Call the application boundary and assert its output/side effects |
| Invariants or inverse conversions | [Properties](testing-api.md#property-based-testing) | Independent generators or a real encode/decode inverse property |
| Async or reusable assertions | [Async](testing-api.md#async-schema-testing), [helpers](testing-api.md#test-helpers) | Deterministic fakes; awaited work; failed branch cannot pass silently |
| Architecture or performance contract | [Structure](testing-api.md#structural-testing), [benchmarks](testing-api.md#performance-testing) | Resolved dependencies or measured baseline, not regex/constant timing guesses |

Generated fixtures supplement hand-authored contract cases. A generator and validator derived from the same schema can agree on the same mistake. Keep reproducible seeds and failing cases. Snapshot updates require review; retain behavior tests for refinements, transforms and constraints not represented in JSON Schema. Do not test private schema internals merely to mirror the implementation.
