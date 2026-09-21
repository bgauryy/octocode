# Octocode Tool Sanity Benchmark

A runnable, **per-tool** checkbox suite that exercises **every schema variant and
advanced option** of each public tool — not just one happy path. One section per
tool: its full schema (all operations, enums, advanced flags), a core task, an
**Advanced coverage** checklist (minification, matchString, line ranges,
pagination, all ecosystem types, every graph analysis, all LSP operations, …),
and any continuation-contract **regression** sub-checks. An agent (or human)
works top to bottom and ticks each box. All boxes green ⇒ the surface is sound
on **native CLI**, **node CLI**, and **MCP (stdio)**.

This is a *sanity/coverage* gate (does every path answer correctly at all), not a
performance benchmark — for comparative measurement see `compare/terra-v3/`.

> **Note on R# items:** `R1`…`R4` are **regression checks, not tools**. Each lives
> under the tool it guards (e.g. R1 is `astSearch`'s deadCode continuation). They
> reproduce specific `outputContractViolation` bugs that were fixed, so a
> regression is caught immediately.

---

## 0. Setup

**0.1 — Build all three surfaces from current source:**

```bash
cd <repo>/packages/octocode-native
# If octocode-core (sibling repo) changed, build + relink first:
#   (cd ../../../octocode-mcp-host/packages/octocode-core && yarn build) && (cd ../.. && yarn install)
yarn build:darwin-arm64   # your platform: CLI binary + platform addon
yarn build:addon          # REQUIRED for MCP — rebuilds the ROOT addon js/runtime.cjs loads
cd ../.. && yarn workspace octocode-mcp build:dev && yarn workspace octocode build:dev
```

> **Gotcha (load-bearing):** MCP loads the **root** `octocode-native.<platform>.node`
> via `js/runtime.cjs`, not the `npm/<platform>/` copy. `build:darwin-arm64` alone
> leaves MCP on the old addon — always run `build:addon` too.

**0.2 — Environment:**

```bash
export GITHUB_TOKEN=$(gh auth token)                     # gh* + artifact live checks
export PATH="<repo>/node_modules/.bin:$PATH"             # typescript-language-server for lspSearch
export BIN=<repo>/packages/octocode-native/npm/darwin-arm64/octocode   # native CLI
export NODECLI=<repo>/packages/octocode/out/octocode.js                # node CLI
export MCP=<repo>/packages/octocode-mcp/dist/index.js                  # MCP server
export ENABLE_CLONE=true                                # opt in for the ghCloneRepo checks
```

**0.3 — Fixture** (local-tool tasks; path policy is scoped to the process cwd):

```bash
export FIX=/tmp/octocode-sanity && rm -rf "$FIX" && mkdir -p "$FIX/src"
printf 'export function greet(name: string): string {\n  return `hello ${name}`;\n}\nexport function shout(name: string): string {\n  return greet(name).toUpperCase();\n}\n' > "$FIX/src/util.ts"
printf 'import { greet, shout } from "./util";\n\nfunction main(): void {\n  const a = greet("world");\n  const b = shout("world");\n  console.log(a, b);\n}\n\nmain();\n' > "$FIX/src/index.ts"
printf '{ "name": "sanity-fixture", "version": "1.0.0", "type": "module" }\n' > "$FIX/package.json"
cd "$FIX"
```

**0.4 — Invocation & PASS.** Use `"$BIN" <name> '<json>'` on the native CLI,
`node "$NODECLI" <name> '<json>'` on the node CLI, or MCP
`tools/call {name, arguments:<input>}`. Every query needs a `reasoning` string.
`semanticAssess` accepts one complete SemanticQuery or
`{queries:[<SemanticQuery>]}`. **PASS** = structured result with **no**
`outputContractViolation`, no `Invalid arguments`/`invalidInput`, no crash, and
the stated content check holds.

**0.5 — Availability and automated runners.** `scheme` always discovers 12
tools. With clone at its default disabled setting and no `OCTOCODE_CLASSIFICATION_API`, 10
are available. MCP registers only available tools, so it omits
`semanticAssess`; the CLI keeps `semanticAssess` and its schema discoverable and
reports the missing key when called. This setup opts into clone. Section 13 runs
the 11 non-provider tools across three surfaces, section 12 checks the gated
semantic tool, and section 14 covers advanced variants. Use the corresponding
tool section to diagnose a failure.

---

## Rollup checklist

- [ ] 1. `localSearch` — text/regex search + advanced (regex modes, case, unique, pagination)
- [ ] 2. `astSearch` — match / symbols / files / tree / topology (all 7 analyses)
- [ ] 3. `astRewrite` — structural rewrite (pattern/rule/experimental, apply, escape hatch)
- [ ] 4. `localFetch` — read file + minify / matchString / line-range / chunk pagination
- [ ] 5. `lspSearch` — all semantic operations + anchoring modes
- [ ] 6. `artifactSearch` — all 8 ecosystems, exact + keyword
- [ ] 7. `ghSearch` — code / repositories / tree + filters
- [ ] 8. `ghGetFileContent` — read + minify / matchString / line-range  (incl. **R2**)
- [ ] 9. `ghSearchHistory` — commits / pullRequests / issues  (incl. **R3**)
- [ ] 10. `ghGetHistoryItem` — commit / pullRequest / issue / compare  (incl. **R4**)
- [ ] 11. `ghCloneRepo` — clone (+ branch / sparsePath)
- [ ] 12. `semanticAssess` — resource-question matrices with typed Noul / Choice / Score answers (gated)
- [ ] R1. `astSearch` deadCode continuation is contract-valid

---

## 1. localSearch

**Purpose:** literal/regex text search across local files (ripgrep engine).

**Schema:** required `reasoning`, `searchText`, `path`. Advanced: `regex`
(`literal`|`rust`|`pcre2`), `caseMode` (`smart`|`sensitive`|`insensitive`),
`multiline` (`off`|`on`|`dotall`), `wholeWord`, `invertMatch`, `unique`
(`off`|`list`|`count`), `resultView` (`matchOnly`), `sort` (`relevance`|`path`|
`matchCount`|`modified`|…), `page`/`pageSize`/`matchPage`, `matchContentLength`,
`matchWindow`, `contextLines`, `include`/`exclude`/`excludeDir` globs, `langType`,
`rankingProfile`, `maxDepth`/`maxFiles`/`maxMatchesPerFile`, `hidden`/`noIgnore`.

**Core task:** *"Find every `greet` under `src/`."*
`{"reasoning":"x","searchText":"greet","path":"src"}` → **PASS:** matches in `util.ts` **and** `index.ts`.

**Advanced coverage** (each must return, no violation):
- [ ] `regex:"pcre2"` lookahead — `{"searchText":"gr(?=eet)","path":"src","regex":"pcre2"}`
- [ ] `regex:"literal"` — `{"searchText":"greet(","path":"src","regex":"literal"}`
- [ ] `caseMode:"insensitive"` — `{"searchText":"GREET","path":"src","caseMode":"insensitive"}`
- [ ] `unique:"count"` + `resultView:"matchOnly"` — dedup/count mode
- [ ] `wholeWord:true`, and `invertMatch:true`
- [ ] `include:["*.ts"]` glob on `path:"."`
- [ ] pagination: `page:2` on a broad `searchText`

- [ ] native CLI [ ] node CLI [ ] MCP

---

## 2. astSearch

**Purpose:** structural/AST queries. One tool, five `operation`s.

**Schema:** required `reasoning`, `operation`, `path`.
- `match`: + `pattern` **or** `rule` (JSON string); `langType` required for dirs;
  `resultView` (`content`|`files`|`countMatches`), `captureText`, pagination.
- `symbols`: + optional `kinds`, `name`, `namedOnly`, `nodeLimit`, `nodeOffset`.
- `files`: + filters `names`/`extensions`/`entryType`(`f`|`d`)/`access`/`size`/
  `time`/`minDepth`/`maxDepth`/`pathPattern`/`pathRegex`/`empty`/`permissions`/`limit`.
- `tree`: + `treeKind` (`filesystem`|`syntax`).
- `topology`: + `analysis` (`deadCode`|`cycles`|`dependencies`|`dependents`|
  `path`|`reachability`|`drift`); `dependencies`/`dependents`/`path` need `file`
  (and `path`-analysis needs `target`); `drift` needs a `baseline` snapshot;
  `entrypoints`, `includeTests`, `rustWorkspace` (`syntax`|`cargo`), `depth`.

**Core task:** *"Find `greet($A)` calls in `src/`."*
`{"operation":"match","path":"src","pattern":"greet($A)","langType":"typescript"}`
→ **PASS:** `stats.totalStructuralMatches` == 2.

**Advanced coverage:**
- [ ] `match` with `rule` (JSON) — `{"operation":"match","path":"src","langType":"typescript","rule":"{\"pattern\":\"greet($A)\"}"}`
- [ ] `match` `resultView:"countMatches"` and `resultView:"files"`
- [ ] `operation:"symbols"` on `src` → lists `greet`/`shout`/`main`
- [ ] `operation:"files"` with `extensions:["ts"]`
- [ ] `operation:"tree"` `treeKind:"filesystem"` **and** `treeKind:"syntax"` (on a single file)
- [ ] `topology` `analysis:"cycles"`
- [ ] `topology` `analysis:"dependencies"` `file:"src/index.ts"`
- [ ] `topology` `analysis:"dependents"` `file:"src/util.ts"`
- [ ] `topology` `analysis:"reachability"`
- [ ] **R1 (regression):** `topology` `analysis:"deadCode"` completes with **no**
      `outputContractViolation` (guards `deadcode_verify_references_continuation_is_contract_valid`)
- [ ] language spread: run a `match` on a Rust/Python file with the right `langType`

- [ ] native CLI [ ] node CLI [ ] MCP

---

## 3. astRewrite

**Purpose:** structural find-and-replace; previews a diff, applies only under a
hash-guarded flow.

**Schema:** required `reasoning`, `path`, `langType`, `ruleKind`
(`pattern`|`rule`|`experimental`). `pattern`: + `pattern`+`rewrite`. `rule`: +
`rule`+`fix`. `experimental`: + `rule`+`transform`+`fix`+`rewriters`. Advanced:
`apply`, `allowSyntaxRegression`, `expectedHashes`, `snapshot`, `selectedMatchIds`,
`postconditions`, `constraints`/`utils`, `include`/`exclude`, `maxFiles`/`maxMatches`,
`page`/`pageSize`.

**Core task:** *"Preview `greet(x)`→`greet2(x)` in `src/util.ts`."*
`{"path":"src/util.ts","langType":"typescript","ruleKind":"pattern","pattern":"greet($A)","rewrite":"greet2($A)"}`
→ **PASS:** `mode:"preview"`, `totalMatches` ≥ 1, includes a `patch`.

**Advanced coverage:**
- [ ] `ruleKind:"rule"` with `rule`+`fix` objects
- [ ] escape hatch: `rewrite:"greet2($A"` (unbalanced) → `errorCode:"ast.rewrite.broken_syntax"`;
      re-run with `allowSyntaxRegression:true` → `mode:"preview"`
- [ ] `include`/`exclude` globs on a directory `path`
- [ ] pagination on many matches (`pageSize:1`, follow `next.nextPage`)

- [ ] native CLI [ ] node CLI [ ] MCP

---

## 4. localFetch

**Purpose:** read a local file — whole, line range, match-filtered, chunk-paginated,
optionally minified.

**Schema:** required `reasoning`, `path`. Advanced: `fullContent`,
`startLine`+`endLine`, `matchString` (+`matchStringIsRegex`,
`matchStringCaseSensitive`, `contextLines`), `minify` (`none`|`standard`|`symbols`),
`chunkType` (`lines`|`bytes`) + `limit`/`offset` (pagination), `contextBytes`.

**Core task:** *"Read `src/util.ts`."*  `{"path":"src/util.ts"}`
→ **PASS:** returns `greet`/`shout` source.

**Advanced coverage:**
- [ ] `minify:"standard"` and `minify:"symbols"` — both return without violation
- [ ] `startLine:1,endLine:2` line-range read
- [ ] `matchString:"gr.et",matchStringIsRegex:true,contextLines:1` — regex match filter
- [ ] `chunkType:"lines",limit:2,offset:0` — chunk pagination
- [ ] `fullContent:true`

- [ ] native CLI [ ] node CLI [ ] MCP

---

## 5. lspSearch

**Purpose:** semantic navigation via a real language server.

**Requires** a language server on `PATH` (`typescript-language-server` for the TS
fixture; `rust-analyzer` for Rust). Without one the tool returns a structured
"unavailable" result — wired but not full function; install one for a true PASS.

**Schema:** anchored ops `definition`/`references`/`callers`/`callees`/
`callHierarchy`/`hover`/`typeDefinition`/`implementation`/`supertypes`/`subtypes`
require `uri` + (`symbolName`+`lineHint`) **or** `position`. `documentSymbols`/
`diagnostic` require `uri`+`operation`. `workspaceSymbol` requires `symbolName`
(+`uri` or `workspaceRoot`). Advanced: `format` (`structured`|`compact`),
`includeDeclaration`, `groupByFile`, `depth`, `page`/`pageSize`, `contextLines`.

**Core task:** *"References to `greet` (util.ts:1)."*
`{"operation":"references","uri":"$FIX/src/util.ts","symbolName":"greet","lineHint":1}`
→ **PASS:** locations or a valid `next.readFile`, no violation.

**Advanced coverage** (each op, TS fixture):
- [ ] `definition`, `hover`, `typeDefinition` (anchored at a symbol)
- [ ] `callers`/`callees` on `main`
- [ ] `implementation`/`supertypes`/`subtypes` (may be empty — must not violate)
- [ ] `documentSymbols` on `src/util.ts`
- [ ] `position`-anchored variant instead of `symbolName`+`lineHint`

- [ ] native CLI [ ] node CLI [ ] MCP

---

## 6. artifactSearch

**Purpose:** find/resolve a package or locate upstream source. **Network.**

**Schema:** required `reasoning`, `type` (`npm`|`pypi`|`crates`|`maven`|`nuget`|
`go`|`packagist`|`rubygems`), and exactly one of `packageName` (exact) or
`keywords` (discovery). Advanced: `registry`, `pageSize`, `cursor`.

**Core task:** *"Look up npm `left-pad`."*  `{"type":"npm","packageName":"left-pad"}`
→ **PASS:** `artifacts[]` with `name: left-pad` + `registryUrl`.

**Advanced coverage — all 8 ecosystems** (each must resolve):
- [ ] `npm` left-pad · [ ] `pypi` requests · [ ] `crates` serde · [ ] `maven` com.google.guava:guava
- [ ] `nuget` Newtonsoft.Json · [ ] `go` github.com/gin-gonic/gin · [ ] `packagist` monolog/monolog · [ ] `rubygems` rails
- [ ] keyword discovery: `{"type":"npm","keywords":["left","pad"]}`

- [ ] native CLI [ ] node CLI [ ] MCP

---

## 7. ghSearch

**Purpose:** search GitHub code / repositories / a repo tree. **Network.**

**Schema:** required `reasoning`, `operation` (`code`|`repositories`|`tree`).
`code`: keywords + `owner`/`repo`/`language`/`extension`/`filename`/`path`/`match`
(`file`|`path`). `repositories`: ≥1 of `owner`/`language`/`stars`/`topics`/… +
`sort` (`stars`|`forks`|`updated`|`best-match`|…). `tree`: `owner`+`repo` (+`branch`,
`materialize`, `maxDepth`). Advanced: `page`/`pageSize`, `visibility`, `archived`.

**Core task:** *"Repositories owned by `bgauryy`."*
`{"operation":"repositories","owner":"bgauryy"}` → **PASS:** non-empty `repositories`.

**Advanced coverage:**
- [ ] `operation:"code"` + `owner`/`repo`/`language:"typescript"`
- [ ] `operation:"tree"` `owner`/`repo`
- [ ] `operation:"repositories"` `sort:"stars"`

- [ ] native CLI [ ] node CLI [ ] MCP

---

## 8. ghGetFileContent

**Purpose:** read a GitHub file without cloning. **Network.**

**Schema:** required `reasoning`, `owner`, `repo`, `path`. Advanced: `branch`,
`fullContent`, `startLine`+`endLine`, `matchString` (+`matchStringIsRegex`,
`matchStringCaseSensitive`, `contextLines`), `minify` (`none`|`standard`|`symbols`),
`chunkType`+`limit`/`offset`, `forceRefresh`.

**Core task:** *"Read `README.md` from `bgauryy/octocode`."*
`{"owner":"bgauryy","repo":"octocode","path":"README.md"}` → **PASS:** `files[0].content`.

**Advanced coverage:**
- [ ] `minify:"standard"`
- [ ] `matchString:"Octocode",contextLines:1`
- [ ] `startLine:1,endLine:5` line-range
- [ ] **R2 (regression):** a **404 path** (`path:"DOES_NOT_EXIST.md"`) returns a
      not-found result with a valid `next.viewTree` hint and **no**
      `outputContractViolation` (guards `filecontent_notfound_viewtree_continuation_carries_pagination`)

- [ ] native CLI [ ] node CLI [ ] MCP

---

## 9. ghSearchHistory

**Purpose:** search commits, pull requests, or issues. **Network.**

**Schema:** required `reasoning`, `operation` (`commits`|`pullRequests`|`issues`);
`commits`/`issues` also need `owner`+`repo`. Advanced filters: `author`/`committer`/
`assignee`/`commenter`/`mentions`, `state` (`open`|`closed`|`merged`), `label`,
`review`/`checks`/`draft`, `since`/`until`/`created`/`updated`/`closed`,
`sort`/`order`, `base`/`head`/`branch`/`path`, `page`/`pageSize`.

**Core task:** *"Recent commits in `bgauryy/octocode`."*
`{"operation":"commits","owner":"bgauryy","repo":"octocode"}` → **PASS:** `commits[]` with SHAs.

**Advanced coverage:**
- [ ] `operation:"issues"` — `{"operation":"issues","owner":"bgauryy","repo":"octocode"}`
- [ ] **R3 (regression):** `operation:"pullRequests"` (`state:"merged"`) completes
      with **no** `outputContractViolation` — its `next.nextPage` must carry
      `pageSize` (guards `ghsearchhistory_nextpage_carries_pagesize`)

- [ ] native CLI [ ] node CLI [ ] MCP

---

## 10. ghGetHistoryItem

**Purpose:** fetch one commit / PR / issue / compare range. **Network.**

**Schema:** required `reasoning`, `owner`, `repo`, `operation` (`commit`|
`pullRequest`|`issue`|`compare`); then `ref` (commit), `number` (PR/issue), or
`base`+`head` (compare). Advanced: `includeDiff`, `minify` (`none`|`standard`),
`matchString`, `path`, `fileBatch`, `charOffset`/`charLength`, and the paginators
`page`/`filePage`/`commentPage`/`reviewPage`/`commitPage`, `content` ref.

**Core task:** *"A commit by ref from `bgauryy/octocode`."*
`{"owner":"bgauryy","repo":"octocode","operation":"commit","ref":"HEAD"}`
→ **PASS:** `type:"commit"` with `sha`+`message`.

**Advanced coverage:**
- [ ] `operation:"commit"` `includeDiff:true`
- [ ] **R4 (regression):** `operation:"compare"` (`base:"HEAD~1",head:"HEAD"`)
      completes with **no** `outputContractViolation` — its `next.nextPage` must
      not clobber `page` (guards `ghgethistoryitem_compare_nextpage_keeps_page`)
- [ ] `operation:"pullRequest"`/`"issue"` with a valid `number` (if the repo has one)

- [ ] native CLI [ ] node CLI [ ] MCP

---

## 11. ghCloneRepo

**Purpose:** clone a repo locally for repeated deep analysis. **Network + git auth.**

**Schema:** required `reasoning`, `owner`, `repo`. Advanced: `branch`, `sparsePath`,
`forceRefresh`.

**Core task:** *"Clone `octocat/Hello-World`."*  `{"owner":"octocat","repo":"Hello-World"}`
→ **PASS:** a `localPath`, **or** a clean `git … clone failed` that reached git
(wired). A crash or `invalidInput` is FAIL. In credential-less sandboxes,
"reached git, auth-blocked" = wired-but-env-limited (note, don't fail).

**Advanced coverage:**
- [ ] `branch:"master"` targeted clone
- [ ] `sparsePath` partial checkout (if clone auth is available)

- [ ] native CLI [ ] node CLI [ ] MCP

---

## 12. semanticAssess (gated)

`semanticAssess` is the only public semantic-assessment tool. The CLI always
discovers its command and schema. MCP registers it only when runtime
configuration resolves a nonblank `OCTOCODE_CLASSIFICATION_API`; live evaluation also
requires provider access. Model selection belongs to runtime configuration,
never the request.

Inspect and execute through the built CLI:

```bash
node "$NODECLI" scheme semanticAssess --view query --compact
node "$NODECLI" semanticAssess --input request.json --compact
```

Each SemanticQuery has a stable `id`, `reasoning`, `resources[]`, and
`questions[]`. Every question is applied to every resource. A resource context
is either supplied non-empty state in `{value:...}` or one unexecuted bounded
read request in `{tool,query}`. Use a root `queries[]` only to batch independent
matrices when a cross-product is incorrect. A single matrix allows at most 25
cells; a batch allows five matrices and 50 total cells. `maxChars` bounds one
resource to at most 80,000 sanitized characters.

Noul answers one binary proposition as `P(yes)`. Choice selects among 2–255
caller labels and returns the distribution. Score evaluates one ordered 2–10
level rubric and returns the expected zero-based level. Instructions must be
non-empty. Noul's complete `true`/`false` criteria and Choice descriptions can
be `null`; Score levels must not be `null`.

Results are correlated as `queries[] → results[] → pages[]` by `queryId`,
`resourceId`, `questionId`, and `pageIndex`. Each success page preserves the
typed provider answer, token usage, and separate `requestedModel` and
`resolvedModel`; its body-free context receipt records hash, coverage,
limitations, and continuation metadata. Large resources remain ordered,
page-local results—there is no hidden reducer. Retain partial and error pages,
and run `next.assess` unchanged when present.

- [ ] Without `OCTOCODE_CLASSIFICATION_API`, `scheme semanticAssess` succeeds,
      `semanticAssess` fails with an error that names the key and setup URL, and
      MCP `tools/list` omits the tool.
- [ ] With the key, MCP `tools/list` includes exactly one `semanticAssess` entry.
- [ ] A two-resource × three-question call returns six correlated cells with
      Noul, Choice, and Score answers, model provenance, receipts, and usage.
- [ ] An independent root `queries[]` batch preserves each query's IDs and
      matrix boundaries.
- [ ] A resource larger than one provider page exposes every page result in
      order; partial coverage supplies executable `next.assess` rather than a
      hidden aggregate.
- [ ] Read-tool contexts return receipts without source bodies; unreadable,
      out-of-policy, and over-budget resources fail before provider access.
- [ ] Unknown root fields, duplicate IDs, empty values/instructions, invalid
      nulls, primitive cardinalities, and matrix limits fail before provider access.
- [ ] Returned judgments match fixed expected outcomes on held-out evidence;
      include insufficient-evidence and conflicting-evidence cases.
- [ ] Report CLI latency and provider input/output tokens separately from host
      prompt/discovery tokens. Missing key or provider failure is unverified,
      never a quality pass.

---

## 13. Core matrix runner (11 non-provider tools × 3 surfaces)

Run from the fixture cwd (§0.3) with the env from §0.2. Prints a pass grid;
`ghCloneRepo` shows `reached-net(auth)` in credential-less sandboxes.

```bash
BIN="$BIN" NODECLI="$NODECLI" MCP="$MCP" FIXROOT="$FIX" node - <<'NODE'
const { spawnSync, spawn } = require("node:child_process");
const FIX = process.env.FIXROOT;
const tools = {
  localSearch:{reasoning:"x",searchText:"greet",path:"src"},
  astSearch:{reasoning:"x",operation:"match",path:"src",pattern:"greet($A)",langType:"typescript"},
  astRewrite:{reasoning:"x",path:"src/util.ts",langType:"typescript",ruleKind:"pattern",pattern:"greet($A)",rewrite:"greet2($A)"},
  localFetch:{reasoning:"x",path:"src/util.ts"},
  lspSearch:{reasoning:"x",operation:"references",uri:FIX+"/src/util.ts",symbolName:"greet",lineHint:1},
  artifactSearch:{reasoning:"x",type:"npm",packageName:"left-pad"},
  ghSearch:{reasoning:"x",operation:"repositories",owner:"bgauryy"},
  ghGetFileContent:{reasoning:"x",owner:"bgauryy",repo:"octocode",path:"README.md"},
  ghSearchHistory:{reasoning:"x",operation:"commits",owner:"bgauryy",repo:"octocode"},
  ghGetHistoryItem:{reasoning:"x",owner:"bgauryy",repo:"octocode",operation:"commit",ref:"HEAD"},
  ghCloneRepo:{reasoning:"x",owner:"octocat",repo:"Hello-World"},
};
const cl=t=>/outputContractViolation|violates its canonical output contract/.test(t)?"FAIL(contract)":/Invalid arguments|invalidInput|contract validation failed|unexpected argument/.test(t)?"FAIL(input)":/panicked|is not a function|Cannot find module/.test(t)?"FAIL(crash)":/could not read Username|terminal prompts disabled|git .*clone failed/.test(t)?"reached-net(auth)":"PASS";
const cli=(bin,n,q)=>{const r=spawnSync(bin[0],[...bin.slice(1),n,JSON.stringify(q),"--compact"],{cwd:FIX,encoding:"utf8",timeout:60000,env:process.env});return cl((r.stdout||"")+(r.stderr||"")+(r.error?.message||""));};
const mcpAll=names=>new Promise(res=>{const c=spawn("node",[process.env.MCP],{stdio:["pipe","pipe","pipe"],cwd:FIX,env:process.env});let b="",i=0,id=100;const o={};const s=x=>c.stdin.write(JSON.stringify(x)+"\n");const nx=()=>{if(i>=names.length){c.kill();return res(o);}const n=names[i++];c._n=n;s({jsonrpc:"2.0",id:++id,method:"tools/call",params:{name:n,arguments:{queries:[tools[n]]}}});};c.stdout.on("data",d=>{b+=d;let ls=b.split("\n");b=ls.pop();for(const l of ls){if(!l.trim())continue;let m;try{m=JSON.parse(l)}catch{continue}if(m.id===1)nx();else if(m.id>100){o[c._n]=cl(JSON.stringify(m.result||m.error||""));nx();}}});c.stderr.on("data",()=>{});s({jsonrpc:"2.0",id:1,method:"initialize",params:{protocolVersion:"2025-06-18",capabilities:{},clientInfo:{name:"x",version:"0"}}});setTimeout(()=>{c.kill();res(o)},120000);});
(async()=>{const names=Object.keys(tools);const mcp=await mcpAll(names);console.log("TOOL".padEnd(18),"NATIVE-CLI".padEnd(20),"NODE-CLI".padEnd(20),"MCP");console.log("-".repeat(78));for(const n of names)console.log(n.padEnd(18),cli([process.env.BIN],n,tools[n]).padEnd(20),cli(["node",process.env.NODECLI],n,tools[n]).padEnd(20),mcp[n]||"?");})();
NODE
```

---

## 14. Advanced matrix runner (schema-variant coverage)

Exercises the advanced variants that don't need special auth (local + registry +
the two regression cases). Prints ✅/❌ per variant on the native CLI (the shared
runtime; run through node/MCP too for full coverage).

```bash
BIN="$BIN" FIXROOT="$FIX" node - <<'NODE'
const { spawnSync } = require("node:child_process");
const FIX=process.env.FIXROOT, BIN=process.env.BIN;
const V = {
  "localFetch minify=standard":["localFetch",{reasoning:"x",path:"src/util.ts",minify:"standard"}],
  "localFetch minify=symbols":["localFetch",{reasoning:"x",path:"src/util.ts",minify:"symbols"}],
  "localFetch lineRange":["localFetch",{reasoning:"x",path:"src/util.ts",startLine:1,endLine:2}],
  "localFetch matchString+regex":["localFetch",{reasoning:"x",path:"src/util.ts",matchString:"gr.et",matchStringIsRegex:true,contextLines:1}],
  "localFetch chunk pagination":["localFetch",{reasoning:"x",path:"src/util.ts",chunkType:"lines",limit:2,offset:0}],
  "localSearch regex=pcre2":["localSearch",{reasoning:"x",searchText:"gr(?=eet)",path:"src",regex:"pcre2"}],
  "localSearch regex=literal":["localSearch",{reasoning:"x",searchText:"greet(",path:"src",regex:"literal"}],
  "localSearch unique=count":["localSearch",{reasoning:"x",searchText:"greet",path:"src",unique:"count",resultView:"matchOnly"}],
  "astSearch op=files":["astSearch",{reasoning:"x",operation:"files",path:".",extensions:["ts"]}],
  "astSearch op=symbols":["astSearch",{reasoning:"x",operation:"symbols",path:"src"}],
  "astSearch tree=syntax":["astSearch",{reasoning:"x",operation:"tree",path:"src/util.ts",treeKind:"syntax"}],
  "astSearch match/rule":["astSearch",{reasoning:"x",operation:"match",path:"src",langType:"typescript",rule:'{"pattern":"greet($A)"}'}],
  "astSearch topology=cycles":["astSearch",{reasoning:"x",operation:"topology",path:".",analysis:"cycles"}],
  "astSearch topology=dependencies":["astSearch",{reasoning:"x",operation:"topology",path:".",analysis:"dependencies",file:"src/index.ts"}],
  "R1 topology=deadCode":["astSearch",{reasoning:"x",operation:"topology",path:".",analysis:"deadCode"}],
  "artifact npm":["artifactSearch",{reasoning:"x",type:"npm",packageName:"left-pad"}],
  "artifact pypi":["artifactSearch",{reasoning:"x",type:"pypi",packageName:"requests"}],
  "artifact crates":["artifactSearch",{reasoning:"x",type:"crates",packageName:"serde"}],
  "artifact maven":["artifactSearch",{reasoning:"x",type:"maven",packageName:"com.google.guava:guava"}],
  "artifact nuget":["artifactSearch",{reasoning:"x",type:"nuget",packageName:"Newtonsoft.Json"}],
  "artifact go":["artifactSearch",{reasoning:"x",type:"go",packageName:"github.com/gin-gonic/gin"}],
  "artifact packagist":["artifactSearch",{reasoning:"x",type:"packagist",packageName:"monolog/monolog"}],
  "artifact rubygems":["artifactSearch",{reasoning:"x",type:"rubygems",packageName:"rails"}],
  "ghFileContent minify":["ghGetFileContent",{reasoning:"x",owner:"bgauryy",repo:"octocode",path:"README.md",minify:"standard"}],
  "R2 ghFileContent 404":["ghGetFileContent",{reasoning:"x",owner:"bgauryy",repo:"octocode",path:"DOES_NOT_EXIST.md"}],
  "R3 ghHistory pullRequests":["ghSearchHistory",{reasoning:"x",operation:"pullRequests",owner:"bgauryy",repo:"octocode",state:"merged"}],
  "R4 ghItem compare":["ghGetHistoryItem",{reasoning:"x",owner:"bgauryy",repo:"octocode",operation:"compare",base:"HEAD~1",head:"HEAD"}],
};
const ok=t=>!/outputContractViolation|invalidInput|Invalid arguments|panicked|contract validation failed/.test(t);
for(const [label,[name,q]] of Object.entries(V)){
  const r=spawnSync(BIN,[name,JSON.stringify(q),"--compact"],{cwd:FIX,encoding:"utf8",timeout:60000,env:process.env});
  console.log((ok((r.stdout||"")+(r.stderr||""))?"✅":"❌")+" "+label);
}
NODE
```

**Expected:** every variant ✅. Any ❌ is a real schema/contract regression — open
the matching tool section.

---

## Maintenance

- Query schemas are authoritative in
  `packages/octocode-native/crates/runtime/src/contracts/generated/tool-contract.json`.
  Regenerate from `@octocodeai/octocode-core`; update a section here when a tool's
  fields change.
- Regression checks R1–R4 mirror Rust tests in
  `crates/runtime/src/contracts/mod.rs`
  (`deadcode_verify_references_continuation_is_contract_valid`,
  `filecontent_notfound_viewtree_continuation_carries_pagination`,
  `ghsearchhistory_nextpage_carries_pagesize`,
  `ghgethistoryitem_compare_nextpage_keeps_page`). Keep them in sync.
- Adding a tool: add a numbered section (purpose → schema → core task → advanced
  coverage → checkbox), a rollup line, and entries in §13/§14.
