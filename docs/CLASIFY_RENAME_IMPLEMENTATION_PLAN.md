# `semanticAssess` to `clasify` implementation plan

Status: proposed. This plan coordinates a hard public rename across the canonical contract package in `../octocode-mcp-host` and the runtime and interface packages in this repository.

> **Intentional spelling:** The target public name is `clasify`. Do not normalize it to `classify`.

## Goal

Replace the public MCP tool and CLI command `semanticAssess` with `clasify` while preserving the existing semantic-query grammar, provider behavior, security policy, availability gates, and configuration system.

Complete the work in this order:

1. Change and verify `@octocodeai/octocode-core` in `../octocode-mcp-host`.
2. Commit the core change so native contract provenance can reference a clean source revision.
3. Regenerate and update the native runtime in this repository.
4. Update CLI, MCP, skills, benchmarks, active documentation, and release guidance.
5. Rebuild and verify the real CLI and MCP paths.

Do not ship either repository independently. A core catalog that advertises `clasify` cannot work with a native runtime that still dispatches `semanticAssess`.

For ownership and existing behavior, see the [native runtime architecture](../packages/octocode-native/ARCHITECTURE.md), the [public tools reference](OCTOCODE_TOOLS.md), and the completed [`semanticAssess` implementation plan](JEV_IMPROVEMENT_PLAN.md).

## Scope

The rename includes every active public surface that identifies or resumes this tool:

| Current surface | Target surface |
|---|---|
| MCP and CLI name `semanticAssess` | `clasify` |
| Core constant `SEMANTIC_ASSESS` | `CLASIFY` |
| Core types and schemas `SemanticAssess*` | `Clasify*` |
| Core module `validation/semanticAssess.ts` | `validation/clasify.ts` |
| Native identity `ToolId::SemanticAssess` | `ToolId::Clasify` |
| Native CLI variant `SemanticAssess` | `Clasify` |
| Continuation `next.assess` | `next.clasify` |
| Skill `octocode-semantic-assess` | `octocode-clasify` |
| Public usage key `semanticAssessUsage` | `clasifyUsage` |
| Public unavailable code `SEMANTIC_ASSESS_UNAVAILABLE` | `CLASIFY_UNAVAILABLE` |
| Active semantic-assessment document names | `CLASIFY` document names |

The following semantic and provider concepts keep their existing names because they describe behavior rather than the public tool identity:

- `SemanticQuery`, `SemanticQuestion`, `SemanticAnswer`, and other generic semantic-domain types.
- Noul, Choice, and Score primitives and their request and response fields.
- The private Jev provider adapter, model names, and provider-specific modules.
- `OCTOCODE_CLASSIFICATION_API`, `OCTOCODE_CLASSIFICATION_TYPE`, and the supported `OCTOCODE_JEV_KEY` provider alias.
- Resource capture, paging, correlation, and evidence-boundary semantics.

## Explicit configuration exclusion

Do not change the configuration implementation for this rename:

- Do not edit `packages/octocode-config/**`.
- Do not add or rename environment variables.
- Do not change provider-key resolution, configuration precedence, or secret loading.
- Do not add a configuration compatibility layer for the old tool name.

An operator who explicitly lists `semanticAssess` in `tools.enabled`, `tools.disabled`, `TOOLS_TO_RUN`, or `DISABLE_TOOLS` must replace that value with `clasify`. This is a documented tool-name migration, not a change to the configuration schema or resolver.

## Cutover policy

Use a hard cutover:

- Register and advertise only `clasify`.
- Do not add a `semanticAssess` alias, forwarding command, duplicate schema, or deprecated skill.
- Make `semanticAssess` fail as an unknown tool or command after the cutover.
- Update all first-party callers in the same release.
- Preserve historical records that accurately describe the old interface. Label the old name as historical instead of rewriting frozen evidence.

This policy follows the existing no-compatibility-shim convention for public tool cutovers.

## Preconditions

Both repositories can contain unrelated in-progress edits. Before implementation:

