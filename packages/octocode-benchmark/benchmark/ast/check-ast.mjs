#!/usr/bin/env node
// AST benchmark check — proves every supported tree-sitter grammar works
// end-to-end through the shipped octocode-engine napi binary, run over the REAL
// third-party files in ast/samples/ (see ast/manifest.json for provenance).
//
// Per grammar:
//   PARSE     — structuralSearch(<real sample>, "$$$") yields nodes ("$$$"
//               matches any node sequence, so >0 proves the grammar loaded and
//               parsed the real file; catches ABI mismatches that only surface
//               at parse time).
//   MATCH     — structuralSearch(<canonical snippet>, <pattern-or-rule>) returns
//               the expected node(s). Pattern probes prove metavars where the
//               grammar supports them cleanly; rule probes prove exact node-kind
//               matching for grammars where bare call patterns are ambiguous.
//   SIGNATURE — signature-tier grammars must return a non-empty skeleton from
//               extractSignatures(<real sample>) AND from the canonical snippet.
//
// A coverage pass asserts every extension in getSupportedStructuralExtensions()
// is claimed by exactly one grammar entry, so a new engine grammar without a
// sample + proof here fails the check. Exits non-zero on any failure.

import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { engine, loadManifestSamples, pad } from '../_engine.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const samples = loadManifestSamples(here)

const PARSE_PROBE = '$$$'

