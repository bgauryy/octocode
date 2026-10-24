import { sameSecret } from '../shared/env.js';
import { isRecord, parseJson, stringList } from '../shared/util.js';
import { auditLine } from './effect.js';
import { ACTION_CLASSES, ASK_REASONS, type ActionClass, type AskReason } from './mode.js';

/**
 * The first line of a subagent's permission prompt: what its parent needs to answer the prompt without a dialog when the
 * parent's own mode already allows the action. It is an optimization only: a title with no valid header is shown whole.
 * Built with `JSON.stringify`, so it is always one line; read from the first line of the title and nowhere else. It
 * carries the child's nonce (`PERMISSION_NONCE_FILE_ENV`), which a dialog the model raised never has: the parent accepts a
 * header only with the nonce it gave that child.
 */
export const HEADER_PREFIX = 'octocode-permission/1 ';

export interface PermissionHeader {
  action: ActionClass;
  /** Other classes the same call is: each must be allowed. */
  also: ActionClass[];
  /** Every reason on the call (a reason only tightens, so the parent checks each against its own mode). */
  reasons: AskReason[];
  /** The child's option labels, as the dialog offers them. */
  options: string[];
  /** The call's targets as the audit names them. */
  what: string[];
  /** The secret the parent gave this child. */
  nonce: string;
}

export const permissionHeader = (header: PermissionHeader): string => `${HEADER_PREFIX}${JSON.stringify(header)}`;

const subset = <T extends string>(value: unknown, allowed: readonly T[]): T[] | undefined => {
  const items = stringList(value);
  return items && items.every((item) => (allowed as readonly string[]).includes(item)) ? (items as T[]) : undefined;
};

/**
 * `title` without its header line, and the header, when the first line is a valid header that carries `nonce` and whose
 * `what` is what the dialog text below it shows (each target equals the audit form of a target line; a trailing
 * `+n more` is the overflow of a long list), so the parent audits what the user would have read. Any other title,
 * including one that only starts like a header, is returned whole: nothing a dialog says is hidden from the user.
 */
export function splitHeader(title: string, nonce: string): { title: string; header?: PermissionHeader } {
  if (!title.startsWith(HEADER_PREFIX)) return { title };
  const end = title.indexOf('\n');
  const line = end < 0 ? title : title.slice(0, end);
  const rest = end < 0 ? '' : title.slice(end + 1);
  const raw = parseJson(line.slice(HEADER_PREFIX.length));
  if (!isRecord(raw) || !sameSecret(nonce, raw['nonce'])) return { title };
  const action = ACTION_CLASSES.find((each) => each === raw['action']);
  const also = subset(raw['also'], ACTION_CLASSES);
  const reasons = subset(raw['reasons'], ASK_REASONS);
  const options = stringList(raw['options']);
  const what = stringList(raw['what']);
  if (!action || !also || !reasons || !options || !what || !showsTargets(rest, what)) return { title };
  return { title: rest, header: { action, also, reasons, options, what, nonce } };
}

/** `text` with every occurrence of the child's `nonce` replaced, so a header the parent rejected never puts the secret on screen (the rest of the text is shown whole). */
export const redactNonce = (text: string, nonce: string): string => (nonce === '' ? text : text.split(nonce).join('[nonce redacted]'));

/** Each target of `what` is the audit form of a target line (two-space indent) of the dialog `body`; only a final `+n more` is not. */
function showsTargets(body: string, what: readonly string[]): boolean {
  const shown = new Set(body.split('\n').filter((line) => line.startsWith('  ')).map((line) => auditLine(line.slice(2))));
  return what.every((target, index) => shown.has(target) || (index === what.length - 1 && /^\+\d+ more$/.test(target)));
}
