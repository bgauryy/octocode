Load when deleting code, removing a dependency, or unifying duplicated policy or parallel implementations.

# Change discipline: removal and unification

## Delete junk and unify

- Use the removal proof below only inside the scope the [gates](change-discipline.md#gates) authorize: a review alone does not authorize deletion. `octocode-clean-agentic-code`, when installed, adds cleanup guidance; this skill owns boundary choice and verification.
- **Prove removal.** Start with AST or graph candidates found from explicit entry roots, resolve identity and consumers with LSP references excluding the declaration (`includeDeclaration: false`), follow the export and re-export chain, then check registries, configuration, scripts, generated consumers, persisted protocols, and external use; finish with tests and the build. Unused imports, stale wrappers, unreachable branches, debug leftovers, and obsolete dependencies are candidates, not automatic deletions. Keep useful diagnostics, compatibility required by a live contract, and tests that expose a failure.
- **Prove unification.** Compare semantics, invariants, defaults, errors, lifetime, and change pressure across every site. Similar syntax, names, or type shapes do not prove one responsibility. Preserve distinct domain identities and version boundaries when their meaning differs.
- **Choose one owner.** Put shared policy in the layer that owns it and direct callers there. Update the existing source of truth and regenerate derived forms; do not introduce a second canonical schema, a generic dumping ground, forwarding aliases, or a permanent parallel implementation.
- **Complete the migration.** Move consumers in verifiable slices, check each path, then remove redundant code, imports, dependencies, configuration, and links that are proven obsolete. Re-resolve references and inspect the resulting graph for new cycles or widened coupling.
- **Verify the result.** Run affected builds and contract/runtime checks, including previous critical paths. State which responsibility now has one owner, what was removed, and what evidence preserves behavior. Reduced line count alone is not proof of cleaner architecture or better efficiency.