1. Inspect `git status` in `../octocode-mcp-host` and this repository.
2. Commit, checkpoint, or coordinate overlapping work. Do not reset or overwrite unrelated changes.
3. Run the existing core tests and record the baseline result.
4. Run `yarn workspace @octocodeai/octocode-native contracts:check` and record any existing contract drift separately from rename failures.
5. Freeze the target contract table in this document. Do not expand the task into a payload redesign.

The native contract generator rejects a dirty core checkout by default. A dirty regeneration is acceptable for local iteration with `--allow-dirty`, but final generated artifacts must report `sourceDirty:false` from a committed core revision.

## Phase 1: Rename the canonical core contract

Repository: `../octocode-mcp-host`

Package: `packages/octocode-core`

### Add failing contract tests

Update or add tests before changing the implementation. The tests must establish the new public boundary:

- The direct catalog contains exactly one `clasify` tool.
- The catalog contains no `semanticAssess` entry.
- The tool order and total count remain unchanged.
- `prepareDirectToolInput("clasify", input)` accepts the existing direct and batched semantic-query forms.
- Preparing `semanticAssess` fails as an unknown tool.
- The public input and output schemas export the `Clasify*` names.
- The output continuation uses `next.clasify` and validates the existing semantic query input.
- Availability-aware MCP and CLI instructions mention `clasify` only when the tool is enabled.
- Generated command patterns use `clasify`.
- The direct-tool schema fixture is named `clasify.json` and declares `"name": "clasify"`.

Rename `src/__tests__/semanticAssess.test.ts` to `src/__tests__/clasify.test.ts` so the test owner follows the public contract.

### Change the canonical tool identity

Update the identity and catalog owners:

- `src/toolContract/names.ts`
- `src/toolContract/catalog.ts`
- `src/toolContract/descriptions.ts`
- `src/toolContract/instructions.ts`
- `src/toolContract/cliContext.ts`

Use `TOOL_NAMES.CLASIFY` as the single core constant. Update the public title and descriptions to make selection behavior clear:

- Use `clasify` for bounded Noul, Choice, or Score judgments over unread resources or supplied work.
- Do not use it for exact lookup, deterministic checks, required proof, or free-form summarization.
- State that caller-supplied alternatives and rubrics constrain the result.
- Preserve the rule that the result routes work but does not prove a source claim.

Do not duplicate field mechanics from the schema in the tool description.

### Rename the public schema module and exports

Rename the public module and tool-coupled declarations:

- `src/toolContract/validation/semanticAssess.ts` to `src/toolContract/validation/clasify.ts`
- `SemanticAssessInputSchema` to `ClasifyInputSchema`
- `SemanticAssessInput` to `ClasifyInput`
- `SemanticAssessOutputSchema` to `ClasifyOutputSchema`
- `SemanticAssessOutput` to `ClasifyOutput`

Update imports and exports in:

- `src/schema.ts`
- `src/toolContract/outputSchemas.ts`
- `src/toolContract/catalog.ts`
- native rule modules and fixtures

Keep generic declarations such as `SemanticQuerySchema`, `SemanticQuestionSchema`, `SemanticContextReceiptSchema`, and `SemanticAnswerSchema`. Renaming those declarations creates churn without improving tool discovery.

### Rename continuation ownership

Change the semantic query result continuation from:

```text
next.assess
```

to:

```text
next.clasify
```

Update its schema description to tell agents to execute the returned `clasify` input unchanged. Keep page indexes, query IDs, resource IDs, question IDs, coverage, and page-local answers unchanged.

### Update preparation, discovery, and validation

Replace tool-name branches in:

- `src/toolContract/discovery/toolInputPreparation.ts`
- `src/toolContract/discovery/toolCommandPatternQueries.ts`
- `src/toolContract/nativeRules/catalog.ts`
- `src/toolContract/nativeRules/fixtures.ts`
- any schema audit or presentation helper that compares the old literal

Update validation diagnostics so they name `clasify`. Preserve accepted payloads, matrix limits, question primitives, and security bounds.

### Update core fixtures and documentation

Update:

- `src/__tests__/__fixtures__/direct-tool-schemas/semanticAssess.json` to `clasify.json`
- `packages/octocode-core/README.md`
- all active core tests that use the public name

