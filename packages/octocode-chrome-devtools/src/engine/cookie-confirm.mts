/**
 * Domain confirmation for cookie transfers out of a live browser. The ack flag alone can be
 * self-supplied by an agent; naming each domain makes the command show exactly what moves.
 */
import type { CookieRecord } from './cookie-jar.mjs';

/** Lowercase, no leading or trailing dot, IDN as punycode (the form cookies carry); unparsable input stays as typed. */
function normalizeDomain(raw: string) {
  const d = raw.trim().toLowerCase().replace(/^\./, '').replace(/\.$/, '');
  if (!d || /[*?\s/:]/.test(d)) return d;
  try {
    return new URL(`http://${d}`).hostname.replace(/\.$/, '');
  } catch {
    return d;
  }
}

// Second-level labels a country registry hands out directly (co.uk, com.au, ac.jp ...).
const REGISTRY_SECOND_LEVEL = new Set([
  'co',
  'com',
  'org',
  'net',
  'gov',
  'edu',
  'ac',
  'or',
  'ne',
  'go',
]);

/** A two-label name under a two-letter country code with a registry label (co.uk): a suffix, not a site. */
function isPublicSuffixLike(domain: string) {
  const labels = domain.split('.');
  return (
    labels.length === 2 &&
    /^[a-z]{2}$/.test(labels[1]) &&
    REGISTRY_SECOND_LEVEL.has(labels[0])
  );
}

/** Parses `--confirm-domains`; returns domains (lowercase, no leading dot) or a refusal message. */
export function parseConfirmDomains(
  raw: string
): { domains: string[] } | { error: string } {
  const domains = raw
    .split(',')
    .map(s => normalizeDomain(s))
    .filter(Boolean);
  if (!domains.length)
    return {
      error:
        'Refusing: reading cookies from a live browser needs --confirm-domains <comma list> naming the cookie domains to transfer (e.g. example.com,auth.example.com).',
    };
  for (const d of domains) {
    if (/[*?\s/:]/.test(d))
      return {
        error: `Refusing --confirm-domains entry "${d}": name plain domains only (no wildcards, schemes, ports or paths).`,
      };
    if (isPublicSuffixLike(d))
      return {
        error: `Refusing --confirm-domains entry "${d}": a public suffix would match every site under it; name a registrable domain.`,
      };
    if (!d.includes('.') && d !== 'localhost')
      return {
        error: `Refusing --confirm-domains entry "${d}": a bare top-level name would match a whole registry; name a registrable domain.`,
      };
  }
  return { domains: [...new Set(domains)] };
}

/** True when the cookie's domain is a confirmed domain or a subdomain of one. */
export function cookieDomainConfirmed(
  cookie: Pick<CookieRecord, 'domain'>,
  domains: readonly string[]
): boolean {
  const host = normalizeDomain(cookie.domain || '');
  return domains.some(d => host === d || host.endsWith(`.${d}`));
}
