import { lookup as dnsLookup } from 'node:dns/promises';
import net from 'node:net';
import { envFlag } from '../shared/env.js';

/** Redirect hops `web` follows; each target is checked before it is fetched. */
const MAX_REDIRECTS = 10;

export type HostLookup = (hostname: string) => Promise<Array<{ address: string }>>;

const defaultLookup: HostLookup = (hostname) => dnsLookup(hostname, { all: true, verbatim: true });

/** Non-public IPv4 ranges: this network, private, CGNAT, loopback, link-local (cloud metadata), protocol/test/benchmark, multicast, reserved. */
const V4_BLOCKS: ReadonlyArray<readonly [string, number]> = [
  ['0.0.0.0', 8], ['10.0.0.0', 8], ['100.64.0.0', 10], ['127.0.0.0', 8],
  ['169.254.0.0', 16], ['172.16.0.0', 12], ['192.0.0.0', 24], ['192.0.2.0', 24],
  ['192.88.99.0', 24], ['192.168.0.0', 16], ['198.18.0.0', 15], ['198.51.100.0', 24],
  ['203.0.113.0', 24], ['224.0.0.0', 4], ['240.0.0.0', 4],
];

function ipv4ToInt(ip: string): number | null {
  const parts = ip.split('.');
  if (parts.length !== 4 || parts.some((part) => !/^\d{1,3}$/.test(part))) return null;
  const values = parts.map(Number);
  if (values.some((value) => value > 255)) return null;
  return ((values[0]! << 24) >>> 0) + (values[1]! << 16) + (values[2]! << 8) + values[3]!;
}

function inV4(ip: number, base: string, bits: number): boolean {
  const mask = bits === 0 ? 0 : (0xffffffff << (32 - bits)) >>> 0;
  return ((ip & mask) >>> 0) === ((ipv4ToInt(base)! & mask) >>> 0);
}

function expandIpv6(ip: string): number[] | null {
  let value = ip.toLowerCase().replace(/%.*$/, '');
  const dotted = /^(.*:)(\d+\.\d+\.\d+\.\d+)$/.exec(value);
  if (dotted) {
    const v4 = ipv4ToInt(dotted[2]!);
    if (v4 === null) return null;
    value = `${dotted[1]}${((v4 >>> 16) & 0xffff).toString(16)}:${(v4 & 0xffff).toString(16)}`;
  }
  const halves = value.split('::');
  if (halves.length > 2) return null;
  const head = halves[0] ? halves[0].split(':') : [];
  const tail = halves.length === 2 && halves[1] ? halves[1].split(':') : [];
  const groups = halves.length === 1 ? head : [...head, ...Array<string>(Math.max(0, 8 - head.length - tail.length)).fill('0'), ...tail];
  if (groups.length !== 8) return null;
  const parsed = groups.map((group) => (/^[0-9a-f]{1,4}$/.test(group) ? Number.parseInt(group, 16) : Number.NaN));
  return parsed.some(Number.isNaN) ? null : parsed;
}

/**
 * The IPv4 addresses inside an IPv6 address that carries one: mapped (::ffff:a.b.c.d, always, so ::ffff:0.0.0.0 is
 * 0.0.0.0), translated (::ffff:0:a.b.c.d), deprecated compatible (::a.b.c.d, except :: and ::1), NAT64
 * (64:ff9b::/96), 6to4 (2002::/16) and Teredo (2001::/32: server, and the bit-inverted client address).
 */
function embeddedIpv4(groups: number[]): string[] {
  const dotted = (left: number, right: number) => `${left >>> 8}.${left & 255}.${right >>> 8}.${right & 255}`;
  const zero = (from: number, to: number) => groups.slice(from, to).every((group) => group === 0);
  const low = dotted(groups[6]!, groups[7]!);
  if (zero(0, 5) && groups[5] === 0xffff) return [low];
  if (zero(0, 4) && groups[4] === 0xffff && groups[5] === 0) return [low];
  if (zero(0, 6) && (groups[6] !== 0 || groups[7]! > 1)) return [low];
  if (groups[0] === 0x64 && groups[1] === 0xff9b && zero(2, 6)) return [low];
  if (groups[0] === 0x2002) return [dotted(groups[1]!, groups[2]!)];
  if (groups[0] === 0x2001 && groups[1] === 0) return [dotted(groups[2]!, groups[3]!), dotted(~groups[6]! & 0xffff, ~groups[7]! & 0xffff)];
  return [];
}

