/**
 * Refuse network requests to non-public hosts, so a page, a ledger row, or a prompt-injected agent cannot
 * steer a script at internal services or cloud metadata. Checks the literal host, every redirect hop, and
 * the DNS answer the connection actually uses (the check runs inside the socket's lookup, so a name that
 * answers differently the second time cannot slip through).
 * Source: skills/common/scripts/lib/net-guard.mjs; regenerate installed copies with skills/common/build.mjs.
 */
import { lookup } from 'node:dns/promises';
import { lookup as lookupCallback } from 'node:dns';
import { request as httpRequest } from 'node:http';
import { request as httpsRequest } from 'node:https';
import { isIP } from 'node:net';
import { Readable, pipeline } from 'node:stream';
import { createBrotliDecompress, createGunzip, createInflate } from 'node:zlib';

// Tests serve fixtures over plain HTTP on 127.0.0.1; nothing else may relax the rules.
const testLoopback = (hostname, protocol = 'http:') => process.env.OCTOCODE_NET_GUARD_TEST_LOOPBACK === '1' && protocol === 'http:' && hostname === '127.0.0.1';

// Non-public IPv4 blocks as [first octet, lowest second octet, highest second octet]; 224 and above are refused too.
const PRIVATE_V4 = [[0, 0, 255], [10, 0, 255], [127, 0, 255], [100, 64, 127], [169, 254, 254], [172, 16, 31], [192, 168, 168], [198, 18, 19]];

function ipv4Private(ip) {
  const [a, b] = ip.split('.').map(Number);
  return a >= 224 || PRIVATE_V4.some(([first, low, high]) => a === first && b >= low && b <= high);
}

/** An IPv4-mapped IPv6 address (`::ffff:127.0.0.1` or `::ffff:7f00:1`) as dotted IPv4, else null. */
function mappedIpv4(ip) {
  const m = ip.toLowerCase().match(/^(?:0*:)*:?ffff:(.+)$/) || ip.toLowerCase().match(/^::ffff:(.+)$/);
  if (!m) return null;
  if (isIP(m[1]) === 4) return m[1];
  const hex = m[1].split(':');
  if (hex.length !== 2) return null;
  const [hi, lo] = hex.map((h) => parseInt(h, 16));
  if (!Number.isFinite(hi) || !Number.isFinite(lo)) return null;
  return [hi >> 8, hi & 255, lo >> 8, lo & 255].join('.');
}

/** True for loopback, private, link-local, shared, benchmarking, multicast, reserved, or unspecified addresses. */
export function privateAddress(raw) {
  const ip = String(raw || '').replace(/^\[|\]$/g, '').split('%')[0];
  if (isIP(ip) === 4) return ipv4Private(ip);
  if (isIP(ip) !== 6) return false;
  const mapped = mappedIpv4(ip);
  if (mapped) return ipv4Private(mapped);
  const h = ip.toLowerCase();
  return h === '::' || h === '::1' || /^f[cd]/.test(h) || /^fe[89ab]/.test(h) || h.startsWith('ff') || h.startsWith('64:ff9b:');
}

function privateName(host) {
  const h = host.toLowerCase().replace(/\.$/, '');
  return h === 'localhost' || h.endsWith('.localhost') || h.endsWith('.local') || h.endsWith('.internal') || h.endsWith('.home.arpa');
}

/**
 * Why a URL may not be fetched without resolving DNS, or null. `allowOrigin` exempts one origin the user
 * named explicitly (a local dev server) from the private-host rule; nothing reached from it inherits that.
 */
export function urlProblem(raw, { httpsOnly = false, allowOrigin = null } = {}) {
  let u;
  try { u = new URL(raw); } catch { return 'not a valid URL'; }
  if (testLoopback(u.hostname, u.protocol)) return null;
  if (!['http:', 'https:'].includes(u.protocol)) return 'only http and https URLs are accepted';
  if (httpsOnly && u.protocol !== 'https:') return 'only https URLs are accepted';
  if (u.username || u.password) return 'URLs with embedded credentials are refused';
  if (allowOrigin && u.origin === allowOrigin) return null;
  if (privateName(u.hostname) || privateAddress(u.hostname)) return 'private, local, or link-local hosts are refused';
  return null;
}

