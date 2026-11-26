Load when attaching CodeQL SARIF or Joern flows and tracing `FLOWS_TO`.

# Data-flow evidence

## Data flow

Data flow arrives as evidence from the tools that compute it. Run a CodeQL path query (`@kind path-problem`, such as `js/sql-injection`) and attach its SARIF as `codeql-sarif`; the provider's `revision` must be the commit the log names in `versionControlProvenance` (an abbreviation of it agrees), or the import is refused. Relative URIs are relative to the analyzed checkout (`%SRCROOT%`): set `sourceRoot` when that is not the graph's root on this machine. Joern flows attach as `joern-flows`: a list of flows, each `{ "name": "<query>", "elements": [{ "nodeType", "tracked", "lineNumber", "columnNumber"?, "method", "file" }] }`.

Each step is a node `<file>#L<line>:<column>`, so two flows meet only at one program point. Its file starts the flow, and each step `FLOWS_TO` the next, carrying the rule id, the result's message and both steps' locations and messages (`details.from`, `details.to`, `role`: `source`, `step` or `sink`).

| Query | Answer |
| --- | --- |
| `trace src/api.js FLOWS_TO+` | Every step reachable from the flows that start in the file, each cited at the line it arrives at; `steps` hold the witnesses with the rule id |
| `trace src/db.js#L8:16 FLOWS_TO` | The next step from one program point |

The flows are the analyzer's: a path it did not report is not absence of flow, and results without `codeFlows` (plain problems) are only counted (`withoutFlow` in the provider record).
