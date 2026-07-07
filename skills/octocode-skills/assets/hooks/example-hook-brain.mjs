#!/usr/bin/env node
// Template hook "brain" script — see references/hooks.md before adapting.
//
// Copy into the target skill's scripts/ (e.g. scripts/example-hook-brain.mjs)
// and point a scripts/hooks/<name>.sh wrapper at it. This script owns the real
// decision; the shell wrapper only locates and execs it.
//
// Usage: node example-hook-brain.mjs <subcommand>
//   check   Example subcommand — rename/add subcommands per hook you wire up.
//
// Exit codes: 0 = allow, 2 = block (PreToolUse/Stop only), 1 = internal error
// (never a decision — hooks must fail open on their own bugs).

import { readFileSync } from 'node:fs';

const [subcommand] = process.argv.slice(2);

if (!subcommand || subcommand === '--help' || subcommand === '-h') {
  console.log(
    'Usage: node example-hook-brain.mjs <subcommand>\n' +
      '  check   Example subcommand — replace with the real check(s) this hook enforces.\n' +
      'Reads the host JSON payload from stdin. Exit 0 = allow, 2 = block, 1 = internal error.'
  );
  process.exit(subcommand ? 0 : 1);
}

function readPayload() {
  try {
    const raw = readFileSync(0, 'utf8');
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {}; // fail open: a missing/malformed payload must never block real work
  }
}

// Claude, Cursor, Codex, and Pi shape payloads differently — read whichever
// field is present instead of assuming one host.
function filePath(payload) {
  return (
    payload?.tool_input?.file_path ??
    payload?.file_path ??
    payload?.input?.path ??
    payload?.args?.path ??
    null
  );
}

const payload = readPayload();

switch (subcommand) {
  case 'check': {
    const path = filePath(payload);
    // TODO: replace with the real condition this hook enforces.
    const blocked = false;
    if (blocked) {
      console.error(`blocked: <reason> (${path ?? 'unknown target'})`);
      process.exit(2);
    }
    process.exit(0);
    break;
  }
  default:
    console.error(`unknown subcommand: ${subcommand}`);
    process.exit(1);
}
