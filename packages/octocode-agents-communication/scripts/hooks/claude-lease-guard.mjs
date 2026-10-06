#!/usr/bin/env node
// Optional Claude structured-edit admission. Node is not required by the raw CLI.
import {isAbsolute} from 'node:path';
import {parseArgs} from 'node:util';
import {fileURLToPath} from 'node:url';

import {checkHostWrite} from './lease-check.mjs';
// Structured edit tools and the input field naming the file each one writes.
const EDIT_TOOLS = {Write: 'file_path', Edit: 'file_path', MultiEdit: 'file_path', NotebookEdit: 'notebook_path'};
const deny = reason => ({hookSpecificOutput: {hookEventName: 'PreToolUse', permissionDecision: 'deny', permissionDecisionReason: reason}});
const options = Object.fromEntries(['binary', 'workspace', 'database', 'session', 'host-session'].map(name => [name, {type: 'string'}]));
async function input() {
  const chunks = []; let bytes = 0;
  const timer = setTimeout(() => process.stdin.destroy(Error('Hook input timed out')), 1500);
  try {
    for await (const chunk of process.stdin) {
      bytes += chunk.length;
      if (bytes > 1024 * 1024) throw Error('Hook input exceeds 1 MiB');
      chunks.push(chunk);
    }
    return JSON.parse(Buffer.concat(chunks).toString('utf8'));
  } finally { clearTimeout(timer); }
}
function validate(values) {
  for (const key of Object.keys(options)) {
    if (typeof values[key] !== 'string' || !values[key] || /[\r\n\0]/.test(values[key])) throw Error(`Missing or invalid --${key}`);
  }
  for (const key of ['binary', 'workspace', 'database']) if (!isAbsolute(values[key])) throw Error(`--${key} must be absolute`);
}
function configuration(values) {
  validate(values);
  if (process.platform === 'win32') throw Error('This config preview requires a POSIX command shell');
  const quote = value => `'${value.replaceAll("'", "'\\''")}'`;
  const command = [process.execPath, fileURLToPath(import.meta.url), ...Object.keys(options).flatMap(key => [`--${key}`, values[key]])].map(quote).join(' ');
  process.stderr.write(JSON.stringify({type: 'leaseGuard', vendor: 'claude', configured: false,
    supportedOperations: Object.keys(EDIT_TOOLS), advisory: true,
    reason: 'Preview only: merge these synchronous hooks into the intended session. Bash/MCP/custom tools and OS writes are not fenced.'}) + '\n');
  return {hooks: {PreToolUse: [{matcher: `^(${Object.keys(EDIT_TOOLS).join('|')})$`, hooks: [{type: 'command', command, timeout: 10}]}]}};
}
async function guard(values) {
  const event = await input();
  // A broad host matcher must not unexpectedly gate read, shell, or custom tools.
  if (event?.hook_event_name !== 'PreToolUse' || !Object.hasOwn(EDIT_TOOLS, event?.tool_name)) return {};
  validate(values);
  if (event.session_id !== values['host-session']) throw Error('Wrong native session');
  const covered = await checkHostWrite(values, {vendorSession: event.session_id, cwd: event.cwd, path: event.tool_input?.[EDIT_TOOLS[event.tool_name]]});
  if (!covered) return deny('File edit blocked: acquire or renew your own covering communication lease, then retry.');
  // No permissionDecision:allow: leave ordinary host permission checks intact.
  return {};
}
try {
  const {values} = parseArgs({options: {...options, config: {type: 'boolean'}, help: {type: 'boolean'}}, strict: true});
  const result = values.help
    ? {usage: 'node claude-lease-guard.mjs [--config] --binary ABSOLUTE_CLI --workspace ABSOLUTE_REPO --database ABSOLUTE_DB --session DB_ID --host-session CLAUDE_ID', coverage: 'PreToolUse Write/Edit/MultiEdit/NotebookEdit only; raw CLI needs no Node. Config is a preview and writes no settings.'}
    : values.config ? configuration(values) : await guard(values);
  process.stdout.write(JSON.stringify(result) + '\n');
} catch {
  if (process.argv.includes('--config')) {
    process.stderr.write('Invalid lease guard configuration. Use --help for required absolute paths and session bindings.\n');
    process.exitCode = 1;
  } else process.stdout.write(JSON.stringify(deny('File edit blocked because live lease ownership or the host binding could not be verified.')) + '\n');
}
