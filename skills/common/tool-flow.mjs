#!/usr/bin/env node
// Generates the route table of skills/octocode-research/references/tool-flow.md
// from the routes core serves as MCP instructions (the generated presentation
// `instructions.routes`), so the guide the MCP resource and `schema --view flow`
// serve lists exactly the served routes. That file is the guide's first part;
// the rest of it and the tool-flow-*.md parts served after it are hand-written.
//
//   node skills/common/tool-flow.mjs           rewrite the table
//   node skills/common/tool-flow.mjs --check   exit 1 when the table is stale
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isMainModule } from './scripts/lib/entrypoint.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const PRESENTATION = join(ROOT, 'packages/octocode-config/contract/cli-presentation.json');
export const TOOL_FLOW = join(ROOT, 'skills/octocode-research/references/tool-flow.md');
/** The heading whose first table is generated. */
export const ROUTE_HEADING = '## Route: where the evidence is';

const cell = text => text.replaceAll('|', '\\|');

/** The table rows for `instructions.routes`: each route's text, and the tools it needs listed. */
export function routeTable(routes) {
  const rows = routes.map(({ needs, text }) => {
    const route = text.replace(/^- /, '').trimEnd();
    const listed = needs.length ? needs.map(tool => `\`${tool}\``).join(', ') : 'always';
    return `| ${cell(route)} | ${listed} |`;
  });
  return ['| Route (server instructions §1) | Served when listed |', '|---|---|', ...rows];
}

/** The guide with the first table under {@link ROUTE_HEADING} replaced by the route table. */
export function withRouteTable(guide, routes) {
  const lines = guide.split('\n');
  const heading = lines.indexOf(ROUTE_HEADING);
  if (heading < 0) throw new Error(`tool-flow.md has no "${ROUTE_HEADING}" heading`);
  const start = lines.findIndex((line, index) => index > heading && line.startsWith('|'));
  const next = lines.findIndex((line, index) => index > heading && line.startsWith('## '));
  if (start < 0 || (next >= 0 && start > next)) throw new Error(`No table under "${ROUTE_HEADING}"`);
  let end = start;
  while (end < lines.length && lines[end].startsWith('|')) end += 1;
  return [...lines.slice(0, start), ...routeTable(routes), ...lines.slice(end)].join('\n');
}

/** Regenerates (or, with `check`, compares) the guide; returns whether it was stale. */
export function buildToolFlow({ check = false } = {}) {
  const { routes } = JSON.parse(readFileSync(PRESENTATION, 'utf8')).instructions;
  const guide = readFileSync(TOOL_FLOW, 'utf8');
  const next = withRouteTable(guide, routes);
  const stale = next !== guide;
  if (stale && !check) writeFileSync(TOOL_FLOW, next);
  return { stale, routes: routes.length };
}

if (isMainModule(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.some(arg => arg !== '--check')) {
    process.stderr.write('Usage: node skills/common/tool-flow.mjs [--check]\n');
    process.exit(args.includes('--help') ? 0 : 1);
  }
  const check = args.includes('--check');
  const { stale, routes } = buildToolFlow({ check });
  const file = relative(process.cwd(), TOOL_FLOW);
  process.stdout.write(`${file}: ${routes} routes; ${stale ? (check ? 'STALE (run node skills/common/tool-flow.mjs)' : 'UPDATED') : 'current'}.\n`);
  process.exitCode = check && stale ? 1 : 0;
}
