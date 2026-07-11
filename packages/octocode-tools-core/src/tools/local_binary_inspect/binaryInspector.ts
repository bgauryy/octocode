// Thin barrel: registration/pagination helpers live in
// binaryInspector/helpers.ts and the per-mode handlers live in
// binaryInspector/handlers.ts. Kept here so external imports of
// './binaryInspector.js' keep working unchanged.
//
// Note: the format lane (inspect/strings) is fully native (octocode-engine)
// and needs no allowlisted command; the binutils commands `xxd`/`strings`
// were removed with the old identify/strings shell-outs.
import { promises as fs } from 'fs';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import type { BinaryInspectQuery } from './scheme.js';
import {
  TOOL_NAME,
  registerBinaryCommands,
} from './binaryInspector/helpers.js';
import {
  handleInspect,
  handleList,
  handleExtract,
  handleDecompress,
  handleStrings,
  handleUnpack,
} from './binaryInspector/handlers.js';

export async function inspectBinary(query: BinaryInspectQuery) {
  registerBinaryCommands();

  const validation = validateToolPath(query, TOOL_NAME);
  if (!validation.isValid) return validation.errorResult;

  const filePath = validation.sanitizedPath;

  try {
    const stat = await fs.stat(filePath);
    if (!stat.isFile()) {
      return createErrorResult(
        `Path is not a regular file: ${filePath}`,
        query
      );
    }
  } catch {
    return createErrorResult(`File not found: ${filePath}`, query);
  }

  switch (query.mode) {
    case 'inspect':
      return handleInspect(filePath, query);
    case 'list':
      return handleList(filePath, query);
    case 'extract':
      return handleExtract(filePath, query);
    case 'decompress':
      return handleDecompress(filePath, query);
    case 'strings':
      return handleStrings(filePath, query);
    case 'unpack':
      return handleUnpack(filePath, query);
    default:
      return createErrorResult(
        `Unknown mode: ${String((query as BinaryInspectQuery).mode)}`,
        query
      );
  }
}
