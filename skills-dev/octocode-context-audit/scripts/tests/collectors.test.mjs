import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, symlinkSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { collectInstructions } from "../lib/collect-local.mjs";
import { crossCheck } from "../lib/usage.mjs";
import { findings } from "../lib/audit-common.mjs";

const cli = resolve(import.meta.dirname, "../context-audit.mjs");

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "context-audit-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const home = join(root, "home"), workspace = join(root, "workspace");
  mkdirSync(home);
  mkdirSync(workspace);
  return { root, home, workspace };
}

function run({ home, workspace }, flags = []) {
  const result = spawnSync(process.execPath, [cli, "--workspace", workspace, "--out", join(workspace, "report"), "--json", ...flags], {
    env: { ...process.env, HOME: home }, encoding: "utf8", timeout: 5000,
  });
  return { ...result, report: result.status === 0 ? JSON.parse(result.stdout) : null };
}

test("missing Claude projects and malformed configuration leave a valid partial report", t => {
  const f = fixture(t);
  writeFileSync(join(f.workspace, ".mcp.json"), JSON.stringify({ mcpServers: { broken: null } }));
  const result = run(f, ["--all-projects"]);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.report.usage.sessions, 0);
  assert.equal(result.report.mcp[0].measurement, "failed");
  assert.match(result.report.mcp[0].error, /invalid server configuration/);
});

test("invalid MCP config is surfaced as incomplete inventory", t => {
  const f = fixture(t);
  writeFileSync(join(f.workspace, ".mcp.json"), "{broken");
  const result = run(f, ["--no-probe"]);
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(result.report.mcp, []);
  assert.ok(result.report.findings.some(finding => finding.area === "mcp" && /inventory incomplete/.test(finding.message)));
});

test("malformed transcript entries are ignored while valid calls survive", t => {
  const f = fixture(t);
  const project = join(f.home, ".claude", "projects", f.workspace.replace(/[^A-Za-z0-9]/g, "-"));
  mkdirSync(project, { recursive: true });
  const lines = ['{"tool_use":', JSON.stringify(null), JSON.stringify({ type: "progress", data: { message: "tool_use" } }), JSON.stringify({ type: "system", message: "progress text" }), JSON.stringify({ type: "user", message: { content: 'Discuss "tool_use" as text' } }), JSON.stringify({ type: "assistant", message: { content: { tool_use: true } } }), JSON.stringify({ type: "assistant", message: { content: [{ type: "tool_use", name: null, id: "bad" }, null] } }), JSON.stringify({ type: "user", message: { content: [{ type: "tool_result", tool_use_id: null }] } }), JSON.stringify({ message: { content: [{ type: "tool_use", name: "Bash", id: "1", input: { command: "rg foo" } }] } })];
  writeFileSync(join(project, "session.jsonl"), `${lines.join("\n")}\n`);
  const result = run(f, ["--no-probe"]);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.report.usage.calls, 1);
  assert.equal(result.report.usage.malformedLines, 5);
  assert.equal(result.report.usage.shell["text search (grep/rg)"], 1);
  assert.ok(result.report.findings.some(finding => finding.subject === "transcript coverage"));
});

test("absent transcript directory is empty, while directory and stat failures mark coverage incomplete", t => {
  const f = fixture(t);
  const absent = run(f, ["--all-projects", "--no-probe"]);
  assert.equal(absent.status, 0, absent.stderr);
  assert.equal(absent.report.usage.readFailures, 0);
  mkdirSync(join(f.home, ".claude"));
  writeFileSync(join(f.home, ".claude", "projects"), "not a directory");
  const badList = run(f, ["--all-projects", "--no-probe"]);
  assert.equal(badList.status, 0, badList.stderr);
  assert.equal(badList.report.usage.incomplete, true);
  rmSync(join(f.home, ".claude", "projects"));
  const project = join(f.home, ".claude", "projects", f.workspace.replace(/[^A-Za-z0-9]/g, "-"));
  mkdirSync(project, { recursive: true });
  symlinkSync(join(f.root, "missing.jsonl"), join(project, "gone.jsonl"));
  const badStat = run(f, ["--no-probe"]);
  assert.equal(badStat.status, 0, badStat.stderr);
  assert.equal(badStat.report.usage.readFailures, 1);
});