/** urlProblem plus a DNS check: every address the host resolves to must be public. */
export async function resolvedUrlProblem(raw, options = {}) {
  const literal = urlProblem(raw, options);
  if (literal) return literal;
  const { hostname, protocol, origin } = new URL(raw);
  if (testLoopback(hostname, protocol) || (options.allowOrigin && origin === options.allowOrigin)) return null;
  if (isIP(hostname.replace(/^\[|\]$/g, ''))) return null;
  let addresses;
  try { addresses = await lookup(hostname, { all: true, verbatim: true }); } catch (error) { return `DNS lookup failed for ${hostname}: ${error.code || error.message}`; }
  const inward = addresses.find((a) => privateAddress(a.address));
  return inward ? `${hostname} resolves to a private address (${inward.address})` : null;
}

/** A socket lookup that refuses private answers unless the origin is exempt; it is what the connection uses. */
export function connectLookup(exempt) {
  return (hostname, options, callback) => {
    lookupCallback(hostname, { ...options, all: true }, (error, addresses) => {
      if (error) return callback(error);
      const inward = exempt ? null : addresses.find((a) => privateAddress(a.address));
      if (inward) return callback(Object.assign(new Error(`${hostname} resolves to a private address (${inward.address})`), { code: 'EPRIVATEADDRESS' }));
      if (options.all) return callback(null, addresses);
      return callback(null, addresses[0].address, addresses[0].family);
    });
  };
}

const NULL_BODY = new Set([101, 103, 204, 205, 304]);
const DECODERS = { gzip: createGunzip, 'x-gzip': createGunzip, deflate: createInflate, br: createBrotliDecompress };

/** One HTTP(S) request through the guarded lookup, returned as a standard Response. */
function pinnedRequest(url, init, exempt) {
  const target = new URL(url);
  const headers = Object.fromEntries(new Headers(init.headers || {}).entries());
  if (!headers['accept-encoding']) headers['accept-encoding'] = 'gzip, deflate, br';
  const method = (init.method || 'GET').toUpperCase();
  const send = target.protocol === 'https:' ? httpsRequest : httpRequest;
  return new Promise((resolve, reject) => {
    const signal = init.signal;
    if (signal?.aborted) return reject(signal.reason);
    // ALPN as fetch() sends it; some hosts reject a TLS handshake that advertises no protocol.
    const tls = target.protocol === 'https:' ? { ALPNProtocols: ['http/1.1'] } : {};
    const req = send(target, { method, headers, agent: false, lookup: connectLookup(exempt), signal, ...tls }, (res) => {
      const responseHeaders = new Headers();
      for (let i = 0; i < res.rawHeaders.length; i += 2) responseHeaders.append(res.rawHeaders[i], res.rawHeaders[i + 1]);
      const status = res.statusCode || 0;
      if (method === 'HEAD' || NULL_BODY.has(status)) {
        res.resume();
        return resolve(new Response(null, { status, headers: responseHeaders }));
      }
      const decoder = DECODERS[(res.headers['content-encoding'] || '').trim().toLowerCase()];
      const stream = decoder ? pipeline(res, decoder(), () => {}) : res;
      resolve(new Response(Readable.toWeb(stream), { status, headers: responseHeaders }));
    });
    req.on('error', (error) => reject(signal?.aborted ? signal.reason : error));
    req.end();
  });
}

/**
 * fetch()-like request that checks the URL and every redirect hop, and refuses a private DNS answer at
 * connect time. Returns a standard Response with a `finalUrl` property, or throws an Error whose `guard`
 * property names the refused URL.
 */
const exemptOrigin = (url, allowOrigin) => {
  const { hostname, protocol, origin } = new URL(url);
  return testLoopback(hostname, protocol) || (allowOrigin && origin === allowOrigin);
};
const isRedirect = (res) => res.status >= 300 && res.status < 400 && res.headers.get('location');

export async function guardedFetch(url, init = {}, { maxRedirects = 5, httpsOnly = false, allowOrigin = null } = {}) {
  let current = url;
  for (let hop = 0; hop <= maxRedirects; hop += 1) {
    const problem = urlProblem(current, { httpsOnly, allowOrigin });
    const refuse = (reason) => Object.assign(new Error(`${hop ? 'redirect target refused' : 'refused'}: ${reason}`), { guard: current });
    if (problem) throw refuse(problem);
    const exempt = exemptOrigin(current, allowOrigin);
    let res;
    try { res = await pinnedRequest(current, init, exempt); } catch (error) {
      if (error.code === 'EPRIVATEADDRESS') throw refuse(error.message);
      throw error;
    }
    if (isRedirect(res)) {
      await res.body?.cancel?.().catch(() => {});
      current = new URL(res.headers.get('location'), current).href;
      continue;
    }
    Object.defineProperty(res, 'finalUrl', { value: current });
    return res;
  }
  throw new Error(`more than ${maxRedirects} redirects`);
}
