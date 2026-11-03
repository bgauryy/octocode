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

- For primitives, formats and optional or nullable values, use [schema types](references/schema-types.md). For parser choice, coercion and type inference, use [parsing](references/parsing-and-inference.md).
- For objects, unions or recursion, use [composition](references/objects-and-composition.md). For conversion, pipes and cross-field checks, use [refinements and transforms](references/refinements-and-transforms.md). For brands and encode/decode contracts, use [advanced APIs](references/advanced-features.md) and the focused [brand](rules/pattern-branded-types.md) or [codec](rules/pattern-codecs.md) example.
- For error shape and customization, use [errors](references/error-handling.md) or [custom messages](rules/error-custom-messages.md). When instrumenting failures, use [structured errors](rules/observe-structured-errors.md) and [error metrics](rules/observe-error-metrics.md).
- For schema placement and evolution, use [boundary architecture](references/boundary-architecture.md) and its [parsing](rules/arch-boundary-parsing.md), [organization](rules/arch-schema-organization.md) or [versioning](rules/arch-schema-versioning.md) rule.
- When measuring schema composition or bundle cost, check [shape spread](rules/perf-extend-spread.md), [schema reuse](rules/perf-reuse-schemas.md) or [Zod Mini](rules/perf-zod-mini.md).
- To diagnose a recurring mistake or migrate v3 string formats, enums or errors, look up the symptom in [anti-patterns](references/anti-patterns.md). Use [lint and CI](references/linter-and-ci.md) for enforcement and [testing](references/testing.md) for assertion, fixture and integration examples.

Use `octocode-research` for unresolved source behavior, `octocode-architect` for boundary design, and `octocode-dev` for Octocode’s generated contracts and checks. Follow [output.md](output.md) for delivery.
