import { contextUtils } from '../../utils/contextUtils.js';
import { recomputeMatchPositions } from './matchPositions.js';

/** Compact discovery context only when every match surviving redaction remains. */
export async function compactMatchedFragment(
  raw: string,
  sanitized: string,
  rawPositions: [number, number][],
  path: string
): Promise<{
  context: string;
  positions: [number, number][];
  minificationType?: string;
  failed?: boolean;
}> {
  const source = {
    context: sanitized,
    positions: recomputeMatchPositions(raw, rawPositions, sanitized),
  };
  try {
    const compact = await contextUtils.minifyContent(sanitized, path);
    if (compact.failed || compact.type === 'failed')
      return { ...source, failed: true };
    const positions = recomputeMatchPositions(
      raw,
      rawPositions,
      compact.content
    );
    if (positions.length !== source.positions.length) return source;
    return {
      context: compact.content,
      positions,
      minificationType: compact.type,
    };
  } catch {
    return { ...source, failed: true };
  }
}
