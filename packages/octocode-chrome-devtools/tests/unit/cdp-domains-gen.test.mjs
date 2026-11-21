// tools/gen-cdp-domains.mjs regenerates docs/cdp-domains.md from a live Chrome's protocol.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  chromeBinary,
  startThrowawayChrome,
} from '../fixtures/throwaway-chrome.mjs';

const tool = join(import.meta.dirname, '../../tools/gen-cdp-domains.mjs');
const committed = join(import.meta.dirname, '../../docs/cdp-domains.md');
const skip = !chromeBinary && 'no Chrome installed';
const tableRows = text =>
  text
    .split('\n')
    .filter(line => /^\| [A-Z]/.test(line) && !line.startsWith('| Domain'));
const columns = row =>
  row
    .split('|')
    .slice(1, -1)
    .map(cell => cell.trim());

test(
  'the generated domain catalog covers every live domain with its enable flag',
  { skip, timeout: 60000 },
  async () => {
    const chrome = await startThrowawayChrome();
    const dir = mkdtempSync(join(tmpdir(), 'octo-cdp-domains-'));
    try {
      const out = join(dir, 'cdp-domains.md');
      const run = spawnSync(
        process.execPath,
        [tool, String(chrome.port), '--out', out],
        {
          encoding: 'utf8',
        }
      );
      assert.equal(run.status, 0, run.stderr);
      const text = readFileSync(out, 'utf8');
      assert.match(text, /^# CDP domains/);
      assert.ok(
        text.includes(chrome.version),
        'header names the Chrome version'
      );
      assert.ok(text.includes('## Parameterless getters'));
      const live = chrome.protocol.domains;
      const rows = tableRows(text).map(columns);
      assert.deepEqual(
        rows.map(row => row[0]).sort(),
        live.map(d => d.domain).sort()
      );
      for (const row of rows) {
        const domain = live.find(d => d.domain === row[0]);
        const hasEnable =
          domain.commands?.some(c => c.name === 'enable') ?? false;
        assert.equal(row[3] !== '-', hasEnable, `${row[0]} enable flag`);
        assert.equal(
          row[2],
          `${domain.commands?.length ?? 0} / ${domain.events?.length ?? 0}`
        );
        assert.ok(row[5].includes(`/tot/${row[0]}/`));
      }
      // The committed catalog lists the same domains and enable flags (Chrome versions may differ in members).
      const saved = tableRows(readFileSync(committed, 'utf8')).map(columns);
      const flags = list =>
        list.map(row => `${row[0]}:${row[3] !== '-'}`).sort();
      if (saved.length === rows.length)
        assert.deepEqual(flags(saved), flags(rows));
      assert.ok(saved.length > 0);
    } finally {
      await chrome.stop();
      rmSync(dir, { recursive: true, force: true });
    }
  }
);
