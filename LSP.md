# LSP: every language, editor and agent

Checked 8 Nov 2026.

The Language Server Protocol (LSP) is how your editor and your AI agent get real code intelligence: go to definition, find references, safe rename, live errors. Install one server per language and every tool can share it.

## The idea in one minute

LSP splits code intelligence into two parts. The server is the brain: one per language, it parses and type-checks your project. The client is anything that speaks the protocol: your editor, or an AI agent through MCP or a plugin. They talk JSON-RPC, usually over stdio. Install the server once per language, and every client you use gets the same answers.

## Install and check, per language

A check prints a version or path when the server is installed. No output or "command not found" means install it.

### TypeScript / JavaScript - `typescript-language-server`
Install:
```sh
npm install -g typescript-language-server typescript
```
Check:
```sh
typescript-language-server --version
```

### Python - `pyright`
Install:
```sh
npm install -g pyright   (or: pip install pyright)
```
Check:
```sh
pyright --version
```

### Go - `gopls`
Install:
```sh
go install golang.org/x/tools/gopls@latest
```
Check:
```sh
gopls version
```

### Rust - `rust-analyzer`
Install:
```sh
rustup component add rust-analyzer rust-src
```
Check:
```sh
rust-analyzer --version
```

### C / C++ - `clangd`
Install:
```sh
ships with LLVM: apt install clangd  /  brew install llvm
```
Check:
```sh
clangd --version
```

### C# - `csharp-ls`
Install:
```sh
dotnet tool install -g csharp-ls
```
Check:
```sh
dotnet tool list -g
```

### Java - `jdtls (Eclipse JDT Language Server)`
Install:
```sh
brew install jdtls   (or download from eclipse.dev)
```
Check:
```sh
jdtls --version
```

### Ruby - `ruby-lsp`
Install:
```sh
gem install ruby-lsp
```
Check:
```sh
ruby-lsp --version
```

### PHP - `intelephense`
Install:
```sh
npm install -g intelephense
```
Check:
```sh
intelephense --version
```

### Lua - `lua-language-server`
Install:
```sh
brew install lua-language-server   (or GitHub release)
```
Check:
```sh
lua-language-server --version
```

### Swift - `sourcekit-lsp`
Install:
```sh
ships with the Xcode / Swift toolchain
```
Check:
```sh
xcrun --find sourcekit-lsp
```

### Bash - `bash-language-server`
Install:
```sh
npm install -g bash-language-server
```
Check:
```sh
bash-language-server --version
```

## Hook it into your editor

- **VS Code.** Install the language extension from the marketplace (rust-analyzer, Python, clangd and friends). Most extensions download or bundle their own server, so nothing needs to be on PATH.
- **Cursor.** A VS Code fork: the same extensions work and the same rules apply. Install the extension, open the project, done.
- **Neovim.** The server binary must be on PATH, then wire it up with nvim-lspconfig (lspconfig.<server>.setup{}) or let Mason install servers for you. :LspInfo shows what attached.
- **Zed.** For the big languages Zed downloads and starts the right server automatically. Open the project and it just works.
- **Helix.** Servers are preconfigured for most languages, but the binary must be on PATH. Check with: hx --health <language>.
- **Emacs.** eglot ships with Emacs 29+. Install the server binary, open a file, run M-x eglot. lsp-mode is the heavier alternative.
- **JetBrains IDEs.** Their own engines power the supported languages natively. For the rest there is LSP support (built-in LSP API or the LSP4IJ plugin), and rust-analyzer can be switched on in RustRover settings.
- **Sublime Text.** Install the LSP package plus the helper for your language (LSP-pyright, LSP-rust-analyzer...). The helper downloads the server for you.

## Worked example: Rust + rust-analyzer

Check what is installed:
```sh
rust-analyzer --version
rustup which rust-analyzer
rustup component list --installed | grep -E 'rust-analyzer|rust-src'
```

Install what is missing (rust-src is the std library source, needed for go to definition into std):
```sh
rustup component add rust-analyzer rust-src
```

Two settings worth adding (VS Code settings.json):
```json
"rust-analyzer.check.command": "clippy",   // lint as you type instead of plain cargo check
"rust-analyzer.cargo.features": "all"      // fixes dead completions inside #[cfg(feature)] blocks
```

Verify it works: open a project file, hover a type for docs, Ctrl+Click to jump to definition, inlay hints show inferred types. CLI sanity check: `rust-analyzer diagnostics .` batch-checks the project like the editor would.

## Gotchas