Do not manually edit files generated into `dist/`. Build the core package to create the public catalog and schema templates from source.

### Verify and commit core

Run:

```bash
yarn workspace @octocodeai/octocode-core lint
yarn workspace @octocodeai/octocode-core typecheck
yarn workspace @octocodeai/octocode-core test
yarn workspace @octocodeai/octocode-core build
```

Then verify:

- `dist/public-catalog.json` advertises `clasify` and not `semanticAssess`.
- `dist/schema-templates/clasify.json` exists.
- No active core source references the old public name outside an intentional migration assertion.

Commit the core change before final native regeneration. Record the core commit in the implementation receipt.

## Phase 2: Regenerate the native contract

Repository: this repository

Package: `packages/octocode-native`

### Refresh the local core dependency

This workspace resolves `@octocodeai/octocode-core` from `../octocode-mcp-host/packages/octocode-core`. After the core commit and build, refresh the workspace dependency only if the installed package copy is stale. Do not modify `@octocodeai/config` while refreshing dependencies.

### Regenerate, do not hand-edit

Run the contract generator from a clean core revision:

```bash
yarn workspace @octocodeai/octocode-native contracts:regen
```

The generator owns:

- `packages/octocode-native/crates/runtime/src/contracts/generated/tool-contract.json`
- `packages/octocode-native/crates/runtime/src/contracts/generated/contracts.rs`
- `packages/octocode-native/crates/runtime/src/contracts/generated/contract-fixtures.json`
- `packages/octocode-native/crates/runtime/src/contracts/generated/contract-provenance.json`
- the pinned contract body hash in the native contract module

Review the generated diff. It must represent the rename and continuation change without unrelated schema drift. Final provenance must contain the committed core revision and `sourceDirty:false`.

## Phase 3: Rename native runtime dispatch

### Change the typed identity

Update `packages/octocode-native/crates/runtime/src/tools/id.rs`:

- Rename `ToolId::SemanticAssess` to `ToolId::Clasify`.
- Return `"clasify"` from `as_str`.
- Rename `is_semantic_assess` to `is_clasify`.
- Keep the tool in `ToolFamily::Remote`.
- Keep the same availability environment hint.
- Assert that `semanticAssess` does not parse as a tool ID.

### Change the CLI command

Update:

- `crates/runtime/src/cli/commands.rs`
- `crates/runtime/src/cli/mod.rs`

Expose `clasify` as the only Clap subcommand and dispatch it to the same runtime execution path. Remove the old command rather than hiding it behind deprecation logic.

### Change runtime name branches

Replace public-name branches in:

- `crates/runtime/src/contracts/mod.rs`
- `crates/runtime/src/contracts/prepare.rs`
- `crates/runtime/src/contracts/validate.rs`
- `crates/runtime/src/runtime/engine.rs`
- `crates/runtime/src/runtime/error.rs`
- `crates/runtime/src/runtime/response.rs`
- `crates/runtime/src/runtime/session_stats.rs`
- `crates/runtime/src/tools/jev/mod.rs`

The runtime must:

- Gate `clasify` on the existing classification provider key.
- Validate direct and batched semantic queries exactly as before.
- Shape the same page-local typed answers.
- Emit and consume `next.clasify`.
- Report statistics under `clasify`.
- Name `clasify` in actionable errors.

Keep private provider and orchestration modules such as `providers/classification`, `runtime/jev_batch`, and `tools/jev` unless a public literal leaks from them. Their names describe internal ownership and do not affect agent tool selection.

### Rename native tests

Rename `tests/runtime_semantic_assess.rs` to `tests/runtime_clasify.rs` and update native unit and integration tests. Cover:

- Required nonblank reasoning.
- Missing-key diagnostics.
- Provider-key alias behavior.
- Noul, Choice, and Score execution.
- Direct and batched queries.
- Bounded resource paging.
- `next.clasify` execution.
- Output-contract validation.
- Rejection of `semanticAssess`.

## Phase 4: Update MCP and CLI interfaces

### MCP package

Update the active name in:

- `packages/octocode-mcp/manifest.json`
- native server availability tests
- stdio acceptance tests
- pagination-contract tests
- package and architecture documentation

