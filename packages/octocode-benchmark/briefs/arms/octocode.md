## Arm brief: Octocode

Your only research tools are Octocode's, through one surface the round names: the MCP tools, or
the CLI (`octocode <tool> '{"queries":[…]}'`, and `{"cursor":"…"}` to continue a page). The MCP
server instructions and tool descriptions are your primer; on the CLI, `octocode schema <tool>`
prints a tool's input and counts as a research call.

Pick the tool by the question:

| Need | Tool |
|---|---|
| Find unknown files or lines in a repository | `ghSearchCode` (hits carry lines and the commit); `localSearch` on a clone |
| Read a known path, a region, a block or an outline | `ghGetFileContent` at a ref; `localFetch` locally |
| A tree, refs or languages | `ghStructure`; `structureSearch` locally |
| Find a repository | `ghSearchRepo` |
| Pull requests, issues, commits, a comparison | `ghSearchHistory`, then `ghGetHistoryItem` |
| A package version, its repository and commit | `artifactSearch` |
| Known vulnerabilities of a package or advisory | `securityCheck` |
| Code shape: declarations, calls, nesting | `astSearch` |
| Definition, references, callers: identity | `lspSearch`, at a line a search or read gave you |
| Many reads in one repository | `ghCloneRepo`, then the local tools |

The round states whether `clasify`, `graphBuild` and `graphQuery` are in scope. When `clasify` is
in scope, its billed provider usage is recorded beside your characters.

Take the leanest path: let a search hit answer; read a region (`matchString`, `ranges`) or an
outline rather than a whole file; read structured files (package.json, lockfiles) whole. Follow
`next` pages before concluding absence, or say what stays unread. An error names the field to
fix: fix it and retry. Read only.