- Open the project root (the folder with Cargo.toml, go.mod, tsconfig.json), not a lone file. Single-file mode is limited or dead.
- A GUI app may not inherit your shell PATH. If the editor "can't find" the server, launch the editor from a terminal once, or install the server through the editor's own mechanism.
- C/C++: clangd needs compile_commands.json to understand your build (CMAKE_EXPORT_COMPILE_COMMANDS=ON, or the bear tool).
- First open is slow: the server indexes the workspace. Wait for the status bar to settle before judging it.
- Old pinned toolchains break new servers (and vice versa). If rust-analyzer complains about std sources, update the toolchain or point the server at stable.
- Two clients, one project: your editor and your agent can each run their own server process on the same codebase. That is normal.

## Give it to your agent

An agent that only reads and rewrites text guesses at symbols. With LSP it gets the same facts your editor has: where a symbol is defined, everywhere it is used, and the type errors an edit just introduced.

### Claude Code: official LSP plugins

Anthropic ships one code intelligence plugin per language. Install the server binary first (it must be on the PATH of the shell you start claude from), then inside Claude Code:

```
/plugin install typescript-lsp@claude-plugins-official
```

Swap in pyright-lsp, gopls-lsp, rust-analyzer-lsp, clangd-lsp, csharp-ls (csharp-lsp), jdtls-lsp, intelephense (php-lsp) and so on. Claude then sees live diagnostics after its own edits and navigates by symbol instead of text search. Terminal sessions only: cloud sessions do not start plugin language servers.

### Any MCP agent: Serena

Serena wraps language servers as MCP tools (find symbol, find references, rename, insert after symbol) across 30+ languages. It plugs into Claude Code, Codex, Cursor, Gemini CLI, JetBrains assistants and any other MCP client. One setup gives your whole agent fleet IDE-level navigation.

### Octocode

Your own MCP server exposes `lspSearch`: symbol-level proof through LSP, given a path, symbolName and line (from a `localSearch` hit). Same protocol, packaged for GitHub and local code research.

## The decision flow, in Octocode order

One algorithm decides which brain answers, every call. Each level degrades gracefully into the next, and nothing ever returns empty.

