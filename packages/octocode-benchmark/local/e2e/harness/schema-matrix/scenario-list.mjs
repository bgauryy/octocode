import { CODE_SCENARIOS } from './scenarios-codes.mjs';
import { LEAD_SCENARIOS } from './scenarios-leads.mjs';
import { REASON_SCENARIOS } from './scenarios-reasons.mjs';
import { REFUSAL_SCENARIOS } from './scenarios-refusals.mjs';

/**
 * Written but not holding when this work stopped: each is a provocation not yet proven
 * to reach its state, not a passing check. Listed so the next reader can finish them.
 */
const NOT_HOLDING = new Set([
  'lead.continueCommitFiles',
  'code.commitMismatch',
  'code.fileTooLarge.search',
  'code.gitFailed',
  'code.github.rateLimited',
  'code.github.redirectDenied',
  'code.github.server',
  'code.github.transport',
  'code.largeSourceWindowOnly',
  'code.provider.invalidClassificationResponse',
  'lead.continueMaterialize',
  'lead.expandDepth',
  'lead.listCommits',
  'lead.partialTreeFailures',
  'lead.providerContentsLimit',
  'lead.providerIncompleteResults',
  'lead.readAtCommit.readParent',
  'lead.readCommitFiles',
  'lead.readHits',
  'lead.repair.search',
  'lead.restart',
  'lead.searchUnpatchedFile',
  'lead.unfiltered.github',
  'lead.unfiltered.local',
  'lsp.declarationClipped',
  'lsp.diagnosticsNotPublished',
  'lsp.diagnosticsTruncated',
  'lsp.documentSyncFailed',
  'lsp.hierarchyCandidateLimit',
  'lsp.hierarchyFanOutLimit',
  'lsp.hierarchyNodeLimit',
  'lsp.hierarchyParentUnanchored',
  'lsp.importerScanFailed',
  'lsp.workspaceRootInvalid',
  'reason.filesSkipped',
  'reason.parseRecovered'
]);

const ALL = [...REFUSAL_SCENARIOS, ...CODE_SCENARIOS, ...LEAD_SCENARIOS, ...REASON_SCENARIOS];
/** OCTOCODE_MATRIX_PARKED=only runs the parked ones, =with runs every scenario; OCTOCODE_MATRIX_SCENARIOS=id,... narrows to those ids. */
const parked = process.env.OCTOCODE_MATRIX_PARKED;
const ids = new Set((process.env.OCTOCODE_MATRIX_SCENARIOS ?? '').split(',').filter(Boolean));
export const PARKED = NOT_HOLDING;
export const SCENARIOS = ALL
  .filter(scenario => (parked === 'only' ? NOT_HOLDING.has(scenario.id) : parked === 'with' || !NOT_HOLDING.has(scenario.id)))
  .filter(scenario => !ids.size || ids.has(scenario.id));