// One entry per distinct tree-sitter Language. `sig:true` => non-empty
// body_query (must produce a signature skeleton). `aliases` resolve to the SAME
// grammar — coverage-checked + parse-probed, share the representative's sample.
const GRAMMARS = [
  { name: 'TypeScript', ext: 'ts', aliases: ['mts', 'cts'], sig: true, snippet: 'const a = foo(bar);\n', pattern: 'foo($X)', min: 1, sigSnippet: 'export function foo(a: number): number {\n  return a + 1;\n}\n' },
  { name: 'TSX', ext: 'tsx', aliases: [], sig: true, snippet: 'const a = foo(bar);\n', pattern: 'foo($X)', min: 1, sigSnippet: 'export function Foo(a: number): number {\n  return a + 1;\n}\n' },
  { name: 'JavaScript', ext: 'js', aliases: ['jsx', 'mjs', 'cjs'], sig: true, snippet: 'log(1, 2, 3);\n', pattern: 'log($$$A)', min: 1, sigSnippet: 'function foo(a) {\n  return a + 1;\n}\n' },
  { name: 'Python', ext: 'py', aliases: ['pyi'], sig: true, snippet: 'print(x)\n', pattern: 'print($X)', min: 1, sigSnippet: 'def foo(a):\n    return a + 1\n' },
  { name: 'Go', ext: 'go', aliases: [], sig: true, snippet: 'package m\nfunc f() { foo(x) }\n', rule: 'rule:\n  kind: call_expression\n', min: 1, sigSnippet: 'package m\nfunc Foo() int {\n  return 1\n}\n' },
  { name: 'Rust', ext: 'rs', aliases: [], sig: true, snippet: 'fn f() { let _ = g(y); }\n', rule: 'rule:\n  kind: call_expression\n', min: 1, sigSnippet: 'fn foo() -> i32 {\n  1\n}\n' },
  { name: 'Java', ext: 'java', aliases: [], sig: true, snippet: 'class A { void m() { foo(x); } }\n', rule: 'rule:\n  kind: method_invocation\n', min: 1, sigSnippet: 'class A {\n  int foo() {\n    return 1;\n  }\n}\n' },
  { name: 'C', ext: 'c', aliases: ['h'], sig: true, snippet: 'int main() { foo(x); }\n', pattern: 'foo($X);', min: 1, sigSnippet: 'int foo(void) {\n  return 1;\n}\n' },
  { name: 'Bash', ext: 'sh', aliases: ['bash', 'zsh'], sig: true, snippet: 'echo hi\nfoo bar baz\n', pattern: PARSE_PROBE, min: 1, sigSnippet: 'foo() {\n  echo hi\n}\n' },
  { name: 'HTML', ext: 'html', aliases: ['htm'], sig: false, snippet: '<div><button id="go">Hi</button></div>', pattern: '<button id="go">$$$</button>', min: 1 },
  { name: 'CSS', ext: 'css', aliases: [], sig: false, snippet: '.btn { color: red; }', pattern: '.btn { color: $C; }', min: 1 },
  { name: 'SCSS', ext: 'scss', aliases: [], sig: false, snippet: '.card { color: red; }', pattern: '.card { color: $C; }', min: 1 },
  { name: 'Less', ext: 'less', aliases: [], sig: false, snippet: '.box { width: 10px; }', pattern: '.box { width: $W; }', min: 1 },
  { name: 'Scala', ext: 'scala', aliases: ['sc', 'sbt'], sig: true,
    snippet: 'object A { println(x) }', pattern: 'println($X)', min: 1,
    sigSnippet: 'class Queue[A] {\n  private var items: List[A] = Nil\n  def enqueue(item: A): Unit = {\n    items = items :+ item\n  }\n  def dequeue(): Option[A] = items match {\n    case Nil => None\n    case h :: t => items = t; Some(h)\n  }\n  def peek(): Option[A] = items.headOption\n  def size: Int = items.length\n}\n\nobject Queue {\n  def empty[A]: Queue[A] = new Queue[A]\n  def of[A](items: A*): Queue[A] = {\n    val q = new Queue[A]\n    items.foreach(q.enqueue)\n    q\n  }\n}\n' },
  { name: 'JSON', ext: 'json', aliases: ['jsonc'], sig: false, snippet: '{"a": 1, "b": 2}', pattern: '$K: $V', min: 1 },
  { name: 'YAML', ext: 'yaml', aliases: ['yml'], sig: false, snippet: 'a: 1\nb: 2\n', pattern: '$K: $V', min: 1 },
  { name: 'TOML', ext: 'toml', aliases: [], sig: false, snippet: 'name = "x"\nver = 2\n', pattern: PARSE_PROBE, min: 1 },
  { name: 'C++', ext: 'cpp', aliases: ['hpp', 'cc', 'cxx', 'hh', 'hxx'], sig: true, snippet: 'int main() { foo(x); }\n', rule: 'rule:\n  kind: call_expression\n', min: 1, sigSnippet: 'int foo() {\n  return 1;\n}\n' },
  { name: 'C#', ext: 'cs', aliases: [], sig: true, snippet: 'class A { void M() { Foo(x); } }\n', rule: 'rule:\n  kind: invocation_expression\n', min: 1, sigSnippet: 'class A {\n  int Foo() {\n    return 1;\n  }\n}\n' },
  // ── Priority-1 additions ─────────────────────────────────────────────────
  { name: 'Ruby', ext: 'rb', aliases: ['rake', 'gemspec', 'ru'], sig: true,
    snippet: 'def hello(name)\n  puts name\nend\n',
    rule: 'rule:\n  kind: method\n', min: 1,
    sigSnippet: 'def foo(a)\n  a + 1\nend\n' },
  { name: 'PHP', ext: 'php', aliases: [], sig: true,
    snippet: '<?php\nfunction foo($x) {\n  return $x;\n}\n',
    rule: 'rule:\n  kind: function_definition\n', min: 1,
    sigSnippet: '<?php\nfunction add($a, $b) {\n  return $a + $b;\n}\n' },
  { name: 'Kotlin', ext: 'kt', aliases: ['kts'], sig: true,
    snippet: 'fun foo(a: Int): Int {\n  return a + 1\n}\n',
    rule: 'rule:\n  kind: function_declaration\n', min: 1,
    sigSnippet: 'fun add(a: Int, b: Int): Int {\n  return a + b\n}\nfun id(x: String): String {\n  return x\n}\n' },
  { name: 'Elixir', ext: 'ex', aliases: ['exs'], sig: true,
    snippet: 'defmodule M do\n  def foo(a) do\n    a\n  end\nend\n',
    rule: 'rule:\n  kind: call\n', min: 1,
    sigSnippet: 'defmodule Greeter do\n  def hello(name) do\n    IO.puts("hi " <> name)\n  end\n  defp greet(msg) do\n    IO.puts(msg)\n  end\nend\n' },
  { name: 'HCL', ext: 'tf', aliases: ['hcl', 'tfvars'], sig: true,
    snippet: 'resource "aws_instance" "main" {\n  ami = "ami-123"\n}\n',
    rule: 'rule:\n  kind: block\n', min: 1,
    sigSnippet: 'resource "aws_rds_instance" "db" {\n  engine            = "mysql"\n  instance_class    = "db.t3.micro"\n  allocated_storage = 20\n  username          = var.db_user\n  password          = var.db_pass\n}\n\nvariable "db_user" {\n  description = "Database username"\n  type        = string\n  sensitive   = true\n}\n\nvariable "db_pass" {\n  description = "Database password"\n  type        = string\n  sensitive   = true\n}\n\noutput "db_endpoint" {\n  description = "RDS instance endpoint"\n  value       = aws_rds_instance.db.endpoint\n  sensitive   = true\n}\n' },
  { name: 'Lua', ext: 'lua', aliases: [], sig: true,
    snippet: 'function foo(a)\n  return a\nend\n',
    rule: 'rule:\n  kind: function_declaration\n', min: 1,
    sigSnippet: 'function greet(name)\n  print("hi " .. name)\nend\nfunction add(a, b)\n  return a + b\nend\n' },
  // ── Priority-2 additions ─────────────────────────────────────────────────
  { name: 'SQL', ext: 'sql', aliases: [], sig: false,
    snippet: 'SELECT id, name FROM users WHERE id = 1;\n',
    rule: 'rule:\n  kind: statement\n', min: 1 },
  { name: 'Protobuf', ext: 'proto', aliases: [], sig: true,
    snippet: 'message Foo {\n  string name = 1;\n  int32 id = 2;\n}\n',
    rule: 'rule:\n  kind: message\n', min: 1,
    sigSnippet: 'message CreateRequest {\n  string name = 1;\n  string email = 2;\n  int32 age = 3;\n  bool active = 4;\n}\n\nmessage User {\n  int64 id = 1;\n  string name = 2;\n  string email = 3;\n}\n\nservice UserService {\n  rpc Create(CreateRequest) returns (User);\n  rpc Get(GetRequest) returns (User);\n  rpc Delete(DeleteRequest) returns (Empty);\n}\n' },
  { name: 'OCaml', ext: 'ml', aliases: ['mli'], sig: false,
    snippet: 'let foo x = x + 1\nlet bar a b = a + b\n',
    rule: 'rule:\n  kind: value_definition\n', min: 1 },
  { name: 'Zig', ext: 'zig', aliases: [], sig: true,
    snippet: 'pub fn foo(a: i32) i32 {\n    return a + 1;\n}\n',
    rule: 'rule:\n  kind: function_declaration\n', min: 1,
    sigSnippet: 'pub fn add(a: i32, b: i32) i32 {\n    return a + b;\n}\npub fn mul(x: i32, y: i32) i32 {\n    return x * y;\n}\n' },
  // ── Priority-3 additions ─────────────────────────────────────────────────
  { name: 'R', ext: 'r', aliases: [], sig: true,
    snippet: 'foo <- function(a, b) {\n  a + b\n}\n',
    rule: 'rule:\n  kind: function_definition\n', min: 1,
    sigSnippet: 'add <- function(a, b) {\n  a + b\n}\nmul <- function(x, y) {\n  x * y\n}\n' },
  { name: 'Julia', ext: 'jl', aliases: [], sig: false,
    snippet: 'function foo(a)\n  a + 1\nend\n',
    rule: 'rule:\n  kind: function_definition\n', min: 1 },
  { name: 'Erlang', ext: 'erl', aliases: ['hrl'], sig: true,
    snippet: '-module(m).\nfoo(X) -> X + 1.\n',
    rule: 'rule:\n  kind: function_clause\n', min: 1,
    sigSnippet: '-module(lists_impl).\n-export([map/2, filter/2, foldl/3]).\n\nmap(_, []) ->\n    [];\nmap(Fun, [Head | Tail]) ->\n    [Fun(Head) | map(Fun, Tail)].\n\nfilter(_, []) ->\n    [];\nfilter(Pred, [Head | Tail]) ->\n    case Pred(Head) of\n        true  -> [Head | filter(Pred, Tail)];\n        false -> filter(Pred, Tail)\n    end.\n\nfoldl(_, Acc, []) ->\n    Acc;\nfoldl(Fun, Acc, [Head | Tail]) ->\n    NewAcc = Fun(Head, Acc),\n    foldl(Fun, NewAcc, Tail).\n' },
  { name: 'Swift', ext: 'swift', aliases: [], sig: true,
    snippet: 'func foo(_ a: Int) -> Int {\n    return a + 1\n}\n',
    rule: 'rule:\n  kind: function_declaration\n', min: 1,
    sigSnippet: 'func add(_ a: Int, _ b: Int) -> Int {\n    return a + b\n}\nfunc greet(_ name: String) -> String {\n    return "hi " + name\n}\n' },
]

