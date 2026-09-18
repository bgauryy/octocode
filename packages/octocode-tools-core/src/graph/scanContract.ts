import { posix } from 'node:path';
import type { GraphFactsScanResult } from '@octocodeai/octocode-engine';
import type { GraphCoverage, RawGraphFacts } from './types.js';

type ScanEntry = GraphFactsScanResult['entries'][number];
export const SUPPORTED_GRAPH_FACTS_SCHEMA_VERSION = 1;

export type DecodedGraphFacts<T> =
  | { ok: true; parsed: T }
  | {
      ok: false;
      code: 'facts-decode-failed' | 'facts-schema-unsupported';
      message: string;
    };

export interface DecodedGraphScan {
  entries: Array<{
    relativePath: string;
    parsed: RawGraphFacts;
    referenceCounts: ScanEntry['referenceCounts'];
  }>;
  diagnostics: GraphCoverage['diagnostics'];
  filesSkipped: number;
}

const normalizeRelativePath = (path: string): string =>
  posix.normalize(path.split('\\').join('/'));

export function decodeGraphFactsJson<T = RawGraphFacts>(
  factsJson: string
): DecodedGraphFacts<T> {
  try {
    const parsed = JSON.parse(factsJson) as T & { schemaVersion?: number };
    if (
      parsed.schemaVersion !== undefined &&
      parsed.schemaVersion !== SUPPORTED_GRAPH_FACTS_SCHEMA_VERSION
    ) {
      return {
        ok: false,
        code: 'facts-schema-unsupported',
        message: `unsupported graph-fact schema version: ${parsed.schemaVersion}`,
      };
    }
    return { ok: true, parsed };
  } catch {
    return {
      ok: false,
      code: 'facts-decode-failed',
      message: 'native graph facts could not be decoded',
    };
  }
}

/** Validate the versioned native envelope before graph policy consumes it. */
export function decodeGraphScanResult(
  scanResult: GraphFactsScanResult
): DecodedGraphScan {
  if (
    scanResult.schemaVersion !== undefined &&
    scanResult.schemaVersion !== SUPPORTED_GRAPH_FACTS_SCHEMA_VERSION
  ) {
    return {
      entries: [],
      diagnostics: [
        {
          file: '.',
          code: 'facts-schema-unsupported',
          message: `unsupported graph scan schema version: ${scanResult.schemaVersion}`,
        },
      ],
      filesSkipped: scanResult.candidatePaths.length,
    };
  }
  const diagnostics: GraphCoverage['diagnostics'] = (
    scanResult.skipped ?? []
  ).map(diagnostic => ({
    file: normalizeRelativePath(diagnostic.relativePath),
    code: 'scan-skip',
    message: `${diagnostic.code}: ${diagnostic.message}`,
  }));
  const entries: DecodedGraphScan['entries'] = [];
  let filesSkipped = scanResult.filesSkipped;

  for (const entry of scanResult.entries) {
    const relativePath = normalizeRelativePath(entry.relativePath);
    const decoded = decodeGraphFactsJson(entry.factsJson);
    if (decoded.ok === false) {
      filesSkipped++;
      diagnostics.push({
        file: relativePath,
        code: decoded.code,
        message: decoded.message,
      });
      continue;
    }
    entries.push({
      relativePath,
      parsed: decoded.parsed,
      referenceCounts: entry.referenceCounts,
    });
  }

  return { entries, diagnostics, filesSkipped };
}
