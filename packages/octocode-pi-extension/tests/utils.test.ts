/**
 * TDD tests for utils.ts
 *
 * These are pure-function tests — no I/O, no mocking required.
 */
import assert from 'node:assert/strict';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';
import {
  splitArgs,
  parseSetupScope,
  getAppendSystemTarget,
  truncateUserVisibleToolOutput,
  USER_VISIBLE_TOOL_PREVIEW_CHARS,
} from '../src/utils.js';

// ─── splitArgs ────────────────────────────────────────────────────────────────

test('splitArgs: splits plain whitespace-separated tokens', () => {
  assert.deepEqual(splitArgs('foo bar baz'), ['foo', 'bar', 'baz']);
});

test('splitArgs: handles double-quoted argument with internal spaces', () => {
  assert.deepEqual(splitArgs('"hello world" extra'), ['hello world', 'extra']);
});

test('splitArgs: handles single-quoted argument with internal spaces', () => {
  assert.deepEqual(splitArgs("'hello world' extra"), ['hello world', 'extra']);
});

test('splitArgs: unescapes backslash-escaped double quotes inside double-quoted arg', () => {
  assert.deepEqual(splitArgs('"he said \\"hi\\""'), ['he said "hi"']);
});

test('splitArgs: unescapes backslash-escaped single quotes inside single-quoted arg', () => {
  assert.deepEqual(splitArgs("'it\\'s fine'"), ["it's fine"]);
});

test('splitArgs: returns empty array for empty string', () => {
  assert.deepEqual(splitArgs(''), []);
});

test('splitArgs: handles multiple consecutive spaces gracefully', () => {
  assert.deepEqual(splitArgs('foo   bar'), ['foo', 'bar']);
});

test('splitArgs: handles flag-style tokens like --global', () => {
  assert.deepEqual(splitArgs('--global --verbose'), ['--global', '--verbose']);
});

// ─── parseSetupScope ─────────────────────────────────────────────────────────

test('parseSetupScope: --global flag returns "global"', () => {
  assert.equal(parseSetupScope('--global'), 'global');
});

test('parseSetupScope: "global" keyword returns "global"', () => {
  assert.equal(parseSetupScope('global'), 'global');
});

test('parseSetupScope: empty string defaults to "project"', () => {
  assert.equal(parseSetupScope(''), 'project');
});

test('parseSetupScope: "project" keyword returns "project"', () => {
  assert.equal(parseSetupScope('project'), 'project');
});

test('parseSetupScope: unrecognised token defaults to "project"', () => {
  assert.equal(parseSetupScope('--unknown-flag'), 'project');
});

test('parseSetupScope: --global among other args returns "global"', () => {
  assert.equal(parseSetupScope('install --global'), 'global');
});

// ─── getAppendSystemTarget ────────────────────────────────────────────────────

test('getAppendSystemTarget: global scope places file in home/.pi/agent/', () => {
  const home = '/fake/home';
  const result = getAppendSystemTarget('global', '/some/cwd', home);
  assert.equal(result, path.join(home, '.pi', 'agent', 'APPEND_SYSTEM.md'));
});

test('getAppendSystemTarget: project scope places file in cwd/.pi/', () => {
  const cwd = '/my/project';
  const result = getAppendSystemTarget('project', cwd, '/fake/home');
  assert.equal(result, path.join(cwd, '.pi', 'APPEND_SYSTEM.md'));
});

test('getAppendSystemTarget: uses process.cwd() default when cwd omitted (project scope)', () => {
  const result = getAppendSystemTarget('project');
  assert.equal(result, path.join(process.cwd(), '.pi', 'APPEND_SYSTEM.md'));
});

test('getAppendSystemTarget: uses os.homedir() default when homeDir omitted (global scope)', () => {
  const result = getAppendSystemTarget('global');
  assert.equal(result, path.join(os.homedir(), '.pi', 'agent', 'APPEND_SYSTEM.md'));
});

// ─── truncateUserVisibleToolOutput ────────────────────────────────────────────

test('truncateUserVisibleToolOutput: short text passes through unchanged', () => {
  const result = truncateUserVisibleToolOutput('short');
  assert.equal(result.text, 'short');
  assert.equal(result.truncated, false);
  assert.equal(result.omittedChars, 0);
});

test('truncateUserVisibleToolOutput: text at exact limit passes through unchanged', () => {
  const text = 'x'.repeat(USER_VISIBLE_TOOL_PREVIEW_CHARS);
  const result = truncateUserVisibleToolOutput(text);
  assert.equal(result.truncated, false);
  assert.equal(result.omittedChars, 0);
  assert.equal(result.text, text);
});

test('truncateUserVisibleToolOutput: text one char over limit is truncated', () => {
  const text = 'x'.repeat(USER_VISIBLE_TOOL_PREVIEW_CHARS + 1);
  const result = truncateUserVisibleToolOutput(text);
  assert.equal(result.truncated, true);
  assert.equal(result.omittedChars, 1);
  assert.ok(result.text.endsWith('…'));
  assert.equal(result.text.length, USER_VISIBLE_TOOL_PREVIEW_CHARS + 1); // limit + ellipsis
});

test('truncateUserVisibleToolOutput: long text truncated with correct omittedChars', () => {
  const text = 'x'.repeat(USER_VISIBLE_TOOL_PREVIEW_CHARS + 500);
  const result = truncateUserVisibleToolOutput(text);
  assert.equal(result.truncated, true);
  assert.equal(result.omittedChars, 500);
});

test('truncateUserVisibleToolOutput: null treated as empty string', () => {
  const result = truncateUserVisibleToolOutput(null);
  assert.equal(result.text, '');
  assert.equal(result.truncated, false);
});

test('truncateUserVisibleToolOutput: undefined treated as empty string', () => {
  const result = truncateUserVisibleToolOutput(undefined);
  assert.equal(result.text, '');
  assert.equal(result.truncated, false);
});

test('truncateUserVisibleToolOutput: custom maxChars respected', () => {
  const result = truncateUserVisibleToolOutput('abcdef', 3);
  assert.equal(result.text, 'abc…');
  assert.equal(result.truncated, true);
  assert.equal(result.omittedChars, 3);
});

test('truncateUserVisibleToolOutput: maxChars = 0 with non-empty text truncates to ellipsis only', () => {
  const result = truncateUserVisibleToolOutput('abc', 0);
  assert.equal(result.text, '\u2026');
  assert.equal(result.truncated, true);
  assert.equal(result.omittedChars, 3);
});
