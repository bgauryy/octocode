# Octocode tools

The catalog has 17 tools. To see the ones available in your terminal, run `octocode schema`. Your MCP client's tool list shows which ones it can call. [Enable optional tools](CONFIGURATION.md#tool-availability-cli-and-mcp)

<a id="text-ast-graph-and-lsp-choose-the-evidence-you-need"></a>

## Choose the evidence you need

| I want to… | Start with |
|---|---|
| Find a file, phrase, code pattern or caller | [Local code tools](#local-code-tools-reference) |
| Read a file at a tag or find the change that introduced it | [GitHub tools](#github-tools-reference) |
| Find a package release or check it for known advisories | [Packages and advisories](#package-and-advisory-tools) |
| Find passages by meaning | [Classification](#semantic-assessment-reference) |

To see a request you can adapt, run `octocode schema <tool> --view variants`. Use `--select operation=<value>` to narrow a schema and `--view output` to look up a result field. [Read a paged answer](TOOL_DATA_CONTRACT.md#executable-continuations)

## Local code tools reference

### `structureSearch`

List a directory with `operation: "tree"`. Use `"files"` to filter by name, size or date. This query finds the largest file in the scanned workspace.

```json
{"queries":[{"operation":"files","entryType":"file","sort":{"by":"bytes"},"limit":1}]}
```

### `localSearch`

Search for literal text or a regular expression. Pass the returned path and line to `localFetch` to read the surrounding code.

```json
{"queries":[{"path":"README.md","matchString":["Octocode"]}]}
```

### `localFetch`

Read a known file. `ranges` uses inclusive line numbers starting at 1. Use `matchString` with `block: true` to read an enclosing declaration, or `view: "symbols"` to see an outline.

### `astSearch`

Find calls or declarations by syntax. A rule can use `$X` to match a single node. To try a rule on a snippet, pass `content` and `parser` together.

```json
{"queries":[{"operation":"match","content":"foo(1); bar(2)","parser":"typescript","rule":{"pattern":"foo($A)"}}]}
```

### `graphBuild` and `graphQuery`

Build a snapshot of a directory, then query the same path for importers, cycles, or the files a change could affect. To reuse a specific snapshot, pass the build's `graph` value. [Code graph examples](OCTOCODE_GRAPH.md)

### `lspSearch`

Find a symbol's definition, references or callers. Start with its file, name and line from a source read. `octocode lsp-server status <file>` shows which language server handles the file.

<a id="subtypes-and-implementations-without-an-answer-in-time"></a>
<a id="typescript-backends"></a>

A language-server result can resolve symbol identity. A syntax fallback gives candidate locations to check in source. [Language servers and custom configuration](OCTOCODE_CLI.md#language-servers)

#### Fetched code

Language servers are off by default for cloned repositories and materialized files. You can still search their text and syntax. [Trust for project code](SECURITY.md#trust-for-project-code)

## GitHub tools reference

### `ghSearchRepo`

Find a repository by keyword or list an owner's repositories. Use the returned `owner` and `repo` for later reads.

### `ghSearchCode`

Search GitHub's default-branch index. A requested `ref` checks indexed candidates at that revision. For files unique to another branch, browse that branch with `ghStructure`. Paths in `alsoAt` share the displayed matches.

### `ghStructure`

Browse a repository tree, list refs, or see its languages. Use `ref` to select a tag, branch or commit. Set `materialize: true` to download selected files for repeated local searches.

### `ghGetFileContent`

Read a known path in a repository. Pass the commit SHA from a search or tree result to read the same revision. As with `localFetch`, you can ask for a line range, declaration or outline.

### `ghSearchHistory`

Find commits, pull requests and issues. This request finds commits that changed the README in a public repository.

```json
{"queries":[{"operation":"commit","owner":"octocat","repo":"Hello-World","path":"README"}]}
```

### `ghGetHistoryItem`

Read a commit, pull request or issue, or compare two refs. Start with a summary and follow the returned call for the patch, files or discussion you need. Use `octocode schema ghGetHistoryItem --select operation=commit` for the sections available on a commit.

### `ghCloneRepo`

Create a checkout for repeated local searches. Use the returned `location.path` and keep its commit SHA with your findings. Use `paths` for a sparse checkout. Cloning requires Git and persistent storage. [Storage settings](STORAGE_AND_LIMITS.md#cache-storage-and-lifecycle)

## Package and advisory tools

### `artifactSearch`

Resolve the version from your lockfile to registry metadata and its source repository. The supported ecosystems are npm, PyPI, crates, Maven, NuGet, Go, Packagist and RubyGems. Use `view: "details"` for dependencies and license information.

```json
{"queries":[{"operation":"resolve","ecosystem":"npm","packageName":"zod","version":"3.22.4"}]}
```

When the result includes a `ref`, read it with its `verification` field. `registryRef` means the registry supplied the revision. For a GitHub source, pass its owner, repository and ref to `ghGetFileContent`. For a source hosted elsewhere, use its repository URL.

### `securityCheck`

Check a specific package version, repository, commit, advisory ID or package URL for known advisories. The summary reports affected versions and available upgrades when established. Details include the advisory sources and version ranges. An empty result means no known advisory matched the query. [What gets sent](SECURITY.md#advisory-egress)

## Semantic assessment reference

### `clasify`

Ask a question about a file, image, page or set of search results. Fresh evaluations use the configured provider. Follow `next.read` to inspect the supporting source. [Setup, budgets and examples](OCTOCODE_CLASIFY.md)

### Keep rows by meaning

Give `clasify` a tool query as its source and describe what you want to find with `predicates`. [Filter search hits](OCTOCODE_CLASIFY.md#filter-search-hits)