/** True for any address that is not a public unicast address (anything unparsable counts as blocked). */
export function isBlockedIp(ip: string): boolean {
  const version = net.isIP(ip.replace(/%.*$/, ''));
  if (version === 4) {
    const value = ipv4ToInt(ip)!;
    return V4_BLOCKS.some(([base, bits]) => inV4(value, base, bits));
  }
  if (version !== 6) return true;
  const groups = expandIpv6(ip);
  if (groups === null) return true;
  const embedded = embeddedIpv4(groups);
  if (embedded.length > 0) return embedded.some(isBlockedIp);
  const first = groups[0]!;
  return (
    groups.slice(0, 7).every((group) => group === 0) || // :: and ::1
    (first === 0x64 && groups[1] === 0xff9b) || // NAT64 local-use 64:ff9b:1::/48 and the rest of 64:ff9b::/32
    (first & 0xfe00) === 0xfc00 || // unique local fc00::/7
    (first & 0xffc0) === 0xfe80 || // link-local fe80::/10
    (first & 0xffc0) === 0xfec0 || // site-local fec0::/10
    (first & 0xff00) === 0xff00 || // multicast
    (first === 0x2001 && groups[1] === 0x0db8) // documentation
  );
}

/** `OCTOCODE_WEB_ALLOW_PRIVATE=1`: web fetches and browser navigation may reach private addresses without asking. */
export function allowPrivate(env: NodeJS.ProcessEnv = process.env): boolean {
  return envFlag(env, 'OCTOCODE_WEB_ALLOW_PRIVATE');
}

/**
 * Whether `url`'s host is, or resolves to, a non-public address (loopback, private, link-local cloud metadata, …).
 * A name that does not resolve is not private: the request then fails on its own.
 */
export async function isPrivateHost(url: URL, lookup: HostLookup = defaultLookup): Promise<boolean> {
  const hostname = url.hostname.replace(/^\[|\]$/g, '');
  if (net.isIP(hostname)) return isBlockedIp(hostname);
  const records = await lookup(hostname).catch(() => undefined);
  if (records === undefined) return false;
  return records.length === 0 || records.some(({ address }) => isBlockedIp(address));
}

/** Throws unless `value` is an http(s) URL whose host is, or resolves only to, public addresses. */
export async function assertPublicUrl(value: string | URL, lookup: HostLookup = defaultLookup): Promise<URL> {
  const url = new URL(value);
  if (url.protocol !== 'http:' && url.protocol !== 'https:') throw new Error(`web fetches http(s) URLs only, not ${url.protocol}`);
  const hostname = url.hostname.replace(/^\[|\]$/g, '');
  const blocked = () => new Error(`Blocked private or non-public address: ${url.host} (set OCTOCODE_WEB_ALLOW_PRIVATE=1 to allow; use the browser tool for local pages)`);
  if (net.isIP(hostname)) {
    if (isBlockedIp(hostname)) throw blocked();
    return url;
  }
  const records = await lookup(hostname);
  if (records.length === 0 || records.some(({ address }) => isBlockedIp(address))) throw blocked();
  return url;
}

/**
 * `fetch` that checks the URL and every redirect target before requesting it. The check resolves the host itself;
 * `fetch` resolves it again, so a DNS answer that changes in between (rebinding) is not caught: pinning the checked
 * address would need undici's dispatcher, which this package does not depend on.
 */
export async function publicFetch(url: string, init: RequestInit, options: { env?: NodeJS.ProcessEnv; lookup?: HostLookup; sameHost?: boolean } = {}): Promise<Response> {
  const anyAddress = allowPrivate(options.env ?? process.env);
  let target = new URL(url);
  for (let hop = 0; ; hop += 1) {
    if (!anyAddress) await assertPublicUrl(target, options.lookup);
    const response = await fetch(target, { ...init, redirect: 'manual' });
    const location = response.headers.get('location');
    if (response.status < 300 || response.status >= 400 || location === null) return response;
    await response.body?.cancel().catch(() => undefined);
    if (hop >= MAX_REDIRECTS) throw new Error(`Too many redirects fetching ${url}`);
    const next = new URL(location, target);
    if (options.sameHost && !sameSite(target, next)) throw new CrossHostRedirect(target, next, response.status);
    target = next;
  }
}

/** A redirect `publicFetch` did not follow because it leaves the host (with `sameHost`); the caller decides. */
export class CrossHostRedirect extends Error {
  constructor(
    readonly from: URL,
    readonly to: URL,
    readonly status: number,
  ) {
    super(`${from.href} redirects (HTTP ${status}) to another host: ${to.href}`);
  }
}

/** Same host (a leading `www.` ignored), and no downgrade from https to http. */
function sameSite(from: URL, to: URL): boolean {
  const host = (url: URL) => url.hostname.replace(/^www\./i, '').toLowerCase();
  return host(from) === host(to) && !(from.protocol === 'https:' && to.protocol === 'http:');
}