Implemented in `lspSearch` as the levels `ide`, `server`, `syntax` and `text`, reported per row in `lsp.level` with `lsp.fellBackFrom` on a fallback: see [the lspSearch reference](docs/OCTOCODE_TOOLS.md#lspsearch).

1. **Detect the language.** File extension first, then confirm with project markers: Cargo.toml, tsconfig.json, go.mod, package.json.
2. **Check the process: inside an IDE?** Env vars, lock file (see the IDE section). Yes: reuse the IDE's engine, providers and diagnostics. Bridge dead or refuses? Drop to step 3, never straight to tree-sitter.
3. **Check the CLI: server on PATH?** Run the check command for the detected language. Yes: start the server and ask it. No: fall through.
4. **Tree-sitter: does a grammar exist?** Yes: parse, query the outline, stitch a repo map. No types, no cross-file resolution.
5. **All missing: return a hint, not nothing.** Point the agent at Octocode's local search tools so the call still moves the work forward.

Operational rules:

- Cache the result per project root. Detection, IDE presence and server paths do not change call to call; re-probe only when the toolchain or markers change.
- A failed IDE bridge drops to the CLI step, not straight to tree-sitter. One broken rung never skips a better one.
- Say which level answered. A tree-sitter outline and a server-proven definition look identical in a response but carry different confidence, and the agent plans its next move on that.

A response when every level misses looks like this:

```json
{
  "level": "local-search",
  "hint": "No language server, IDE bridge, or tree-sitter grammar is available for this file. Use Octocode local search instead: localSearchCode(path, searchText) for text/regex matches, localViewStructure(path) for the directory tree, localFindFiles(path, ...) to locate files, then localGetFileContent(path, ...) for the exact region. Matches are text anchors, not symbol-proven facts; verify by reading the code."
}
```

Octocode tie-in: localSearchCode(mode:"structural") already runs AST shape queries, so steps 4 and 5 share one tool family, and a localSearchCode hit's line number is the lineHint lspGetSemantics needs when a server does exist.

## No server? Fall back to tree-sitter

Some languages have no LSP server, or the server will not run in your setup. The next best brain is tree-sitter: a parser generator and incremental parsing library. It builds a concrete syntax tree per file, keeps it updated as the file is edited, and stays useful even when the code has syntax errors, which is exactly the state code is in while an agent edits it.

What you keep:
- Symbol outline: every function, class and method with names and line ranges, extracted with S-expression queries over the syntax tree.
- Syntax-aware search: match nodes by structure (a call whose object is a member expression) instead of regex over text.
- Fast and local: one grammar per language, no project build, no index, works on a single file.

What you lose:
- Types and hover docs: tree-sitter has no type system.
- Cross-file resolution: it parses one file at a time, so "who calls this" degrades to name matching.
- Compiler-grade diagnostics and call hierarchy: no semantic analysis, no build integration.

The playbook:

1. Install the grammar for the language. The tree-sitter org and its community keep one per language, including many with no LSP server.
2. Parse with the CLI or a binding (Rust crate, py-tree-sitter, web-tree-sitter for Node):
```sh
tree-sitter parse src/**/*.py
tree-sitter query outline.scm src/**/*.py
```
3. Write queries that capture definition nodes. Node names come from the grammar; this Python example captures every function and its name:
```
(function_definition
  name: (identifier) @name) @function
```
4. Stitch the outlines into a repo map: the file list plus key symbols per file. Aider does exactly this and feeds the map to the model.
5. References degrade to "same name, nearest definition" until a real server appears. Say so in the answer instead of pretending.

Order of preference: IDE engine if one is running, then the LSP server, then tree-sitter, then local search.

## Inside an IDE? Borrow its engine

If your agent runs in an IDE's integrated terminal, the IDE already has language intelligence warmed up: extensions, indexes, diagnostics. Detect that, and reuse it instead of starting a second server.

Detect it:
```sh
echo "$TERM_PROGRAM $TERMINAL_EMULATOR"
env | grep -E '^VSCODE_|^CLAUDECODE='
```

- VS Code integrated terminal: TERM_PROGRAM=vscode, plus TERM_PROGRAM_VERSION and VSCODE_* variables, set in VS Code's own terminal environment code. Cursor is a VS Code fork and can report the same markers, so read the value as "a VS Code-family editor".
- JetBrains integrated terminal: TERMINAL_EMULATOR=JetBrains-JediTerm.
- Inside a Claude Code subprocess: CLAUDECODE=1, set for its Bash tool, hooks, tmux sessions and MCP subprocesses; IDE extensions also set it in their integrated terminals.
- Env vars say which IDE hosts your terminal, not which extensions it has installed. Keep the tree-sitter fallback ready.

### VS Code / Cursor: run the installed providers

The extension API ships built-in commands that execute the language providers already installed in the editor: `vscode.executeDefinitionProvider`, `vscode.executeReferenceProvider`, `vscode.executeDocumentSymbolProvider`, `vscode.executeWorkspaceSymbolProvider`, `vscode.executeHoverProvider` and more. A small bridge extension can call them through `vscode.commands.executeCommand(...)` and expose the results to your agent, for example as an MCP server. The answers are exactly what the IDE's own extensions compute, with no second server to install or index.

### Claude Code: connect to the IDE plugin

Start `claude` from the IDE's integrated terminal and it auto-connects to the IDE plugin's local MCP server; from an external terminal, run `/ide`. Once connected, Claude can call `mcp__ide__getDiagnostics` for the IDE's own errors and warnings on a file, and your selection and active file travel with each prompt as context. The plugin writes an auth token to a lock file at `~/.claude/ide/.lock`; the transport is unencrypted ws:// on an ephemeral loopback port.

### JetBrains 2025.2+: the built-in MCP Server

The IDE ships an integrated MCP Server (Settings | AI | MCP Server) with tools like `get_file_problems` (IntelliJ inspections on a file), `analyze_calls` (call hierarchy), `search_symbol` and `build_project`. Claude Code, Codex and other MCP clients connect to it, and the IDE can write the client configuration for them.

IDE engines reuse whatever server or inspection the IDE runs, so the agent inherits the IDE's config for free: one brain, no duplicate index.

## Massive inputs: huge files, huge repos

LSP's cost is up front: the server indexes the project before it answers. On a Chromium-scale codebase clangd's global index can take hours and gigabytes of RAM. Giant single files are cheap (hover and document symbols read one file); what scans the world is references, workspace symbols and diagnostics.

The knobs, per server:

- **rust-analyzer.** files.exclude drops directories from analysis; lru.capacity caps how many syntax trees stay in memory (default 128); numThreads and cachePriming.numThreads control the CPU budget.
- **clangd.** The background index writes .idx files to .cache/clangd/index/ next to compile_commands.json and reuses them across restarts. It can be disabled on huge projects to save CPU and battery, and a remote index can serve Chromium-scale codebases from another machine.
- **TypeScript (tsserver).** VS Code caps its memory with typescript.tsserver.maxTsServerMemory. On a monorepo, open the package, not the repo: each package root gets its own project.
- **pyright.** exclude prunes paths (node_modules, __pycache__ and virtualenvs are excluded by default); include and executionEnvironments scope what gets analyzed.
- **Any server.** Big answers stream: LSP reports partial results and progress over $/progress, and a cancelled request can still return its partial result.

When to skip LSP: if the question is "where does X appear", ripgrep-scale text search answers in milliseconds where a cold server needs minutes of indexing. That is Octocode's documented order: localSearchCode first, lspGetSemantics after you hold a real line number. And when even the outline is too big, rank it: Aider compresses its repo map with a dependency-graph ranking so only the most relevant parts spend tokens.

## Cache locally, answer fast

Every level of the decision flow produces cacheable facts. Cache them, and the second call is free.

- **Cache the probe, including the negatives.** Language, project root, IDE presence, server path, and "no server for this language" all belong in a per-project cache with a TTL, so a fresh install still gets noticed. This is the decision flow's operational rule, made concrete.
- **Cache per file, keyed by identity.** Outlines and symbol tables keyed by path + mtime + size (cheap) or content hash (exact). Invalidate on write with a watcher (rust-analyzer's files.watcher does this for config), or re-check mtime on read with a TTL as the safety net.
- **Keep the server warm.** A running server with a built index answers in milliseconds; a cold one pays the full index cost. Reuse one process per project instead of starting one per query.
- **Put the cache where the convention is.** A .cache directory inside the project (clangd) or a user-level home (Octocode reads config from ~/.octocode).

A cache entry might look like:

```json
{
  "project": { "root": "/repo/pkg", "marker": "package.json", "language": "typescript" },
  "server": { "path": "/usr/local/bin/typescript-language-server", "ok": true, "checkedAt": 1731628800, "ttlHours": 24 },
  "files": {
    "src/index.ts": { "mtime": 1731628701, "size": 48211, "outline": "..." }
  }
}
```

Octocode tie-in: lspGetSemantics already pages big results (page / itemsPerPage, max 100) and reports evidence.completeness with hints when an answer is partial, so "massive" degrades into "paged", never into silent truncation.

## Sources

- [Language Server Protocol, the official spec and server list](https://microsoft.github.io/language-server-protocol/implementors/servers/)
- [rust-analyzer manual: installation](https://rust-analyzer.github.io/book/installation.html)
- [rust-analyzer manual: the binary and rustup components](https://rust-analyzer.github.io/book/rust_analyzer_binary.html)
- [rust-analyzer manual: configuration](https://rust-analyzer.github.io/book/configuration.html)
- [Claude Code docs: code intelligence (LSP) plugins](https://code.claude.com/docs/en/plugins/code-intelligence)
- [Serena: LSP-backed MCP toolkit for coding agents](https://github.com/oraios/serena)
- [nvim-lspconfig: Neovim LSP configurations](https://github.com/neovim/nvim-lspconfig)
- [Octocode: lspSearch reference](docs/OCTOCODE_TOOLS.md#lspsearch)
- [Octocode: local tools reference (localSearchCode and friends)](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md)
- [Octocode: tool quality notes](skills-dev/octocode-dev/docs/TOOL_QUALITY.md)
- [clangd: the index (background index, on-disk .idx cache, remote index)](https://clangd.llvm.org/design/indexing.html)
- [pyright: configuration (exclude, executionEnvironments)](https://github.com/microsoft/pyright/blob/main/docs/configuration.md)
- [VS Code source: tsserver configuration (maxTsServerMemory)](https://github.com/microsoft/vscode/blob/da36e0eba18babc8eb3e18f5a9b30e1dd7e083d4/extensions/typescript-language-features/src/configuration/configuration.ts)
- [tree-sitter: parser generator and incremental parsing library](https://tree-sitter.github.io/tree-sitter/)
- [tree-sitter query syntax (S-expression patterns)](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html)
- [tree-sitter CLI: parse and query commands](https://tree-sitter.github.io/tree-sitter/cli/parse.html)
- [Aider: building a repository map with tree-sitter](https://aider.chat/2023/10/22/repomap.html)
- [VS Code API: built-in commands (vscode.execute* providers)](https://code.visualstudio.com/api/references/commands)
- [Claude Code environment variables (CLAUDECODE, CLAUDE_CODE_AUTO_CONNECT_IDE)](https://code.claude.com/docs/en/env-vars)
- [Claude Code JetBrains integration (/ide, getDiagnostics, lock file)](https://code.claude.com/docs/en/jetbrains)
- [JetBrains: built-in MCP Server (2025.2+)](https://www.jetbrains.com/help/idea/mcp-server.html)
- [VS Code source: terminal environment sets TERM_PROGRAM](https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/terminal/common/terminalEnvironment.ts)
- [JetBrains jediterm: TERMINAL_EMULATOR=JetBrains-JediTerm](https://github.com/JetBrains/jediterm/issues/253)
