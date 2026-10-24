# The Octocode protocol

Octocode exposes text, file structure, syntax, semantic locations, history, packages and classification through one tool contract. Each tool supplies evidence and typed next steps so an agent can move from discovery to an exact source claim.

## One contract for every surface

Core authors schemas, descriptions, defaults and limits. Config generates the embedded contract and types. Native validates and executes every tool; CLI and MCP launchers carry no tool logic. Tool names are stable across surfaces. [Tool reference](OCTOCODE_TOOLS.md).

New calls contain `queries` and an optional `responseChars`; resumed calls contain only an emitted `cursor`. Shared concepts use the same types: matcher arrays, before/after context, inclusive line-range objects, typed filter bounds, and by/order sorting. Unknown fields and incompatible variants fail before execution. There is no alternate shorthand language or compatibility conversion.

## Research loop

1. Choose the evidence dimension and a bounded scope.
2. Discover candidates, then read source or verify semantic locations.
3. Follow the continuations needed for the claim and inspect coverage disclosures.
4. Cite the evidence; identify what remains unknown.

Batch independent queries of one tool. Build dependent calls from observed identities. [Routing and examples](OCTOCODE_WORKFLOWS.md).

## Evidence and views

Default outputs carry evidence and facts needed to understand its scope. Source reads preserve line coordinates. Compact and symbols views are explicit transformations. A match or declaration owns its evidence fields; callers need no shared-field inheritance or cross-row shorthand to interpret a page.

Search-index gaps, ignored files, provider limits, classification uncertainty and unavailable LSP capabilities remain visible. Source redaction happens before projection. A smaller response may remove redundant metadata, never silently lose evidence.

## Continuations and related work

A `continue` call carries a saved cursor and reaches remaining or recoverable evidence. A `related` call carries public query input and starts optional new work. Hints are prose only. Whole-response pages precede tool-level pages; independent collections each have their own continuations. Saved work is bound to tool, configuration and access context. [Data contract](TOOL_DATA_CONTRACT.md).

## Precision and classification

Text search finds spelling; AST finds syntax; LSP verifies semantic locations using a real server. Graph edges and syntax matches need confirmation before claims about callers or unused code.

Inline `classification` selects candidates on four readers/search tools with one predicates/context policy. Public clasify evaluates content or unread sources, assesses sections, and discovers models. Both preserve provider usage and source provenance while keeping unassessed evidence distinct from negative judgments. [Classification guide](OCTOCODE_CLASIFY.md).

## Runtime boundaries

Local reads enforce the configured workspace and path policy. Native sanitizes input and output and binds saved evidence to access policy. GitHub and registry transports respect provider limits, report terminal caps and cache eligible data. Memory storage cannot resume after its runtime closes; persistent storage supports eligible later-process continuations.

Some semantic operations need installed language servers. Cold startup, incomplete indexes and provider caps constrain what a tool can prove. Measure quality with actual evidence walks and provider effects, not schema snapshots or output length alone. [Tool quality](../skills-dev/octocode-dev/docs/TOOL_QUALITY.md) defines acceptance; [benchmarks](../packages/octocode-benchmark/README.md) measure broader outcomes.