Verify both availability modes:

- Without a classification key, MCP omits `clasify`.
- With a classification key, MCP advertises exactly one `clasify` tool.

Do not alter configuration acquisition or provider-key resolution.

### CLI package

Update CLI tests and documentation under:

- `packages/octocode/tests/cli/**`
- `packages/octocode/docs/OCTOCODE_CLI.md`
- `packages/octocode/README.md`

The Node-owned `scheme` command derives the public name from the core catalog. Avoid adding a second tool-name table in TypeScript.

Verify:

- `octocode scheme clasify --view query --compact` succeeds.
- `octocode clasify --help` succeeds.
- `octocode semanticAssess --help` fails as an unknown command.
- A keyless `octocode clasify` call names the existing classification environment variable and setup action.

## Phase 5: Update first-party skills and scripts

The canonical skill source is the repository root `skills/` directory. Rename:

- `skills/octocode-semantic-assess/` to `skills/octocode-clasify/`
- the frontmatter name to `octocode-clasify`
- active commands and examples to `octocode clasify`
- helper names such as `semantic-assess-local.mjs` to `clasify-local.mjs`
- public helper outputs such as `semanticAssessUsage` to `clasifyUsage`
- public helper errors such as `SEMANTIC_ASSESS_UNAVAILABLE` to `CLASIFY_UNAVAILABLE`

Update callers and guidance in:

- `skills/octocode-research`
- `skills/octocode-scraping`
- `skills/octocode-chrome-devtools`
- `skills/octocode-rfc-generator`
- root and package skill indexes

Build the CLI to stage root skills into `packages/octocode/skills/`. Do not maintain the staged copy as an independent source:

```bash
yarn workspace octocode build:dev
```

Update skill self-tests and script fixtures so they invoke `scheme clasify` and `clasify`.

## Phase 6: Update benchmarks and evaluations

Update active benchmark setup, fixtures, and preflight checks under `packages/octocode-benchmark`:

- Catalog expectations use `clasify`.
- Missing-key checks call `clasify`.
- Current primers and runner context name `clasify`.
- Usage metrics use the new public key when they consume helper output.

Preserve frozen result artifacts that recorded `semanticAssess`. Mark them as pre-rename evidence when readers might mistake them for runnable commands.

Add a focused routing evaluation for the naming hypothesis. Freeze the baseline before changing the evaluated catalog and include:

- Positive cases for unread-candidate classification, Choice, Score, and supplied-draft review.
- Negative cases for exact lookup, deterministic checks, required proof, and free-form summarization.
- Availability cases with and without the provider key.

The rename is behaviorally successful only if correct first-tool selection improves or remains equal without increasing unnecessary classification calls. Tests prove contract integrity; they do not prove that the new name improves agent selection.

## Phase 7: Update active documentation

Rename active documents:

- `docs/OCTOCODE_SEMANTIC_ASSESS.md` to `docs/OCTOCODE_CLASIFY.md`
- `docs/SEMANTIC_ASSESS_RESEARCH_GUIDE.md` to `docs/CLASIFY_RESEARCH_GUIDE.md`

Update active references in:

- `README.md`
- `AGENTS.md`
- `docs/README.md`
- `docs/OCTOCODE_TOOLS.md`
- `docs/OCTOCODE_MCP.md`
- `docs/CONFIGURATION.md`
- `docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md`
- package READMEs and architecture pages
- active skill documentation

The configuration reference changes only its public tool-name examples and allowlist migration note. It must not describe a new environment variable or configuration behavior.

Add a concise migration note:

| Before | After |
|---|---|
| `octocode semanticAssess` | `octocode clasify` |
| `scheme semanticAssess` | `scheme clasify` |
| `next.assess` | `next.clasify` |
| `octocode-semantic-assess` | `octocode-clasify` |

Historical documents such as provider-era benchmark reports and completed implementation receipts can retain `semanticAssess` when that was the observed interface. Add a current-name note where needed, but do not rewrite commands or measurements that were historically executed under the old name.

## Phase 8: Build and verify the integrated system

### Core and native checks