const structuralExts = new Set(engine.getSupportedStructuralExtensions())
const signatureExts = new Set(engine.getSupportedSignatureExtensions())
const sc = (content, ext, pattern) => engine.structuralSearch(content, `probe.${ext}`, pattern, null).length
const scRule = (content, ext, rule) => engine.structuralSearch(content, `probe.${ext}`, null, rule).length

const rows = []
const failures = []
const claimed = new Map()
const claim = (ext, name) => { if (claimed.has(ext)) failures.push(`coverage: ${ext} claimed by ${claimed.get(ext)} and ${name}`); claimed.set(ext, name) }

for (const g of GRAMMARS) {
  const issues = []
  claim(g.ext, g.name)
  for (const a of g.aliases) claim(a, g.name)

  // CONTRACT
  if (!structuralExts.has(g.ext)) issues.push('not in structural list')
  if (signatureExts.has(g.ext) !== g.sig) issues.push(g.sig ? 'engine reports structural-only, expected signature' : 'engine reports signature, expected structural-only')
  for (const a of g.aliases) if (!structuralExts.has(a)) issues.push(`alias ${a} not in structural list`)

  // PARSE (real sample — content is sha256/byte-verified by readSample)
  const { content } = samples.readSample(g.ext, issues)
  let parseInfo = 'no sample'
  if (content) {
    let nodes = 0
    try { nodes = sc(content, g.ext, PARSE_PROBE) } catch (e) { issues.push(`parse threw: ${e.message}`) }
    if (nodes <= 0) issues.push('real sample parsed 0 nodes')
    parseInfo = `${nodes} nodes`
    for (const a of g.aliases) {
      try { if (sc(content, a, PARSE_PROBE) <= 0) issues.push(`alias ${a} parsed 0 nodes`) } catch (e) { issues.push(`alias ${a} threw: ${e.message}`) }
    }
  }

  // MATCH (canonical snippet)
  let matches = 0
  const queryLabel = g.pattern || g.rule.trim().split('\n').join(' ')
  try { matches = g.pattern ? sc(g.snippet, g.ext, g.pattern) : scRule(g.snippet, g.ext, g.rule) } catch (e) { issues.push(`match threw: ${e.message}`) }
  if (matches < g.min) issues.push(`canonical match got ${matches}, expected >=${g.min} for \`${queryLabel}\``)

  // SIGNATURE (signature-tier)
  let sigInfo = g.sig ? '' : 'n/a'
  if (g.sig) {
    let canon = null
    try { canon = engine.extractSignatures(g.sigSnippet, `probe.${g.ext}`) } catch (e) { issues.push(`signature threw: ${e.message}`) }
    if (!canon || !canon.trim()) issues.push('canonical signature null/empty')
    let realLines = 0
    if (content) { const real = engine.extractSignatures(content, `probe.${g.ext}`); realLines = real ? real.split('\n').filter(Boolean).length : 0; if (!real) issues.push('real sample signature null') }
    sigInfo = `${realLines} lines`
  }

  const ok = issues.length === 0
  if (!ok) failures.push(`${g.name}: ${issues.join('; ')}`)
  rows.push({ name: g.name, ext: g.ext, tier: g.sig ? 'sig' : 'struct', parse: parseInfo, match: matches, sig: sigInfo, ok })
}

for (const ext of structuralExts) if (!claimed.has(ext)) failures.push(`coverage: engine supports ".${ext}" but no grammar entry claims it`)

console.log(`\nAST benchmark — ${rows.length} grammars over real samples (ast/samples/)`)
console.log(`${pad('Grammar', 12)} ${pad('ext', 6)} ${pad('tier', 7)} ${pad('parse', 12)} ${pad('match', 6)} ${pad('signature', 10)} status`)
console.log('-'.repeat(72))
for (const r of rows) console.log(`${pad(r.name, 12)} ${pad(r.ext, 6)} ${pad(r.tier, 7)} ${pad(r.parse, 12)} ${pad(r.match, 6)} ${pad(r.sig, 10)} ${r.ok ? 'PASS' : 'FAIL'}`)
console.log(`\nCoverage: ${claimed.size}/${structuralExts.size} declared structural extensions claimed.`)

if (failures.length) { console.error(`\n✗ AST check FAILED (${failures.length}):`); for (const f of failures) console.error(`  • ${f}`); process.exit(1) }
console.log(`\n✓ all ${rows.length} grammars working (parse + match + signature) on real samples.`)
