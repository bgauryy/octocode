# Zod 4 behaviors to check

Verified on 4.6.7. Each one breaks a schema, its JSON Schema output or a test when assumed from Zod 3 or from memory.

- `z.toJSONSchema` defaults to `io: 'output'`: a plain `z.object` gets `additionalProperties: false` and `.default()` keys are required; with `io: 'input'`, neither. Refinements vanish. Output-side transforms, dates, bigints and `z.custom` throw unless `unrepresentable: 'any'` (each becomes `{}`). `z.int()` emits ±`MAX_SAFE_INTEGER` bounds. A reused schema is inlined unless it is registered in a `z.registry` (with `uri`) or `reused: 'ref'` is set.
- `.default(v)` short-circuits on `undefined`: it returns `v`, an output value, without running checks or transforms. `.prefault(v)` parses `v`.
- String `min`, `max` and `length` count code points, as JSON Schema does, not UTF-16 `.length`.
- `z.object` strips unknown keys, `z.strictObject` rejects them, `z.looseObject` keeps them. `.extend()` keeps the mode; spreading `.shape` into `z.object()` resets it to strip. `.pick`, `.omit` and `.partial` throw on an object with a refinement; `.extend` keeps the refinement.
- `z.record(z.enum([...]), v)` requires every key (`z.partialRecord` does not). `.optional()` accepts an explicit `undefined`; `.exactOptional()` does not.
- Errors: pass `{ error }` (a string, or a function returning a string or `undefined` to defer). `message` is deprecated; `required_error`, `invalid_type_error` and `errorMap` are gone; a schema's `error` beats a per-parse one. Read `err.issues` (there is no `.errors`) and format with `z.prettifyError`, `z.treeifyError` or `z.flattenError`. An issue carries the raw `input` only with `reportInput: true`: keep that off for real data.
- `safeParse` does not catch an exception thrown inside `refine` or `transform`. An async check under `parse` throws `$ZodAsyncError`; use `parseAsync`.
- `z.coerce.boolean()` turns `"false"` into `true` (use `z.stringbool()`). `z.number()` rejects `Infinity`; `z.int()` accepts safe integers only.
- Prefer `z.email()`, `z.url()`, `z.uuid()`, `z.iso.datetime()` to the string methods, `z.enum(obj)` to `z.nativeEnum`, `A.extend(B.shape)` to `.merge`, `z.ZodType` to `ZodTypeAny`. `z.input` and `z.output` type the two sides of a transform.
