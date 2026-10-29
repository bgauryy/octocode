#!/usr/bin/env node
// Context audit: what an agent harness loads into every session, what it
// costs, and what is never used. Zero dependencies.
//
//   node context-audit.mjs [--workspace DIR] [--days 30] [--all-projects]
//        [--no-probe] [--out DIR] [--json] [--html]
//   node context-audit.mjs --render context-audit.json [--out DIR]
//
// Collects (1) instruction files (CLAUDE.md, AGENTS.md, rules, memory index),
// (2) skill descriptions, (3) every configured MCP server's instructions and
// tool schemas (stdio servers are probed live with initialize + tools/list),
// (4) actual tool/skill usage from Claude Code transcripts. Writes
// context-audit.json; HTML is opt-in, including from an existing snapshot.
import fs from "node:fs";
import path from "node:path";
import { BUDGET, findings } from "./lib/audit-common.mjs";
import { collectInstructions, collectSkills } from "./lib/collect-local.mjs";
import { collectMcp } from "./lib/collect-mcp.mjs";
import { collectUsage, crossCheck } from "./lib/usage.mjs";
import { html, serverMeasurement } from "./lib/report-html.mjs";

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const option = (name, fallback) => {
  const index = args.indexOf(name);
  if (index < 0) return fallback;
  const value = args[index + 1];
  if (!value || value.startsWith("--")) throw new Error(`${name} requires a value`);
  return value;
};
if (flag("--help")) {
  console.log("Usage: node context-audit.mjs [--workspace DIR] [--days 30] [--all-projects] [--no-probe] [--out DIR] [--json] [--html]\n       node context-audit.mjs --render context-audit.json [--out DIR] [--json]");
  process.exit(0);
}
if (flag("--render")) {
  const source = option("--render");
  const report = JSON.parse(fs.readFileSync(path.resolve(source), "utf8"));
  const output = path.resolve(option("--out", path.dirname(path.resolve(source))), "context-audit.html");
  const page = html(report);
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, page);
  console.log(flag("--json") ? JSON.stringify({ output, render: "unverified" }) : `HTML generated; browser verification pending: ${output}`);
  process.exit(0);
}
const WORKSPACE = path.resolve(option("--workspace", process.cwd()));
const DAYS = Number(option("--days", "30"));
if (!Number.isInteger(DAYS) || DAYS < 0) throw new Error("--days requires a non-negative integer");
const OUT = path.resolve(option("--out", path.join(WORKSPACE, ".octocode", "context-audit")));
const PROBE = !flag("--no-probe");

const instructions = collectInstructions(WORKSPACE);
const skills = collectSkills(WORKSPACE);
const mcp = await collectMcp(WORKSPACE, PROBE);
const usage = await collectUsage(WORKSPACE, DAYS, flag("--all-projects"));
const unusedTools = crossCheck(mcp, skills, usage);
const report = { workspace: WORKSPACE, generatedAt: new Date().toISOString(), budgets: BUDGET, instructions, skills, mcp, usage, unusedTools, findings };
fs.mkdirSync(OUT, { recursive: true });
fs.writeFileSync(path.join(OUT, "context-audit.json"), JSON.stringify(report, null, 2));
if (flag("--html")) fs.writeFileSync(path.join(OUT, "context-audit.html"), html(report));
if (flag("--json")) {
  process.stdout.write(`${JSON.stringify(report)}\n`);
} else {
  const count = (severity) => findings.filter((f) => f.severity === severity).length;
  console.log(`context-audit: ${findings.length} findings (high ${count("high")}, medium ${count("medium")}, low ${count("low")})`);
  console.log(`  instructions always loaded ~${instructions.alwaysLoadedTokens} tok · skills ${skills.uniqueCount} (~${skills.descriptionTokens} tok) · MCP ${mcp.map((s) => `${s.name}: ${serverMeasurement(s)}`).join(", ") || "none"}`);
  console.log(`  usage: ${usage.sessions} sessions, ${usage.calls} tool calls in ${usage.days} days · unused MCP tools ${unusedTools.length}`);
  for (const f of findings.filter((f) => f.severity !== "low").slice(0, 15)) console.log(`  [${f.severity}] ${f.area} ${f.subject}: ${f.message}`);
  console.log(`  report: ${path.join(OUT, "context-audit.json")}`);
  if (flag("--html")) console.log(`  HTML generated; browser verification pending: ${path.join(OUT, "context-audit.html")}`);
}
