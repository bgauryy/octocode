/**
 * Text from subagents and MCP servers is untrusted: an escape sequence in it could retitle the terminal, write a
 * clickable link (OSC 8), move the cursor or reorder text with bidi overrides. Strip those before drawing it.
 * Adapted from the native composer's paste sanitiser.
 */
const OSC_SEQUENCE = /(?:\u001b\]|\u009d)[\s\S]*?(?:\u0007|\u001b\\|\u009c|$)/gu;
/** DCS, SOS, PM and APC strings. */
const TERMINAL_STRING_SEQUENCE = /(?:\u001b[P_X^]|[\u0090\u0098\u009e\u009f])[\s\S]*?(?:\u001b\\|\u009c|$)/gu;
const CSI_SEQUENCE = /(?:\u001b\[|\u009b)[0-?]*[ -/]*[@-~]/gu;
const ESC_SEQUENCE = /\u001b[ -/]*[0-~]/gu;
/** C0 and C1 controls except \t and \n (\r is folded into \n first). */
const UNSAFE_CONTROLS = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/gu;
const BIDI_FORMATTING = /[\u061c\u200e\u200f\u202a-\u202e\u2066-\u206f]/gu;
/**
 * Invisible characters that can smuggle text past a reader: Unicode tags (U+E0000–E007F), zero-width space and
 * non-joiner, word joiner and invisible operators (U+2060–2064), the BOM, and a zero-width joiner unless it joins
 * two emoji (👩‍💻 keeps its ZWJ).
 */
const INVISIBLE = /[\u200b\u200c\u2060-\u2064\ufeff\u{e0000}-\u{e007f}]|(?<!\p{Extended_Pictographic}[\u{1f3fb}-\u{1f3ff}\ufe0f]?)\u200d|\u200d(?!\p{Extended_Pictographic})/gu;

export function sanitizeTerminalText(value: string): string {
  // Fast path: most output is plain printable text.
  if (!/[\u0000-\u0008\u000b-\u001f\u007f-\u009f\u061c\u200b-\u200f\u202a-\u202e\u2028\u2029\u2060-\u2064\u2066-\u206f\ufeff\u{e0000}-\u{e007f}]/u.test(value)) return value;
  return value
    .replace(OSC_SEQUENCE, '')
    .replace(TERMINAL_STRING_SEQUENCE, '')
    .replace(CSI_SEQUENCE, '')
    .replace(ESC_SEQUENCE, '')
    .replace(BIDI_FORMATTING, '')
    .replace(INVISIBLE, '')
    .replace(/\r\n?/gu, '\n')
    .replace(/[\u2028\u2029]/gu, '\n')
    .replace(UNSAFE_CONTROLS, '');
}

/** Text as stored in a database: sanitized, trimmed and capped at `max` characters. */
export function storedText(text: string | undefined, max: number): string {
  return sanitizeTerminalText(text ?? '').trim().slice(0, max);
}

const SECRETS: Array<[RegExp, string]> = [
  [/\b(?:AKIA|ASIA)[0-9A-Z]{16}\b/, 'an AWS access key id'],
  [/\bghp_[A-Za-z0-9]{30,}|\bgithub_pat_[A-Za-z0-9_]{30,}|\bgh[ousr]_[A-Za-z0-9]{30,}/, 'a GitHub token'],
  [/\bsk-(?:proj-|ant-)?[A-Za-z0-9_-]{20,}/, 'an API secret key (sk-…)'],
  [/\bxox[bpas]-[A-Za-z0-9-]{10,}/, 'a Slack token'],
  [/-----BEGIN [A-Z ]*PRIVATE KEY-----/, 'a private key'],
  [/\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}/, 'a JWT'],
  [/\b[sr]k_(?:live|test)_[A-Za-z0-9]{16,}/, 'a Stripe key'],
  [/\bAIza[0-9A-Za-z_-]{35}/, 'a Google API key'],
  [/\bnpm_[A-Za-z0-9]{36}/, 'an npm token'],
  [/\bglpat-[A-Za-z0-9_-]{20,}/, 'a GitLab token'],
  // `scheme://user:password@host`; `${VAR}` or `<placeholder>` passwords are templates, not secrets.
  [/\b[a-z][a-z0-9+.-]*:\/\/[^\s:@/]+:[^\s@/${}<>]{3,}@/i, 'a password in a URL'],
  // `password = s3cretValue`: the value must hold a digit and no code punctuation, so `token = getToken()` passes.
  [/\b(?:password|passwd|secret|api[_-]?key|token)\s*[:=]\s*['"]?(?=[^\s'"(){}<>,;$]*\d)[^\s'"(){}<>,;$]{8,}/i, 'a password or key assignment'],
  [/\bBearer\s+[A-Za-z0-9._~+/-]{20,}/, 'a bearer token'],
];

/** Why `text` must not be stored (it looks like it holds a credential), or undefined when it looks clean. */
export function secretProblem(text: string): string | undefined {
  const hit = SECRETS.find(([pattern]) => pattern.test(text));
  return hit ? `it looks like it contains ${hit[1]}; never store secrets — refer to where the secret lives instead` : undefined;
}

/** `text` with every credential-looking span replaced by `[redacted]`, for text drawn on screen or shared with other sessions. */
export function redactSecrets(text: string): string {
  return SECRETS.reduce((out, [pattern]) => out.replace(new RegExp(pattern.source, `${pattern.flags.replace('g', '')}g`), '[redacted]'), text);
}
