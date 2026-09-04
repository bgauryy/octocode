import fs from 'node:fs';
import path from 'node:path';
import type { ContentPart, PiContext, ToolCallResult } from '../types.js';
import { chunkReadHint, writeEphemeralToolOutput } from './ephemeral-tool-output.js';
import { createSessionArtifactContext } from './session-artifacts.js';

/** Results above this size become reference-first instead of transcript-first. */
export const MODEL_VISIBLE_TOOL_RESULT_MAX_CHARS = 12_000;
/** Maximum diagnostic text retained from a heavy result (25% head, 75% tail). */
export const MODEL_VISIBLE_TOOL_RESULT_PREVIEW_CHARS = 4_000;
export const MODEL_VISIBLE_TOOL_RESULT_MAX_IMAGES = 2;
const ESTIMATED_IMAGE_CHARS = 4_800;
const TRUNCATION_NOTICE_RESERVE_CHARS = 1_000;

export interface ToolResultBudgetOptions {
  ctx?: PiContext;
  toolCallId: string;
  toolName: string;
  maxChars?: number;
  maxImages?: number;
}

function safePart(value: string): string {
  return value.replace(/[^a-zA-Z0-9._-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 80) || 'result';
}

function spillText(content: ContentPart[], options: ToolResultBudgetOptions): string | undefined {
  const text = content.flatMap((part) => part.type === 'text' ? [part.text] : []).join('\n\n');
  if (!text) return undefined;
  try {
    return writeEphemeralToolOutput(text, {
      toolName: options.toolName,
      toolCallId: options.toolCallId,
      extension: 'txt',
    });
  } catch {
    return undefined;
  }
}

function imageExtension(mimeType: string): string {
  const subtype = mimeType.toLowerCase().split('/')[1]?.split(/[;+]/)[0] ?? 'bin';
  if (subtype === 'jpeg') return 'jpg';
  return subtype.replace(/[^a-z0-9]/g, '').slice(0, 12) || 'bin';
}

function existingImagePaths(details: unknown, allowedRoots: string[]): string[] {
  const paths: string[] = [];
  const roots = allowedRoots.map((root) => fs.realpathSync.native(root));
  const visit = (value: unknown): void => {
    if (!value || typeof value !== 'object') return;
    if (Array.isArray(value)) {
      value.forEach(visit);
      return;
    }
    const record = value as Record<string, unknown>;
    const imagePath = typeof record['imagePath'] === 'string'
      ? record['imagePath']
      : record['type'] === 'image' && typeof record['sourcePath'] === 'string'
        ? record['sourcePath']
        : undefined;
    if (imagePath && fs.existsSync(imagePath)) {
      const resolved = fs.realpathSync.native(imagePath);
      if (roots.some((root) => {
        const relative = path.relative(root, resolved);
        return relative === '' || (!relative.startsWith('..') && !path.isAbsolute(relative));
      })) paths.push(resolved);
    }
    for (const [key, child] of Object.entries(record)) {
      if (key !== 'imagePath' && key !== 'sourcePath') visit(child);
    }
  };
  visit(details);
  return paths;
}

function spillImages(
  content: ContentPart[],
  keptImages: number,
  options: ToolResultBudgetOptions,
  details: unknown,
): string | undefined {
  if (!options.ctx) return undefined;
  const omitted = content.filter((part) => part.type === 'image').slice(keptImages);
  if (omitted.length === 0) return undefined;
  try {
    const artifacts = createSessionArtifactContext(options.ctx);
    const prefix = `${safePart(options.toolCallId)}-${safePart(options.toolName)}`;
    const existing = existingImagePaths(details, [options.ctx.cwd ?? process.cwd(), artifacts.root]);
    const images = omitted.map((part, offset) => {
      const existingPath = existing[keptImages + offset];
      if (existingPath) {
        return { index: keptImages + offset, mimeType: part.mimeType, bytes: Buffer.byteLength(part.data, 'base64'), path: existingPath };
      }
      const bytes = Buffer.from(part.data, 'base64');
      const relative = `tool-results/${prefix}-image-${keptImages + offset + 1}.${imageExtension(part.mimeType)}`;
      artifacts.writeBinary(relative, bytes);
      return { index: keptImages + offset, mimeType: part.mimeType, bytes: bytes.length, path: artifacts.resolve(relative) };
    });
    const relative = `tool-results/${prefix}-images.json`;
    artifacts.writeJson(relative, { version: 1, toolCallId: options.toolCallId, toolName: options.toolName, images });
    try {
      artifacts.registerProducer('log', relative);
    } catch {
      // The manifest and image files are already durable under the session root.
    }
    return artifacts.resolve(relative);
  } catch {
    return undefined;
  }
}

function safeSlice(text: string, start: number, end?: number): string {
  let from = start;
  let to = end ?? text.length;
  if (from > 0 && (text.charCodeAt(from) & 0xFC00) === 0xDC00) from += 1;
  if (to < text.length && to > 0 && (text.charCodeAt(to - 1) & 0xFC00) === 0xD800) to -= 1;
  return text.slice(from, to);
}

/** Bound provider-visible output while preserving omitted text/images by reference. */
export function budgetToolResult(
  result: ToolCallResult,
  options: ToolResultBudgetOptions,
): ToolCallResult {
  const maxChars = Math.max(1_000, options.maxChars ?? MODEL_VISIBLE_TOOL_RESULT_MAX_CHARS);
  const maxImages = Math.max(0, options.maxImages ?? MODEL_VISIBLE_TOOL_RESULT_MAX_IMAGES);
  const totalTextChars = result.content.reduce((sum, part) => sum + (part.type === 'text' ? part.text.length : 0), 0);
  const totalImages = result.content.filter((part) => part.type === 'image').length;
  const estimatedChars = totalTextChars + totalImages * ESTIMATED_IMAGE_CHARS;
  if (estimatedChars <= maxChars && totalImages <= maxImages) return result;

  const keptImageLimit = Math.min(
    totalImages,
    maxImages,
    Math.max(0, Math.floor((maxChars - TRUNCATION_NOTICE_RESERVE_CHARS) / ESTIMATED_IMAGE_CHARS)),
  );
  const spillPath = totalTextChars > 0 ? spillText(result.content, options) : undefined;
  const imageManifestPath = spillImages(result.content, keptImageLimit, options, result.details);
  const omittedImages = Math.max(0, totalImages - keptImageLimit);
  const notice = [
    `[heavy tool output referenced: inline budget=${maxChars} estimated chars; original text=${totalTextChars} chars`,
    spillPath ? `full text=${spillPath}` : totalTextChars > 0 ? 'rerun with a narrower query for omitted text' : '',
    omittedImages > 0 ? `omitted images=${omittedImages}` : '',
    imageManifestPath ? `image manifest=${imageManifestPath}` : omittedImages > 0 ? 'rerun to recover omitted images' : '',
  ].filter(Boolean).join('; ') + ']';
  const readHint = spillPath ? chunkReadHint(spillPath) : '';
  const available = Math.max(0, Math.min(
    MODEL_VISIBLE_TOOL_RESULT_PREVIEW_CHARS,
    maxChars - notice.length - readHint.length - 4 - keptImageLimit * ESTIMATED_IMAGE_CHARS,
  ));
  const allText = result.content.flatMap((part) => part.type === 'text' ? [part.text] : []).join('\n\n');
  const headChars = Math.floor(available / 4);
  const tailChars = available - headChars;
  const omittedText = Math.max(0, allText.length - headChars - tailChars);
  const preview = omittedText > 0
    ? `${safeSlice(allText, 0, headChars)}\n[… ${omittedText.toLocaleString()} chars omitted …]\n${safeSlice(allText, allText.length - tailChars)}`
    : allText;
  const content: ContentPart[] = preview ? [{ type: 'text', text: preview }] : [];
  let keptImages = 0;
  for (const part of result.content) {
    if (part.type === 'image' && keptImages < keptImageLimit) {
      content.push(part);
      keptImages += 1;
    }
  }
  content.push({ type: 'text', text: `${notice}${readHint ? `\n${readHint}` : ''}` });
  return { ...result, content };
}
