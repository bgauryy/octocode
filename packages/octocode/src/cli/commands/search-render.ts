/**
 * Barrel for OQL search-result rendering. The implementation is split under
 * ./search-render/ by responsibility (envelope/plan/diagnostics, row/record
 * rendering, record-detail per recordType, continuation hints, shared value
 * helpers) to stay under the package's max-lines lint limit. This file exists
 * so external consumers (e.g. commands/search.ts) don't need to change their
 * import path.
 */
export {
  render,
  renderRawContent,
  renderRows,
} from './search-render/envelope.js';
