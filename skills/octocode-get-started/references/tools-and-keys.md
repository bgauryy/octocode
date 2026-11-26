Load when choosing a first tool call, enabling an optional tool, or adding a provider key.

# Tools and keys

## Choose a tool

| Task | Start here |
|---|---|
| Read local code | Find paths with `structureSearch` or `localSearch` and read with `localFetch`. Use `astSearch` for syntax and `lspSearch` for callers or definitions. |
| Read GitHub code or history | Locate the repository with `ghSearchRepo`, find files with `ghStructure` or `ghSearchCode`, then use `ghGetFileContent`. For history, use `ghSearchHistory` then `ghGetHistoryItem`. |
| Check a dependency | Take the version from the lockfile, look it up with `artifactSearch` and check known advisories with `securityCheck`. |
| Explore dependencies locally | `ghCloneRepo` creates a checkout. `graphBuild` then `graphQuery` give dependency paths, cycles and change impact within an available graph scope; both come from the separate graph package (`npx -y @octocodeai/octocode-graph graphBuild`), not from `octocode`. |
| Filter results by meaning | `clasify` can use local or GitHub reads, images, public pages and supported search results. A fresh evaluation sends evidence to the configured provider and can incur charges. |

## Optional tools

| To enable | Command |
|---|---|
| Graphs in MCP | Add the separate graph server as its own stdio entry: `npx -y @octocodeai/octocode-graph mcp` (Node.js 24.15 or later). Its launcher turns on `graphBuild`, `graphQuery` and the local tools; `local.beta` on the `octocode` server lists no graph tool. |
| Cloning in MCP | `npx -y octocode config set tools.enableClone true` with local tools enabled (requires Git and persistent storage) |
| Advisory checks in MCP | `npx -y octocode config set tools.enableSecurity true` |
| Classification in CLI and MCP | `npx -y octocode config set OCTOCODE_TYPESAFE_KEY --stdin` |

## Add and configure keys

Enter secrets on stdin and restart MCP after changing its environment or settings. For classification, `npx -y octocode config check OCTOCODE_TYPESAFE_KEY` exits 0 when a key is present; it does not prove access. Only a provider call verifies access, and it may incur a charge. For a custom gateway, check `classification.apiHost` before sending evidence. [Classification setup](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLASIFY.md)

Skills can have their own optional providers. Use `npx -y octocode skill info <name>` to check the skill’s requirements before adding keys.
