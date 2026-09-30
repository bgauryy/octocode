// Head-to-head: base shell tools (rg/find/sed) vs Octocode over the local MCP
// server, the same five tasks in each of the 12 grammar repos.
//   node octocode-local-testing/harness/compare.mjs [lang,lang]
// Writes results/compare.json and prints a per-language table.
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve(new URL("../..", import.meta.url).pathname);
const REPOS = path.join(ROOT, "octocode-local-testing/repos");
// ripgrep from PATH, or RG_BIN (e.g. a bundled copy).
const rgCmd = process.env.RG_BIN ?? "rg";

// One realistic anchor per grammar: a declaration line, its symbol, the file
// that declares it, a source directory, and a base-tool outline regex.
const LANGS = [
  { lang: "typescript", dir: "typescript", file: "packages/typescript/src/ast/factory.generated.ts", decl: "export function createSourceFile(", symbol: "createSourceFile", line: 6068, src: "tsc/internal/compiler", outline: "^\\s*(export )?(async )?(function|class|interface|const|type) ", lsp: true },
  { lang: "tsx", dir: "tsx", file: "packages/element/src/mutateElement.ts", decl: "export const mutateElement = ", symbol: "mutateElement", line: 40, src: "packages/element/src", outline: "^\\s*(export )?(async )?(function|class|interface|const|type) ", lsp: true },
  { lang: "javascript", dir: "javascript", file: "lodash.js", decl: "function debounce(func, wait, options)", symbol: "debounce", line: 10403, src: "fp", outline: "^\\s*(var |function |\\w+\\.prototype\\.)", lsp: true },
  { lang: "python", dir: "python", file: "django/shortcuts.py", decl: "def get_object_or_404(", symbol: "get_object_or_404", line: 79, src: "django/db", outline: "^\\s*(async )?(def|class) " },
  { lang: "go", dir: "go", file: "promql/engine.go", decl: "func NewEngine(", symbol: "NewEngine", line: 387, src: "promql", outline: "^(func|type) " },
  { lang: "rust", dir: "rust", file: "tokio/src/task/blocking.rs", decl: "pub fn spawn_blocking<F, R>(", symbol: "spawn_blocking", line: 220, src: "tokio/src/runtime", outline: "^\\s*(pub(\\([a-z]+\\))? )?(async )?(fn|struct|enum|trait|impl|mod) ", lsp: true },
  { lang: "java", dir: "java", file: "guava/src/com/google/common/base/Preconditions.java", decl: "public static <T> T checkNotNull(@Nullable T reference)", symbol: "checkNotNull", line: 955, src: "guava/src/com/google/common/base", outline: "^\\s*(public|private|protected|static|final|abstract).*[({]\\s*$" },
  { lang: "c", dir: "c", file: "src/networking.c", decl: "client *createClient(connection *conn)", symbol: "createClient", line: 122, src: "src", outline: "^[A-Za-z_][A-Za-z0-9_ \\*]*\\(.*\\)\\s*\\{?$", lsp: true },
  { lang: "cpp", dir: "cpp", file: "include/nlohmann/detail/json_pointer.hpp", decl: "class json_pointer", symbol: "json_pointer", line: 36, src: "include/nlohmann/detail", outline: "^\\s*(class|struct|template|[A-Za-z_:<>]+ [A-Za-z_]+\\()", lsp: true },
  { lang: "csharp", dir: "csharp", file: "Src/Newtonsoft.Json/JsonConvert.cs", decl: "public static string SerializeObject(object? value)", symbol: "SerializeObject", line: 550, src: "Src/Newtonsoft.Json/Serialization", outline: "^\\s*(public|private|protected|internal).*(class|\\()" },
  { lang: "scala", dir: "scala", file: "core/src/main/scala/cats/Foldable.scala", decl: "def foldMap[A, B](fa: F[A])(f: A => B)(implicit B: Monoid[B]): B", symbol: "foldMap", line: 460, src: "core/src/main/scala/cats", outline: "^\\s*(def|class|trait|object|type) " },
  { lang: "asm", dir: "asm", file: "simd/x86_64/jccolext-avx2.asm", decl: "EXTN(jsimd_rgb_ycc_convert_avx2):", symbol: "jsimd_rgb_ycc_convert_avx2", line: 38, src: "simd/x86_64", outline: "^[A-Za-z_][A-Za-z0-9_]*:|^EXTN\\(|^%macro " },
];

const only = process.argv[2]?.split(",");
const langs = LANGS.filter((l) => !only || only.includes(l.lang));
const brief = { goal: "Compare base tools with Octocode on one research task.", reasoning: "Measure the context each approach returns." };

