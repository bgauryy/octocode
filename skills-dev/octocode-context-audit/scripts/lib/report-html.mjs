// HTML report and shared MCP measurement labels.
import path from "node:path";

const escape = (value) => String(value).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

function mcpCost(servers) {
  if (!servers.length) return "none";
  const measured = servers.filter(server => server.measurement === "measured");
  if (!measured.length) return "not measured";
  const cost = measured.reduce((sum, server) => sum + server.totalTokens, 0);
  const missing = servers.length - measured.length;
  return `~${cost.toLocaleString()} tok${missing ? ` measured; ${missing} unmeasured` : ""}`;
}

export const serverMeasurement = server => server.measurement === "measured"
  ? `${server.toolCount} tools · instructions ${server.instructionsChars} chars · total ~${server.totalTokens} tok`
  : `not measured (${server.skipped ?? server.error})`;

export function html(report) {
  const rank = { high: 0, medium: 1, low: 2 };
  const rows = [...report.findings].sort((a, b) => (rank[a.severity] ?? 3) - (rank[b.severity] ?? 3) || (b.chars ?? 0) - (a.chars ?? 0));
  const table = (headers, body) => `<table><thead><tr>${headers.map((h) => `<th>${h}</th>`).join("")}</tr></thead><tbody>${body.join("")}</tbody></table>`;
  const bar = (value, max) => `<span class="bar" style="width:${Math.max(2, Math.round((100 * value) / Math.max(1, max)))}%"></span>`;
  const maxTool = Math.max(1, ...report.mcp.flatMap((server) => server.tools.map((tool) => tool.totalChars)));
  const maxFile = Math.max(1, ...report.instructions.files.map((file) => file.chars));
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Context Audit</title>
<style>
:root{--bg:#fbfaf7;--fg:#1d1c1a;--muted:#6b675f;--line:#e4e0d8;--accent:#b4532a;--high:#b42318;--medium:#b54708;--low:#475467;--barc:#d9a58a}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){--bg:#171614;--fg:#ecebe8;--muted:#a19d95;--line:#34312c;--accent:#e08a5f;--high:#f97066;--medium:#fdb022;--low:#98a2b3;--barc:#8a5a43;color-scheme:dark}}
:root[data-theme="dark"]{--bg:#171614;--fg:#ecebe8;--muted:#a19d95;--line:#34312c;--accent:#e08a5f;--high:#f97066;--medium:#fdb022;--low:#98a2b3;--barc:#8a5a43;color-scheme:dark}
body{background:var(--bg);color:var(--fg);font:14px/1.5 system-ui,sans-serif;margin:0;padding-block:24px;padding-inline:16px;max-width:1100px;margin-inline:auto}
h1{font-size:22px;margin:0 0 4px}h2{font-size:16px;margin:28px 0 8px;border-bottom:1px solid var(--line);padding-bottom:4px}
.muted{color:var(--muted)}.kpis{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:10px;margin:16px 0}
.kpi{border:1px solid var(--line);border-radius:8px;padding:10px}.kpi b{display:block;font-size:20px}
.wrap{overflow-x:auto}table{border-collapse:collapse;width:100%;font-size:13px}th,td{text-align:left;padding:5px 8px;border-bottom:1px solid var(--line);vertical-align:top}
th{color:var(--muted);font-weight:600}td.num{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
.sev{font-weight:700;text-transform:uppercase;font-size:11px}.high{color:var(--high)}.medium{color:var(--medium)}.low{color:var(--low)}
.bar{display:inline-block;height:8px;background:var(--barc);border-radius:2px;vertical-align:middle}td.barcell{width:30%}
code{font-size:12px;word-break:break-all}
</style></head><body>
<h1>Context audit</h1><div class="muted">${escape(report.workspace)} · ${escape(report.generatedAt)} · usage window ${escape(report.usage.days)} days, ${escape(report.usage.sessions)} sessions, ${escape(report.usage.calls)} tool calls</div>
<div class="kpis">
<div class="kpi"><span class="muted">Always-loaded instructions</span><b>~${escape(report.instructions.alwaysLoadedTokens.toLocaleString())} tok</b></div>
<div class="kpi"><span class="muted">Skill descriptions (${escape(report.skills.uniqueCount)})</span><b>~${escape(report.skills.descriptionTokens.toLocaleString())} tok</b></div>
<div class="kpi"><span class="muted">MCP servers (${report.mcp.length})</span><b>${escape(mcpCost(report.mcp))}</b></div>
<div class="kpi"><span class="muted">Unused MCP tools</span><b>${report.unusedTools.length}</b></div>
<div class="kpi"><span class="muted">Findings</span><b>${report.findings.length}</b></div>
</div>
<h2>Findings</h2><div class="wrap">${table(["Severity", "Area", "Subject", "Finding"], rows.map((f) => `<tr><td class="sev ${Object.hasOwn(rank, f.severity) ? f.severity : "unknown"}">${escape(f.severity)}</td><td>${escape(f.area)}</td><td><code>${escape(f.subject)}</code></td><td>${escape(f.message)}</td></tr>`))}</div>
<h2>MCP tools</h2><div class="wrap">${table(["Server/tool", "Total chars", "", "Desc", "Schema", "Calls", "Errors"], report.mcp.flatMap((server) => [
  `<tr><td colspan="7"><b>${escape(server.name)}</b> <span class="muted">${escape(server.source)} · ${escape(serverMeasurement(server))}</span></td></tr>`,
  ...[...server.tools].sort((a, b) => b.totalChars - a.totalChars).map((tool) => `<tr><td><code>${escape(tool.name)}</code></td><td class="num">${escape(tool.totalChars.toLocaleString())}</td><td class="barcell">${bar(tool.totalChars, maxTool)}</td><td class="num">${escape(tool.descriptionChars)}</td><td class="num">${escape(tool.schemaChars.toLocaleString())}</td><td class="num">${escape(tool.calls ?? "–")}</td><td class="num">${escape(tool.errors ?? "–")}</td></tr>`),
]))}</div>
<h2>Instruction files</h2><div class="wrap">${table(["File", "Loaded", "Chars", ""], [...report.instructions.files].sort((a, b) => b.chars - a.chars).map((file) => `<tr><td><code>${escape(path.relative(report.workspace, file.path) || file.path)}</code></td><td>${escape(file.loaded)}</td><td class="num">${escape(file.chars.toLocaleString())}</td><td class="barcell">${bar(file.chars, maxFile)}</td></tr>`))}</div>
<h2>Skills</h2><div class="wrap">${table(["Skill", "Scope", "Description chars", "Invocations"], [...report.skills.skills].sort((a, b) => b.descriptionChars - a.descriptionChars).map((skill) => `<tr><td><code>${escape(skill.name)}</code></td><td>${escape(skill.scope)}</td><td class="num">${escape(skill.descriptionChars)}</td><td class="num">${escape(skill.calls ?? "–")}</td></tr>`))}</div>
<h2>MCP usage by server (transcripts)</h2><div class="wrap">${table(["Server", "Calls", "Errors", "Tools called"], Object.entries(report.usage.servers).sort((a, b) => b[1].calls - a[1].calls).map(([name, server]) => `<tr><td><code>${escape(name)}</code></td><td class="num">${escape(server.calls)}</td><td class="num">${escape(server.errors)}</td><td>${Object.entries(server.tools).sort((a, b) => b[1] - a[1]).map(([tool, count]) => `${escape(tool)} ${escape(count)}`).join(" · ")}</td></tr>`))}</div>
<h2>Shell calls by kind</h2><div class="wrap">${table(["Kind", "Calls"], Object.entries(report.usage.shell ?? {}).slice(0, 20).map(([kind, count]) => `<tr><td>${escape(kind)}</td><td class="num">${escape(count)}</td></tr>`))}</div>
<h2>Most used tools</h2><div class="wrap">${table(["Tool", "Calls", "Errors"], Object.entries(report.usage.tools).slice(0, 30).map(([name, count]) => `<tr><td><code>${escape(name)}</code></td><td class="num">${escape(count)}</td><td class="num">${escape(report.usage.errors[name] ?? 0)}</td></tr>`))}</div>
<p class="muted">Tokens ≈ chars ÷ 4. Budgets are review points, not errors. Usage comes from Claude Code transcripts only.</p>
</body></html>`;
}
