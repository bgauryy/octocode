// Related leads stay in their owning worker and scenario lifetime.
import { walkOn } from './runner.mjs';

export async function replayLeads(worker, ledger, { allow = [] } = {}) {
  const failures = [];
  while (worker.pending.length) {
    const lead = worker.pending.shift();
    const { report: walked } = await walkOn(worker, { surface: lead.surface, tool: lead.tool, input: lead.input, ledger });
    // Cache files appear under this tree while it is walked: a lead over it may rightly report a changed tree.
    for (const failure of walked.failures.filter(entry => !allow.includes(entry.check) && !/staleSnapshot|next\.restart/.test(String(entry.detail)))) failures.push({ id: `lead:${lead.from}->${lead.tool}`, tool: lead.tool, surface: lead.surface, check: `lead.${failure.check}`, detail: failure.detail, input: lead.input });
    const root = walked.root?.json;
    if (root?.errorCode === 'invalidInput' || root?.results?.some(row => row.data?.errorCode === 'invalidInput')) failures.push({ id: `lead:${lead.from}->${lead.tool}`, tool: lead.tool, surface: lead.surface, check: 'lead.rejected', detail: JSON.stringify(root).slice(0, 400), input: lead.input });
  }
  return failures;
}
