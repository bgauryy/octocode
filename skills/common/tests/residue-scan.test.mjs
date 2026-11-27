import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { PATTERNS, scanText } from '../scripts/residue-scan.mjs';

const common = resolve(import.meta.dirname, '..');
const scanner = join(common, 'scripts/residue-scan.mjs');
const tells = readFileSync(join(common, 'references/writing-tells.md'), 'utf8');
const scan = (args, cwd) => spawnSync(process.execPath, [scanner, ...args], { cwd, encoding: 'utf8' });
const row = (family) => tells.split('\n').find((line) => line.startsWith(`| ${family} |`));

test('every pattern family is a writing-tells.md row, and each documented example of it is found', () => {
  const families = [...new Set(PATTERNS.map((p) => p.family))];
  assert.ok(families.length > 0);
  for (const family of families) {
    const examples = row(family)?.split('|')[2];
    assert.ok(examples, `${family} is a row of writing-tells.md`);
    // Backticked and quoted examples; a documented `N` stands for any number.
    const samples = [...examples.matchAll(/`([^`]+)`|"([^"]+)"/g)].map((m) => (m[1] ?? m[2]).replaceAll(':N', ':0').replace(/: N\]/, ': 0]'));
    assert.ok(samples.length > 0, family);
    for (const sample of samples) {
      const found = scanText(`Before ${sample} after.`, 'x.md');
      assert.deepEqual(found.map((f) => [f.family, f.match]), [[family, sample]], sample);
    }
  }
});

test('ordinary text, a real Markdown link and a format spec are not findings', () => {
  for (const text of ['See [your account](https://example.com/account).', 'Name it <YYYY-MM-DD>-<slug>.md.', 'utm_source=newsletter', 'Return to turn left.', 'Set YOUR_API_KEY in .env.']) {
    assert.deepEqual(scanText(text, 'x.md'), [], text);
  }
});

test('the CLI walks folders, skips dependency, git and binary files, reports lines, and exits 1 on findings', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'residue-scan-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const dir of ['docs', 'node_modules/pkg', '.git']) mkdirSync(join(root, dir), { recursive: true });
  writeFileSync(join(root, 'docs/a.md'), 'Clean line.\nSee https://x.dev/?utm_source=chatgpt.com and [cite: 2].\n');
  writeFileSync(join(root, 'docs/clean.md'), 'Nothing here.\n');
  writeFileSync(join(root, 'node_modules/pkg/readme.md'), 'utm_source=openai\n');
  writeFileSync(join(root, '.git/notes.md'), 'utm_source=openai\n');
  writeFileSync(join(root, 'docs/image.bin'), Buffer.from([0x75, 0x00, 0x74, 0x6d]));
  const hit = scan(['.'], root);
  assert.equal(hit.status, 1, hit.stderr);
  assert.deepEqual(JSON.parse(hit.stdout), {
    ok: false,
    files: 2,
    findings: [
      { file: 'docs/a.md', line: 2, family: 'Tool residue', match: 'utm_source=chatgpt.com' },
      { file: 'docs/a.md', line: 2, family: 'Tool residue', match: '[cite: 2]' },
    ],
  });
  const clean = scan(['docs/clean.md'], root);
  assert.equal(clean.status, 0);
  assert.deepEqual(JSON.parse(clean.stdout), { ok: true, files: 1, findings: [] });
  for (const args of [[], ['missing.md'], ['--fix', 'docs']]) {
    const bad = scan(args, root);
    assert.equal(bad.status, 2, args.join(' '));
    assert.equal(JSON.parse(bad.stdout).error.code, 'invalidInput');
  }
  const help = scan(['--help'], root);
  assert.equal(help.status, 0);
  for (const { family, name } of PATTERNS) assert.ok(help.stdout.includes(`${family}: ${name}`), name);
});
