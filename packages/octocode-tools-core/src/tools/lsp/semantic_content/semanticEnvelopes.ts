// Barrel re-exporting the split semanticEnvelopes modules so existing
// imports of './semanticEnvelopes.js' keep working unchanged.
export {
  DEFAULT_SYMBOLS_PER_PAGE,
  DEFAULT_LOCATIONS_PER_PAGE,
  DEFAULT_CALLS_PER_PAGE,
  type PaginationInfo,
  emptyCategoryForReason,
  failedAnchorEnvelope,
  emptyEnvelope,
  paginateItems,
} from './semanticEnvelopes/envelopeHelpers.js';

export {
  locationsEnvelope,
  referencesEnvelope,
  hoverEnvelope,
  buildReferencesByFile,
  normalizeHover,
  stringifyHoverPart,
} from './semanticEnvelopes/locationEnvelopes.js';

export {
  callsEnvelope,
  typeHierarchyEnvelope,
} from './semanticEnvelopes/callEnvelopes.js';
