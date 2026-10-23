#!/usr/bin/env node
// Context audit: what an agent harness loads into every session, what it
// costs, and what is never used. Zero dependencies.
//
//   node context-audit.mjs [--workspace DIR] [--days 30] [--all-projects]
//        [--no-probe] [--out DIR] [--json]
//
// Collects (1) instruction files (CLAUDE.md, AGENTS.md, rules, memory index),
// (2) skill descriptions, (3) every configured MCP server's instructions and
// tool schemas (stdio servers are probed live with initialize + tools/list),
// (4) actual tool/skill usage from Claude Code transcripts. Writes
// context-audit.json and context-audit.html; prints a compact summary.
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
  return index >= 0 && args[index + 1] ? args[index + 1] : fallback;
};
const WORKSPACE = path.resolve(option("--workspace", process.cwd()));
const DAYS = Number(option("--days", "30"));
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
fs.writeFileSync(path.join(OUT, "context-audit.html"), html(report));
if (flag("--json")) {
  process.stdout.write(`${JSON.stringify(report)}\n`);
} else {
  const count = (severity) => findings.filter((f) => f.severity === severity).length;
  console.log(`context-audit: ${findings.length} findings (high ${count("high")}, medium ${count("medium")}, low ${count("low")})`);
  console.log(`  instructions always loaded ~${instructions.alwaysLoadedTokens} tok · skills ${skills.uniqueCount} (~${skills.descriptionTokens} tok) · MCP ${mcp.map((s) => `${s.name}: ${serverMeasurement(s)}`).join(", ") || "none"}`);
  console.log(`  usage: ${usage.sessions} sessions, ${usage.calls} tool calls in ${usage.days} days · unused MCP tools ${unusedTools.length}`);
  for (const f of findings.filter((f) => f.severity !== "low").slice(0, 15)) console.log(`  [${f.severity}] ${f.area} ${f.subject}: ${f.message}`);
  console.log(`  report: ${path.join(OUT, "context-audit.html")}`);
}
