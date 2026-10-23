import { ERR, MAX_FRAME_BYTES, RpcError } from './protocol.js';
import { redactData, redactSecrets } from '../shared/sanitize.js';
import { saveContentCopy, saveFullOutput } from '../shared/spill.js';
import { capChars } from '../shared/util.js';

const RESULT_MAX_CHARS = 64_000;

/**
 * Model text for an event or a history entry: credential-looking spans redacted first (so a secret that straddles the
 * limit is never left half-visible), then, over `RESULT_MAX_CHARS`, cut with the whole redacted text saved to a file
 * the note names (or the note says it could not be saved).
 */
export function boundedText(text: string): string {
  const safe = redactSecrets(text);
  if (safe.length <= RESULT_MAX_CHARS) return safe;
  // The same answer reaches `message`, `agent.end` and the history: the file is named by its content, so that is one file, not one per channel.
  const copy = saveContentCopy(safe, 'api message');
  return capChars(safe, RESULT_MAX_CHARS, copy ? `the whole text is in ${copy}` : 'the whole text could not be saved');
}

/** An external agent's result, redacted; over half a frame it is saved whole on the host and the client gets the file. */
export function boundedExternal(value: unknown): unknown {
  const safe = redactData(value);
  const text = JSON.stringify(safe);
  const bytes = Buffer.byteLength(text);
  if (bytes <= MAX_FRAME_BYTES / 2) return safe;
  const file = saveFullOutput(text, 'external-api-result');
  if (!file) throw new RpcError(ERR.unavailable, 'Complete external evidence could not be saved; execution may already have completed', { execution: 'uncertain', bytes });
  return { fullOutput: { path: file, bytes, format: 'json' }, notice: 'Complete redacted result saved on the Pi host; read this file before acting on the result.' };
}
