Load when naming a dependency, coupling or cohesion term, a structural metric (fan-in, fan-out, instability), or an architecture smell.

# Concepts: coupling and smells

## Dependencies and coupling

Vocabulary for the edges the graph shows. Metrics are component-level hypotheses; the consequence still needs a consumer or change-impact trace ([proof and ranking](analysis-proof.md#proof-and-ranking)).

| Term | Meaning | Evidence |
|---|---|---|
| Afferent coupling (Ca) | Incoming dependents of a module | graphQuery `dependents`, `hubs` with `direction: "in"` |
| Efferent coupling (Ce) | Outgoing dependencies of a module | graphQuery `deps`, `hubs` with `direction: "out"` |
| Instability (I) | Ce / (Ca + Ce): 0 stable, 1 unstable | No detector: compute from the Ca and Ce rows above |
| Abstractness (A) | Share of abstract types in a component | No detector: count exported traits, interfaces and types with astSearch `symbols` |
| Main sequence | A + I = 1; zone of pain (stable and concrete), zone of uselessness (abstract and unused) | No detector; foundation modules (text, types, config) sit in the zone of pain by design, so confirm with real consumers |
| Cumulative component dependency (CCD, NCCD) | Sum of each file's transitive reach; normalized against a balanced tree | `issues` summary `health.ccd`, `health.nccd`, `averageReach` |
| Stable dependencies principle | Depend toward stability | A stable module importing an unstable one is a candidate |
| Stable abstractions principle | Stable modules are abstract | Interfaces live where dependents point |
| Acyclic dependencies principle | No cycles between components | graphQuery `cycles`, `issues` with `detectors: ["cycle","dir-cycle"]` |
| Reuse-release, common-closure, common-reuse | Package by what releases, changes, and is used together | Change history and consumer sets |
| Dependency direction versus call direction | An interface in the core is implemented and called from outside; the import points inward | Separate `imports` from `calls` edges (`edges: ["imports"]`, `edges: ["calls"]`) |
| Type-only edge | Import used only in type positions | `typesOnly` on `impact` rows; no runtime proof |
| Transitive dependency | Reached through another module | `walk` with `depth: n`, `path` |
| Fan-in, fan-out | Caller and callee counts of a symbol | `callers`, `callees` |
| Re-export (barrel) edge | Dependent reaches a file through a re-exporting module | `via` on `dependents` rows: `reexport` for a consumer's `uses` edge resolved through the barrel; for the barrel's own `imports` edge, `named-reexport` (`export {…} from`) or `star-reexport` (`export * from`), each with a `type-` form for type-only re-exports. Plain imports show `static-import` |

Coupling kinds, strongest first: content (reaching into internals), common (shared global state), control (a flag that selects the callee's behavior), stamp (passing a structure for one field), data (passing only what is needed), message (communicating by messages). Cohesion is the inverse at module level: one capability per module.

Connascence names what two parts must agree on, weakest to strongest: name, type, meaning (magic values), position (argument order), algorithm (both sides implement the same rule), execution order, timing, value (invariants across copies), identity (the same instance). Strong connascence across a module boundary is a coupling candidate; inside a module it is normal.

## Architecture smells

Graph detectors emit ranked hypotheses to confirm against source and `lspSearch`, never findings ([evidence lanes](analysis-topology.md#choose-the-narrowest-evidence-lane)). Smells without a detector need scenarios, AST shape, and LSP references.

| Smell | What it looks like | Detector or lane |
|---|---|---|
| Cyclic dependency | Modules or directories that import each other, directly or through a chain | `cycle`, `dir-cycle`; rule out type-only edges |
| Hub-like or god module | Many incoming and outgoing edges plus mixed responsibilities | No detector: `critical-file` ranks blast radius; inspect responsibilities, not size |
| Critical file, single point of failure | Most reachable code passes through one file | `critical-file`, `single-point-of-failure`; composition roots and generated registries are expected hubs |
| Unstable dependency | A stable module depends on an unstable one | No detector: compare I of both ends; a module root using its own submodules is not one |
| Boundary violation | An import crosses a declared layer or package boundary | `boundary-violation` with the declared rule; folder names alone are `inferred` |
| Misplaced file | A file whose dependencies belong to another module | No detector: compare the file's graphQuery `walk` imports with its module's |
| Dead or test-only code | Unreachable file, unused export, export used only locally | `unreachable-file`, `unused-export`, `export-only-local`, `test-only` with explicit `entries`; rule out registration and external consumers |
| Undeclared dependency | Import absent from the manifest | `undeclared-dependency`; a development-only dependency in production code needs the package's bundling and publishing read from its manifest |
| Unresolved import | An edge the linker could not resolve | `unresolved-import`: a coverage gap, not an absence |
| Scattered concern, shotgun surgery | One change touches many modules | `impact` with `since: "<rev>"` over history; AST for the repeated shape |
| Feature envy | A module operates mostly on another module's data | `callees` and `uses` edges; LSP references |
| Leaky abstraction | Transport, storage, or framework types in a core interface | AST on signatures; [contract trace](checks.md#contract-trace) |
| Parallel implementation | One rule, allowlist or schema in two places without a parity fixture | Semantic comparison ([unify](change-removal.md#delete-junk-and-unify)) |
| Speculative abstraction | One-implementation interface, one-value flag | `inherits` edges; consumers |
