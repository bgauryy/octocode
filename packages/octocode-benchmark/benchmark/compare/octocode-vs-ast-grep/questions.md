# Octocode vs ast-grep — Questions

`ROOT` = absolute repo path.

## Q1

Across `$ROOT/packages/octocode-tools-core/src`, find every call of the shape
`JSON.parse(<arg>)`. Report the count and the files.

## Q2

Across `$ROOT/packages/octocode-tools-core/src/tools`, report the count of
exported function declarations (with a body) and, separately, the count of
non-exported function declarations.

## Q3

Across `$ROOT/packages/octocode-tools-core/src`, find every `await` expression
inside a `try { … }` block. Report the count.

## Q4

Across `$ROOT/packages/octocode-tools-core/src`, find every method call of the
shape `<obj>.push(<args>)`. Report the count and the top 3 files by hits.

## Q5

(a) In `$ROOT/packages/octocode-tools-core/src`, count call expressions of the
shape `<fn>(<args>)`. (b) In `$ROOT/packages/octocode-engine/src`, count
function/macro call shapes. Report both counts.

## Q6

`isLocalTool` is defined in
`$ROOT/packages/octocode-tools-core/src/tools/toolNames.ts`. Give the
`file:line` of every place that calls it across the repo.

## Q7

In `pmndrs/zustand`'s `src/` directory, find every call expression of the shape
`create(<args>)`. Report the count and sample files.

## Q8

In `$ROOT/packages/octocode-tools-core/src`, report the files that both have
`scheme` in their path or content and contain a `z.object(<args>)` call, and the
count of those calls.

## Q9

Give the `file:line` of the first call of the shape `describe(<args>)` under
`$ROOT/packages`, and its enclosing function/block.

## Q10

For a `.json`, `.yaml`, or `.md` file in this repo, give a structural or
symbol-level result over it.
