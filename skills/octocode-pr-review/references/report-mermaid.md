Load when choosing a specialized diagram. Author a `mermaid` fence; the shared viewer owns configuration, colors and controls. Target its pinned Mermaid **12.1.0**, not unversioned examples. Start with the smallest diagram that answers the reader's question.

# Mermaid chooser

| Reader needs | Syntax keyword | Selection rule |
|---|---|---|
| Decisions and dependencies | `flowchart` | Default for small directed explanations; label branches, group stages. |
| Dense layered dependencies | `flowchart-elk` | Try ELK when ordinary flowchart layout becomes tangled; split if labels remain tiny. |
| Agent tasks and tools | `agentflow-beta` | Explicit task, tool, input and decision shapes within agent flows. |
| Responsibility handoffs | `swimlane-beta` | Separate actors or teams into lanes. |
| Messages over time | `sequenceDiagram` | Show ordering, alternatives and asynchronous interactions. |
| Lifecycle transitions | `stateDiagram-v2` | States, events, guards and terminal outcomes. |
| Domain events | `eventmodeling` | Commands, events and read models in a business process. |
| Data model | `erDiagram` | Entities and cardinality. |
| Type structure | `classDiagram` | Classes, members, inheritance and composition. |
| Service topology | `architecture-beta` | Services, infrastructure groups and connections; built-in icons need no registration. |
| Architecture abstraction | `C4Context`, `C4Container`, `C4Component` | Choose one level per diagram; explain boundaries. |
| Architecture behavior or hosting | `C4Dynamic`, `C4Deployment` | Numbered interactions / runtime deployment topology. |
| Deliberate spatial arrangement | `block-beta` | Block placement matters more than automatic graph layout. |
| Work planning | `gantt`, `kanban` | Dates and dependencies / work stages. |
| Chronology or experience | `timeline`, `journey` | Events / user stages and experience. |
| Branch history | `gitGraph` | Explain branch and merge behavior; not a substitute for actual Git evidence. |
| Hierarchy | `mindmap`, `treeView-beta` | Concept exploration / directory-like structure. |
| Parts of a whole | `pie`, `treemap` | Few proportions / nested proportions. State units and source. |
| Trends or comparisons | `xychart-beta`, `radar-beta` | Ordered bars/lines / profiles sharing comparable scales. Prefer bars for precise comparisons. |
| Two-axis prioritization | `quadrantChart` | Explicitly define both axes; placements require evidence or an estimate label. |
| Weighted movement | `sankey-beta` | Conserved flows with named units; avoid invented weights. |
| Set overlap | `venn-beta` | Membership and intersections, not causal relationships. |
| Strategic mapping | `wardley-beta`, `cynefin-beta` | Value-chain evolution / decision context; explain subjective placement. |
| Root-cause hypotheses | `ishikawa-beta` | Group possible causes; a branch is not proof of causality. |
| User goals | `usecase-beta` | Actors, system boundary and use cases. |
| Traceability | `requirementDiagram` | Requirements, relationships and verification. |
| Binary layout | `packet-beta` | Bit positions and protocol fields. |
| Grammar | `railroad-beta`, `railroad-ebnf-beta`, `railroad-abnf-beta`, `railroad-peg-beta` | Choose the dialect matching the actual grammar. |
| Runtime diagnostics | `info` | Verify Mermaid version; omit from normal reports. |

## Author and verify

- Get only the selected syntax from the [versioned source documentation](https://github.com/mermaid-js/mermaid/tree/mermaid%4012.1.0/packages/mermaid/src/docs/syntax); beta and experimental families can change. Do not load this entire upstream catalog into context.
- Use meaningful labels, a nearby takeaway and source links; use `accTitle`/`accDescr` where the selected family supports them. Do not rely on color alone or add decorative animation.
- Split large diagrams by question or abstraction. Expand/zoom helps inspection but does not rescue an unreadable overview. Test long labels and narrow screens.
- ZenUML requires a separate integration absent from this viewer; use `sequenceDiagram`. Custom icon packs, callbacks and external plugins are not registered by authored Markdown.
- Run **Render check**, inspect the actual SVG and labels, and disclose failures. A successful example does not validate every grammar feature, accessibility behavior, export or large-graph performance.
- Use the [graph/chart widgets](report-widgets.md) when readers need network search, interactive legends or exact-value tables; use Mermaid for authored explanations. Keep content in Markdown rather than writing viewer code.