const child = spawn("node", ["packages/octocode-mcp/dist/index.js"], { cwd: ROOT, env: { ...process.env, ENABLE_LOCAL: "true" }, stdio: ["pipe", "pipe", "ignore"] });
let buf = "";
const wait = new Map();
let id = 0;
child.stdout.on("data", (c) => {
  buf += c;
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, i);
    buf = buf.slice(i + 1);
    try {
      const m = JSON.parse(line);
      wait.get(m.id)?.(m);
    } catch {}
  }
});
const req = (method, params) => new Promise((r) => { const n = ++id; wait.set(n, r); child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id: n, method, params })}\n`); });
await req("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "compare", version: "1" } });
child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);

async function octo(calls) {
  let out = "";
  let ms = 0;
  let errors = 0;
  for (const [tool, query] of calls) {
    const t = Date.now();
    const r = await req("tools/call", { name: tool, arguments: { queries: [{ ...brief, ...query }] } });
    ms += Date.now() - t;
    if (r.result?.isError) errors += 1;
    out += JSON.stringify(r.result?.structuredContent ?? r.result ?? r.error);
  }
  return { out, ms, calls: calls.length, errors };
}
function base(commands) {
  let out = "";
  let ms = 0;
  for (const command of commands) {
    const t = Date.now();
    const r = spawnSync("/bin/zsh", ["-c", command], { encoding: "utf8", maxBuffer: 512 << 20 });
    ms += Date.now() - t;
    out += r.stdout;
  }
  return { out, ms, calls: commands.length, errors: 0 };
}
const q = (s) => `'${s.replaceAll("'", "'\\''")}'`;

const rows = [];
for (const l of langs) {
  const repo = path.join(REPOS, l.dir);
  const file = path.join(repo, l.file);
  const src = path.join(repo, l.src);
  const tasks = [
    {
      task: "definition",
      check: [path.basename(l.file), String(l.line)],
      base: [`${rgCmd} -n -F ${q(l.decl)} ${q(repo)}`],
      octo: [["localSearch", { path: repo, searchText: l.decl, regex: "literal" }]],
    },
    {
      task: "call sites",
      check: [l.symbol],
      base: [`${rgCmd} -n -w ${q(l.symbol)} ${q(repo)}`],
      octo: l.lsp
        ? [["lspSearch", { operation: "references", uri: file, symbolName: l.symbol, lineHint: l.line }]]
        : [["localSearch", { path: repo, searchText: l.symbol, wholeWord: true }]],
    },
    {
      task: "outline",
      check: [l.symbol],
      base: [`${rgCmd} -n ${q(l.outline)} ${q(file)}`],
      octo: [["astSearch", { operation: "symbols", path: file }]],
    },
    {
      task: "read function",
      check: [l.decl.slice(0, 24)],
      base: [`${rgCmd} -n -F ${q(l.decl)} ${q(file)}`, `sed -n ${l.line},${l.line + 30}p ${q(file)}`],
      octo: [["localFetch", { path: file, matchString: l.decl, contextLines: 15 }]],
    },
    {
      task: "dir overview",
      check: [fs.readdirSync(src).filter((n) => !n.startsWith("."))[0] ?? ""],
      base: [`find ${q(src)} -maxdepth 2`],
      octo: [["structureSearch", { operation: "tree", path: src, maxDepth: 2 }]],
    },
  ];
  for (const t of tasks) {
    const b = base(t.base);
    const o = await octo(t.octo);
    const found = (s) => t.check.every((c) => s.includes(c));
    rows.push({ lang: l.lang, task: t.task, base: { chars: b.out.length, ms: b.ms, calls: b.calls, ok: found(b.out) }, octo: { chars: o.out.length, ms: o.ms, calls: o.calls, ok: found(o.out) && o.errors === 0, errors: o.errors, tool: t.octo[0][0] } });
  }
}
child.kill();
fs.writeFileSync(path.join(ROOT, "octocode-local-testing/results/compare.json"), JSON.stringify(rows, null, 1));

const pad = (s, n) => String(s).padEnd(n);
const num = (s, n) => String(s).padStart(n);
console.log(`${pad("lang", 11)}${pad("task", 14)}${num("base chars", 11)}${num("ms", 7)} ok | ${pad("octocode tool", 16)}${num("chars", 9)}${num("ms", 7)} ok`);
for (const r of rows) {
  console.log(`${pad(r.lang, 11)}${pad(r.task, 14)}${num(r.base.chars, 11)}${num(r.base.ms, 7)} ${r.base.ok ? "✓" : "✗"}  | ${pad(r.octo.tool, 16)}${num(r.octo.chars, 9)}${num(r.octo.ms, 7)} ${r.octo.ok ? "✓" : "✗"}`);
}
const sum = (side, key) => rows.reduce((s, r) => s + r[side][key], 0);
const okCount = (side) => rows.filter((r) => r[side].ok).length;
console.log(`TOTAL base ${sum("base", "chars")} chars ${sum("base", "ms")} ms ${okCount("base")}/${rows.length} ok · octocode ${sum("octo", "chars")} chars ${sum("octo", "ms")} ms ${okCount("octo")}/${rows.length} ok`);