test("transcript stream failure marks coverage incomplete", t => {
  const f = fixture(t);
  const project = join(f.home, ".claude", "projects", f.workspace.replace(/[^A-Za-z0-9]/g, "-"));
  mkdirSync(project, { recursive: true });
  const file = join(project, "unreadable.jsonl");
  mkdirSync(file);
  const result = run(f, ["--no-probe"]);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.report.usage.readFailures, 1);
  assert.equal(result.report.usage.incomplete, true);
});

test("repeated MCP pagination cursor fails measurement instead of reporting partial tools", t => {
  const f = fixture(t);
  const server = join(f.root, "server.cjs");
  writeFileSync(server, `const readline=require('node:readline');readline.createInterface({input:process.stdin}).on('line',line=>{const x=JSON.parse(line);if(x.id){const result=x.method==='initialize'?{instructions:'fixture'}:{tools:[{name:'tool',inputSchema:{}}],nextCursor:'again'};process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:x.id,result})+'\\n')}});`);
  writeFileSync(join(f.workspace, ".mcp.json"), JSON.stringify({ mcpServers: { fixture: { command: process.execPath, args: [server] } } }));
  const result = run(f);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.report.mcp[0].measurement, "failed");
  assert.equal(result.report.mcp[0].totalChars, null);
  assert.deepEqual(result.report.mcp[0].tools, []);
  assert.match(result.report.mcp[0].error, /repeated cursor/);
});

test("probe stderr is not copied into findings or JSON", t => {
  const f = fixture(t);
  const server = join(f.root, "server.cjs");
  writeFileSync(server, "process.stderr.write('fixture-secret');process.exit(2)");
  writeFileSync(join(f.workspace, ".mcp.json"), JSON.stringify({ mcpServers: { fixture: { command: process.execPath, args: [server] } } }));
  const result = run(f);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.report.mcp[0].measurement, "failed");
  assert.ok(!result.stdout.includes("fixture-secret"));
});

test("synchronous spawn option errors do not expose config values", t => {
  const f = fixture(t);
  writeFileSync(join(f.workspace, ".mcp.json"), JSON.stringify({ mcpServers: { fixture: {
    command: process.execPath, env: { "BAD\u0000KEY": "fixture-secret" },
  } } }));
  const result = run(f);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.report.mcp[0].measurement, "failed");
  assert.ok(!result.stdout.includes("fixture-secret"));
});

test("invalid day windows fail before writing a report", t => {
  const f = fixture(t);
  for (const days of ["bad", "-1", "1.5"]) {
    const result = run(f, ["--days", days, "--no-probe"]);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /--days requires a non-negative integer/);
  }
});

test("nested repository inventory does not leak into a second workspace", t => {
  const f = fixture(t);
  const first = join(f.root, "first"), second = join(f.root, "second");
  mkdirSync(join(first, "nested", ".git"), { recursive: true });
  mkdirSync(second);
  assert.deepEqual(collectInstructions(first).nestedRepositoriesSkipped, ["nested"]);
  assert.deepEqual(collectInstructions(second).nestedRepositoriesSkipped, []);
});

test("incomplete transcript coverage suppresses absence and error-rate judgments", () => {
  const before = findings.length;
  const mcp = [{ name: "fixture", source: ".mcp.json", tools: [{ name: "tool", totalChars: 100 }], totalTokens: 25, totalChars: 100 }];
  const skills = { skills: [{ name: "fixture-skill", scope: "workspace", descriptionChars: 20 }] };
  const usage = { sessions: 1, days: 30, incomplete: true, tools: {}, errors: { failing: 5 }, skills: {}, servers: {}, shell: {} };
  assert.deepEqual(crossCheck(mcp, skills, usage), []);
  assert.equal(findings.length, before);
});


test("complete MCP pagination follows more than 100 pages", t => {
  const f = fixture(t);
  const server = join(f.root, "server.cjs");
  writeFileSync(server, `const readline=require('node:readline');readline.createInterface({input:process.stdin}).on('line',line=>{const x=JSON.parse(line);if(x.id){const page=Number(x.params?.cursor??0);const result=x.method==='initialize'?{instructions:'fixture'}:{tools:[{name:'tool'+page,inputSchema:{}}],...(page<100?{nextCursor:String(page+1)}:{})};process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:x.id,result})+'\\n')}});`);
  writeFileSync(join(f.workspace, ".mcp.json"), JSON.stringify({ mcpServers: { fixture: { command: process.execPath, args: [server] } } }));
  const result = run(f);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.report.mcp[0].measurement, "measured");
  assert.equal(result.report.mcp[0].toolCount, 101);
  assert.equal(result.report.mcp[0].tools.at(-1).name, "tool100");
});
