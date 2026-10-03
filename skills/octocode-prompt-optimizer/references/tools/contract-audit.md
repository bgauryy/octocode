# Cross-tool contract audit

Load when a server exposes more than one tool, or after editing any server instruction, description, or schema. Audit the set, not the tool: each tool can be correct alone while the set contradicts itself. If a human cannot say which tool applies, an agent cannot either ([Anthropic context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)).

## Sweep

1. **Inventory** — every tool: name, one-line job, and the layer that states that job. A tool whose job is stated in no layer is unroutable.
2. **Selection overlap** — for each pair whose jobs sound close, write the sentence that decides between them. If you cannot, merge the tools, rename them, or add the deciding condition to both descriptions.
3. **Shared descriptors** — compare fields reused across tools and operations: names, types, units, defaults, requiredness, and scope. Reuse one definition for equivalent semantics; document legitimate differences instead of forcing unrelated fields into one contract.
4. **Workflow closure** — every tool named in a `Next` exists, and the prerequisite that description promises matches that tool's required fields.
5. **Boundary consistency** — approval, trust, and completeness claims in a description or result must not exceed what the server instructions allow.

## Contradiction classes

| Class | Symptom | Repair |
|---|---|---|
| Split owner | the same rule in server instructions and a description, worded differently | keep one owner; delete the copy |
| Name drift | `path` in one tool, `directory` in another for the same input; `limit` versus `maxResults` | one canonical name from the shared definition |
| Type drift | a field required in one tool and optional in another with no stated reason | inspect each operation's prerequisites; align equivalent fields or document the difference |
| Semantic drift | `page` is 1-based in one tool and a byte offset in another | one meaning per name; rename the other (`page` versus `charOffset` versus `cursor`) |
| Enum drift | guidance suggests a value unsupported by the selected operation | validate against that operation's schema; share only equivalent enums |
| Phantom next | a description points to a tool, field, or mode that does not exist | fix the pointer or delete the claim |
| Silent-failure drift | one tool says an empty result proves absence while another says it does not | state it once in the server instructions |
| Authority drift | a description implies a mutation or scope the server forbids | the server instructions win; narrow the description |

Name drift and semantic drift are the expensive pair: an agent that learned a field name in one tool reuses it in the next, and a renamed-but-similar field fails silently instead of erroring.

## Output

```markdown
## Contract Audit
| Tool | Job | Overlaps with | Finding | Class | Repair |
|---|---|---|---|---|---|
| <tool> | <one line> | <tool or none> | <evidence> | <class or none> | <bounded change> |

Shared descriptors: <field -> tools, single definition yes/no>
Unresolved: <contradiction needing an owner decision>
```

Report only the pairs and fields you compared. An unexamined tool is a gap, not a pass.

Sources: [Anthropic, writing tools](https://www.anthropic.com/engineering/writing-tools-for-agents); [MCP Tools spec](https://modelcontextprotocol.io/specification/2026-07-28/server/tools).

Next: repair in the owning layer `tool-contracts.md`; wire findings `mcp-wire-contract.md`; cross-app semantics `../agents/cross-app-contracts.md`; prove selection with `octocode-eval-benchmark`; record repairs in `../flow/fix.md`.
