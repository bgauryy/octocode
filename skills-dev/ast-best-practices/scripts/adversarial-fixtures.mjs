#!/usr/bin/env node
// Writes adversarial source fixtures for AST parser/walker/matcher/offset tests.
// Deterministic, dependency-free, finite. See references/testing.md for what each class catches.
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const LANGS = {
  js: { ext: "js", call: (s) => `foo(${s});`, fn: (b) => `function f() {\n${b}\n}\n`, open: "(", close: ")", broken: "function f( {\n  return 1\n" },
  ts: { ext: "ts", call: (s) => `foo(${s});`, fn: (b) => `function f(): void {\n${b}\n}\n`, open: "(", close: ")", broken: "const x: = <number>;\n" },
  py: { ext: "py", call: (s) => `foo(${s})`, fn: (b) => `def f():\n${b.replace(/^/gm, "    ")}\n`, open: "(", close: ")", broken: "def f(:\n  return\n" },
  rs: { ext: "rs", call: (s) => `foo(${s});`, fn: (b) => `fn f() {\n${b}\n}\n`, open: "(", close: ")", broken: "fn f( {\n    let = ;\n" },
  go: { ext: "go", call: (s) => `foo(${s})`, fn: (b) => `package p\n\nfunc f() {\n${b}\n}\n`, open: "(", close: ")", broken: "package p\nfunc f( {\n" },
};

function usage(code = 0) {
  console.log(`usage: adversarial-fixtures.mjs --out <dir> [--lang js|ts|py|rs|go|all] [--depth N] [--huge-bytes N] [--matches N]

Writes one file per fixture class (emoji, crlf, bom, no-final-newline, broken,
deep-nesting, huge-line, invalid-utf8, nul-byte, many-matches) plus manifest.json.
Defaults: --lang all --depth 20000 --huge-bytes 1048576 --matches 70000.`);
  process.exit(code);
}

const args = process.argv.slice(2);
if (args.includes("--help") || args.includes("-h")) usage(0);
const opt = (name, def) => {
  const i = args.indexOf(name);
  if (i === -1) return def;
  const v = args[i + 1];
  if (v === undefined || v.startsWith("--")) usage(2);
  return v;
};
const out = opt("--out", null);
if (!out) usage(2);
const langArg = opt("--lang", "all");
const depth = Number(opt("--depth", "20000"));
const hugeBytes = Number(opt("--huge-bytes", String(1 << 20)));
const matches = Number(opt("--matches", "70000"));
for (const [k, v] of Object.entries({ depth, hugeBytes, matches })) {
  if (!Number.isInteger(v) || v < 1 || v > 50_000_000) {
    console.error(`invalid ${k}: ${v}`);
    process.exit(2);
  }
}
const langs = langArg === "all" ? Object.keys(LANGS) : langArg.split(",");
for (const l of langs) if (!LANGS[l]) { console.error(`unknown lang: ${l}`); process.exit(2); }

const manifest = [];
for (const key of langs) {
  const L = LANGS[key];
  const dir = join(resolve(out), key);
  mkdirSync(dir, { recursive: true });
  const write = (cls, data, note) => {
    const file = join(dir, `${cls}.${L.ext}`);
    writeFileSync(file, data);
    manifest.push({ lang: key, class: cls, file, bytes: Buffer.byteLength(data), note });
  };
  const body = [L.call('"a"'), L.call('"😀𝒳é"') + " " + L.call("x"), L.call("y")].join("\n");
  write("emoji", L.fn(body), "astral chars before a call on line 2: byte/UTF-16/char columns differ");
  write("crlf", L.fn(body).replace(/\n/g, "\r\n"), "CRLF endings: rows count \\n only");
  write("bom", Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from(L.call("z") + "\n" + L.fn(body))]), "UTF-8 BOM then a call on line 1: row-0 column must be 0 in UTF-16, not 3 (bytes) or 1");
  write("no-final-newline", L.fn(body).trimEnd(), "last line has no terminator");
  write("broken", L.broken, "ERROR/MISSING recovery");
  write("deep-nesting", L.fn(L.call(L.open.repeat(depth) + "1" + L.close.repeat(depth))), `${depth} nested parens: recursive walkers/oxc overflow`);
  const unit = L.call("x") + " ";
  write("huge-line", L.fn(unit.repeat(Math.ceil(hugeBytes / unit.length))), `~${hugeBytes} bytes on one line: O(line) column scans, size caps`);
  const [pre, post] = L.fn(L.call('"a@@"')).split("@@");
  write("invalid-utf8", Buffer.concat([Buffer.from(pre), Buffer.from([0xff, 0xfe, 0xc3]), Buffer.from(post)]), "invalid UTF-8 bytes inside a string literal: utf8_text/&str conversion must not panic; the call should still match");
  write("nul-byte", L.fn(L.call("a") + "\0" + L.call("b")), "NUL mid-file: TS scanner treats it as EOF");
  write("many-matches", L.fn(Array.from({ length: matches }, (_, i) => L.call(`v${i}`)).join("\n")), `${matches} calls: match-limit / result-cap handling`);
}
writeFileSync(join(resolve(out), "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
console.log(`wrote ${manifest.length} fixtures to ${resolve(out)} (manifest.json)`);