Run:

```bash
cd ../octocode-mcp-host
yarn workspace @octocodeai/octocode-core lint
yarn workspace @octocodeai/octocode-core typecheck
yarn workspace @octocodeai/octocode-core test
yarn workspace @octocodeai/octocode-core build

cd ../octocode
yarn workspace @octocodeai/octocode-native contracts:check
yarn workspace @octocodeai/octocode-native fmt:rust:check
yarn workspace @octocodeai/octocode-native test:rust
yarn workspace @octocodeai/octocode-native build:dev
```

### Interface checks

Run:

```bash
yarn workspace octocode build:dev
yarn workspace octocode test
yarn workspace octocode-mcp build:dev
yarn workspace octocode-mcp test
yarn docs:verify
node skills/octocode-skills/scripts/skill-review.mjs skills
```

Run the applicable self-tests for research, scraping, Chrome DevTools, RFC generation, and `octocode-clasify` after their command fixtures change.

### Real CLI checks

Using the rebuilt CLI, verify:

```bash
node packages/octocode/out/octocode.js scheme clasify --view query --compact
node packages/octocode/out/octocode.js clasify --help
```

Also verify that `scheme semanticAssess` and direct `semanticAssess` invocation fail as unknown.

Without a classification key, a direct `clasify` call must return the existing actionable key setup diagnostic. With a key, execute:

- One Noul question.
- One Choice question.
- One Score question.
- One batched request.
- One request that returns `next.clasify`, followed by execution of that continuation unchanged.

### Real MCP checks

Run isolated MCP processes with and without the classification key:

- Key absent: the tool list omits `clasify` and `semanticAssess`.
- Key present: the tool list contains exactly one `clasify` entry.
- The MCP schema matches the CLI schema and native fingerprint.
- A provider-backed matrix returns correlated query, resource, question, and page results.
- The result contains no source body.

### Active-name audit

Search active source, tests, docs, manifests, and skills for:

- `semanticAssess`
- `SemanticAssess`
- `SEMANTIC_ASSESS`
- `semantic-assess`

Every remaining occurrence must be one of:

- A migration assertion that proves the old name is rejected.
- A historical record that accurately names the pre-rename interface.
- A historical-to-current mapping from the old name to `clasify`.

Unexplained active occurrences block release.

## Commit and release sequence

Use separate reviewable commits in dependency order:

1. **Core contract:** rename identity, schemas, continuation, tests, and core docs in `octocode-mcp-host`.
2. **Generated contract and native runtime:** regenerate from the clean core commit and rename native dispatch.
3. **Interfaces:** update CLI, MCP, manifests, and interface tests.
4. **First-party consumers:** update skills, scripts, benchmarks, and staged skill output.
5. **Documentation and migration:** update active references and preserve labeled history.
6. **Verification receipt:** record clean provenance, fingerprints, package checks, CLI/MCP smoke results, and the routing evaluation verdict.

Publish the matching core and Octocode releases together. Do not publish a mixed-name catalog/runtime pair.

## Acceptance checklist

- [ ] The intentional target spelling is `clasify` everywhere on active public surfaces.
- [ ] Core discovery exposes only `clasify`.
- [ ] Core exports `ClasifyInputSchema`, `ClasifyOutputSchema`, and their types.
- [ ] `next.clasify` validates and executes unchanged.
- [ ] Final native contract provenance is clean and points to the intended core commit.
- [ ] Native CLI and runtime dispatch only `clasify`.
- [ ] MCP omits or exposes `clasify` according to the existing provider-key gate.
- [ ] `semanticAssess` has no compatibility alias.
- [ ] No files under `packages/octocode-config/**` changed for this work.
- [ ] Classification environment variables and provider resolution are unchanged.
- [ ] Canonical skills and their staged CLI copies use `octocode-clasify` and `clasify`.
- [ ] Active docs and examples use `clasify`; historical evidence remains accurate and labeled.
- [ ] Core, native, CLI, MCP, skill, and documentation checks pass.
- [ ] Real CLI and MCP calls pass with matching contract fingerprints.
- [ ] The held-out routing evaluation records whether the name improved agent selection.
