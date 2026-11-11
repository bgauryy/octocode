---
name: zod
description: "Create, change, review or test Zod schemas, or migrate v3 to v4. Covers parsing, inferred types, composition, errors, transforms and boundary tests against the installed version."
license: MIT
user-invocable: false
agentic: false
compatibility: "TypeScript ^5.5 projects using zod ^4.3.0 or later"
metadata:
  author: Anivar Aravind
  author_url: https://anivar.net
  version: 1.1.0
  tags: zod, validation, schema, typescript, parsing, type-inference, forms, api, testing, jest, vitest
---

# Zod

## Working rules

Read the installed version, owning schema and consumer before choosing an API. Resolve discrepancies against project source and upstream documentation. Migrate only when requested or required by the consumer.

Parse untrusted input once at its boundary and pass validated values inward. Preserve the repository's schema generator. Choose `safeParse` for expected validation failures or `parse` when callers expect an exception. Await the async parser for async refinements or transforms. Exceptions from user callbacks can still escape.

Decide which unknown keys to strip (`object`), reject (`strictObject`) or preserve (`looseObject`). Derive pre-transform types with `z.input` and output types with `z.output` or `z.infer`. Verify accepted, rejected and boundary inputs through the real consumer, including transformed values and error paths. Keep sensitive raw input out of diagnostics.

## Resources

| When | Read | Why |
|---|---|---|
| Primitives, formats, optional/nullable | [Schema types](references/schema-types.md) | Value constraints |
| Parser, coercion or inferred types | [Parsing](references/parsing-and-inference.md) | Input/output contract |
| Objects, unions or recursion | [Composition](references/objects-and-composition.md) | Schema structure |
| Conversion, pipes or cross-field checks | [Refinements and transforms](references/refinements-and-transforms.md) | Validation versus transformation |
| Brands or encode/decode | [Advanced APIs](references/advanced-features.md), [brand](rules/pattern-branded-types.md), [codec](rules/pattern-codecs.md) | Identity and round trips |
| Errors or observability | [Errors](references/error-handling.md), [custom messages](rules/error-custom-messages.md), [structured errors](rules/observe-structured-errors.md), [metrics](rules/observe-error-metrics.md) | Safe failure diagnostics |
| Placement or evolution | [Boundary architecture](references/boundary-architecture.md); [parsing](rules/arch-boundary-parsing.md), [organization](rules/arch-schema-organization.md), [versioning](rules/arch-schema-versioning.md) | One boundary owner |
| Composition or bundle cost | [Shape spread](rules/perf-extend-spread.md), [reuse](rules/perf-reuse-schemas.md), [Mini](rules/perf-zod-mini.md) | Measure the relevant cost |
| Recurring mistakes or v3 migration | [Anti-patterns](references/anti-patterns.md) | Symptom → repair |
| Enforcement or tests | [Lint/CI](references/linter-and-ci.md), [testing](references/testing.md) | Assertions, fixtures and integration |

Use `octocode-research` for unresolved source behavior, `octocode-engineer` for boundary design, and `octocode-dev` for Octocode’s generated contracts and checks. Follow [output.md](output.md) for delivery.
