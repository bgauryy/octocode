// `save` on evaluate, cdp and extract steps: the step's raw result lands under <run folder>/saved/<name>,
// so a later search or read of that file finds the text a step produced.
import { mkdirSync, writeFileSync } from 'node:fs';
import { extname, join, relative, resolve, sep } from 'node:path';
import { scrubTyped, typedValues } from './plan-redact.mjs';
import type { PlanState, StepContext, StepRow } from './plan-types.mjs';

/** A step's value waiting for the step to finish; a failed step never commits it. */
const staged = new WeakMap<StepRow, { value: unknown }>();

/** Records the value a successful op produced for its `save` file. No-op without `save`. */
export function stageSave({ step, row }: StepContext, value: unknown) {
  if (step.save !== undefined) staged.set(row, { value });
}

/** Text as saved: a string as-is, anything else pretty JSON with a trailing newline. */
function savedText(value: unknown, secrets: readonly string[]) {
  const clean = scrubTyped(value, secrets);
  if (typeof clean === 'string') return clean;
  return scrubTyped(`${JSON.stringify(clean ?? null, null, 2)}\n`, secrets);
}

// A raw CDP result whose top-level `data` is base64 (captureScreenshot, printToPDF) saves as the file itself.
const BINARY_EXTENSIONS = new Set([
  '.png',
  '.jpg',
  '.jpeg',
  '.webp',
  '.pdf',
  '.bin',
]);
const BASE64 =
  /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

/** The bytes (or mhtml text) a cdp step's `data` string saves as, by the file extension; undefined keeps the JSON form. */
function rawData(step: StepContext['step'], value: unknown) {
  if (step.op !== 'cdp' && step.op !== 'protocol') return undefined;
  const data = (value as { data?: unknown } | null)?.data;
  if (typeof data !== 'string') return undefined;
  const ext = extname(step.save ?? '').toLowerCase();
  if (ext === '.mhtml') return data;
  if (!BINARY_EXTENSIONS.has(ext)) return undefined;
  if (!BASE64.test(data))
    throw new Error(
      `save "${step.save}" needs a base64 \`data\` string; this result\x27s data is not base64 (omit the extension or use .json)`
    );
  return Buffer.from(data, 'base64');
}

/** Writes the staged value (typed secrets scrubbed) and prints its artifact line; the path stays inside saved/. */
export function commitSave(state: PlanState, { step, row }: StepContext) {
  const pending = staged.get(row);
  staged.delete(row);
  if (step.save === undefined || !pending) return;
  const dir = resolve(state.connection.outputDir, 'saved');
  const file = resolve(dir, step.save);
  const inside = relative(dir, file);
  if (!inside || inside.startsWith('..') || inside.includes(sep))
    throw new Error(`save "${step.save}" resolves outside saved/`);
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  const raw = rawData(step, pending.value);
  writeFileSync(
    join(dir, inside),
    raw ?? savedText(pending.value, typedValues(state.plan.steps)),
    { mode: 0o600 }
  );
  row.saved = file;
  state.log(`[ARTIFACT] SAVED_${step.save} ${file}`);
}
