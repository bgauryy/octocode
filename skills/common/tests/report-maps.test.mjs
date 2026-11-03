import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { runInNewContext } from 'node:vm';
import { mdKit } from '../scripts/lib/report/markdown.mjs';
import { collect, resolveRefs, skillFor } from '../scripts/lib/report/workspace.mjs';

const names = ['__proto__', 'constructor', 'toString', 'hasOwnProperty'];
const kit = runInNewContext(`(${mdKit.toString()})`)();

function workspace(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'report-maps-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}

test('undefined Markdown reference names never become inherited links', () => {
  const html = kit.parse(names.map(name => `[Label][${name}]`).join('\n\n'));
  assert.doesNotMatch(html, /<a\b/);
  assert.match(html, /\[Label\]\[constructor\]/);
});

test('defined Markdown reference names preserve their own targets without creating others', () => {
  const links = names.map((name, index) => `[Label ${index}][${name}]`).join('\n\n');
  const definitions = names.map((name, index) => `[${name}]: https://example.com/${index} "Title ${index}"`).join('\n');
  const html = kit.parse(`${links}\n\n[Unknown][url]\n\n${definitions}`);
  for (const index of names.keys()) assert.ok(html.includes(`<a href="https://example.com/${index}" title="Title ${index}">Label ${index}</a>`));
  assert.equal((html.match(/<a\b/g) || []).length, names.length);
  assert.ok(html.includes('[Unknown][url]'));
});

test('undefined footnotes remain text even for prototype names', () => {
  const html = kit.parse(names.map(name => `[^${name}]`).join(' '));
  assert.doesNotMatch(html, /class="(?:fnref|footnotes)"/);
  for (const name of names) assert.ok(html.includes(`[^${name}]`));
});

test('defined footnotes retain the text of every prototype name', () => {
  const refs = names.map(name => `[^${name}]`).join(' ');
  const definitions = names.map((name, index) => `[^${name}]: Footnote text ${index}`).join('\n');
  const html = kit.parse(`${refs}\n\n${definitions}`);
  assert.equal((html.match(/class="fnref"/g) || []).length, names.length);
  for (const index of names.keys()) assert.ok(html.includes(`Footnote text ${index}`));
  assert.doesNotMatch(html, /\[object Object\]|native code/);
});

test('prototype names do not acquire a semantic evidence tone', () => {
  for (const name of names) assert.equal(kit.toneOf(name, 'Evidence'), '', name);
  assert.equal(kit.toneOf('High', 'Evidence'), 'green');
  assert.equal(kit.toneOf('High', 'Risk'), 'amber');
});

test('workspace fragment links resolve duplicate prototype headings to exact lines', t => {
  const root = workspace(t);
  fs.writeFileSync(path.join(root, 'target.md'), names.flatMap(name => [`## ${name}`, `## ${name}`]).join('\n'));
  const targets = names.flatMap(name => [`target.md#${name.toLowerCase()}`, `target.md#${name.toLowerCase()}-1`]);
  const refs = resolveRefs(targets.map(target => `[Section](${target})`).join('\n'), root, root);
  assert.equal(Object.keys(refs).length, targets.length);
  for (const [index, target] of targets.entries()) assert.equal(refs[target]?.line, index + 1, target);
});

test('ordinary prototype-named folders are not classified as skill folders', t => {
  const root = workspace(t);
  for (const name of names) {
    const dir = path.join(root, name, 'nested');
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'report.md'), '# Report');
  }
  const { files } = collect(root, { workspace: root });
  for (const file of files) {
    assert.equal(file.kind, '');
    assert.equal(file.group, path.posix.dirname(file.path));
    assert.equal(skillFor(file.source), '');
  }
  assert.equal(skillFor(path.join(root, 'octocode-research', 'constructor', 'report.md')), 'octocode-research');
});
