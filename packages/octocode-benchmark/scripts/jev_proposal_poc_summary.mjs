#!/usr/bin/env node
import fs from 'node:fs/promises';
import { isMainModule } from '../../../skills/common/scripts/lib/entrypoint.mjs';

const median = values => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted.length ? (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.ceil((sorted.length - 1) / 2)]) / 2 : null;
};

export function summarize(run) {
  const rows = run.results.filter(row => row.arm);
  return {
    mode: run.mode, split: run.split, totals: run.totals,
    metric: 'Retrospective source-line arrival; UTF-16 characters of serialized result.data rows, excluding outer envelopes and tool-call arguments. Not agent tokens or answer correctness.',
    arms: [...new Set(rows.map(row => row.arm))].map(arm => {
      const selected = rows.filter(row => row.arm === arm);
      const valid = selected.filter(row => row.coverage === true && row.page !== null && !row.error);
      const pairs = valid.map(row => [row, rows.find(other => other.case === row.case && other.arm === 'native-2k')])
        .filter(([, baseline]) => baseline?.coverage === true && baseline?.responseChars > 0);
      return { arm, attempted: selected.length, valid: valid.length, failed: selected.length - valid.length,
        wins: pairs.filter(([row, baseline]) => row.responseChars < baseline.responseChars).length,
        ties: pairs.filter(([row, baseline]) => row.responseChars === baseline.responseChars).length,
        losses: pairs.filter(([row, baseline]) => row.responseChars > baseline.responseChars).length,
        medianPairedResponseRatio: median(pairs.map(([row, baseline]) => row.responseChars / baseline.responseChars)),
        medianEvidencePage: median(valid.map(row => row.page)),
        providerCalls: selected.every(row => Number.isFinite(row.providerCalls)) ? selected.reduce((sum, row) => sum + row.providerCalls, 0) : null,
        inputTokens: selected.every(row => Number.isFinite(row.inputTokens) || row.providerCalls === 0)
          ? selected.reduce((sum, row) => sum + (row.inputTokens ?? 0), 0) : null,
        medianFullTraversalMs: selected.every(row => Number.isFinite(row.ms)) ? median(selected.map(row => row.ms)) : null };
    }),
    deterministic: run.mode === 'offline' ? run.results.map(({ current, ...row }) => ({ ...row, exit: current?.code ?? null })) : undefined,
  };
}

// argv[1] keeps a symlinked path while import.meta.url is the real path, so compare real paths.
if (isMainModule(import.meta.url)) {
  if (!process.argv[2]) throw new Error('Usage: node jev_proposal_poc_summary.mjs <run.json>');
  console.log(JSON.stringify(summarize(JSON.parse(await fs.readFile(process.argv[2], 'utf8'))), null, 2));
}
