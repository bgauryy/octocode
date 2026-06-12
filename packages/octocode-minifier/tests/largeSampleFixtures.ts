/**
 * Authentic ~400-line code samples per language for large-file benchmarks.
 * Sources: real open-source projects (httpx, Spring, nlohmann/json, tokio,
 * rails, Laravel, kotlinx, swift-stdlib, etc.) with representative comment
 * density, docstrings, and blank-line patterns.
 */

// ---------------------------------------------------------------------------
// TypeScript  (~400 lines) — async HTTP service, real patterns from httpx/axios
// ---------------------------------------------------------------------------
export const TS_LARGE = `
import type { IncomingMessage, ServerResponse } from 'node:http';
import { EventEmitter } from 'node:events';
import { URL } from 'node:url';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** HTTP methods supported by the client. */
export type HttpMethod =
  | 'GET'
  | 'POST'
  | 'PUT'
  | 'PATCH'
  | 'DELETE'
  | 'HEAD'
  | 'OPTIONS';

/** Timeout configuration in milliseconds. */
export interface TimeoutConfig {
  /** Connection timeout. Defaults to 5 000 ms. */
  connect?: number;
  /** Read timeout after connection established. Defaults to 30 000 ms. */
  read?: number;
  /** Write timeout for request body. Defaults to 30 000 ms. */
  write?: number;
}

/** Retry policy applied to idempotent requests. */
export interface RetryPolicy {
  /** Maximum retry attempts. */
  maxRetries: number;
  /** Base delay between retries in ms. */
  baseDelayMs: number;
  /** Jitter factor 0-1 added to each delay. */
  jitter?: number;
  /** Status codes that trigger a retry. */
  retryOn?: number[];
}

export type HeadersInit =
  | Record<string, string>
  | [string, string][]
  | Headers;

/** Normalised HTTP response. */
export interface HttpResponse<T = unknown> {
  status: number;
  statusText: string;
  headers: Headers;
  body: T;
  elapsed: number;
  url: URL;
}

/** Request options accepted by \`HttpClient.request\`. */
export interface RequestOptions {
  method?: HttpMethod;
  headers?: HeadersInit;
  body?: BodyInit | null;
  timeout?: TimeoutConfig | number;
  retry?: RetryPolicy;
  signal?: AbortSignal;
  /** Follow redirects automatically. Defaults to true. */
  followRedirects?: boolean;
  /** Maximum redirects to follow. Defaults to 10. */
  maxRedirects?: number;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Merge two Header instances, with \`overrides\` taking precedence. */
function mergeHeaders(
  base: Headers,
  overrides: HeadersInit | undefined
): Headers {
  const merged = new Headers(base);
  if (!overrides) return merged;

  const entries =
    overrides instanceof Headers
      ? overrides
      : Array.isArray(overrides)
        ? overrides
        : Object.entries(overrides);

  for (const [key, value] of entries) {
    merged.set(key, value);
  }
  return merged;
}

/** Exponential-backoff delay with optional jitter. */
function backoffDelay(attempt: number, policy: RetryPolicy): number {
  const base = policy.baseDelayMs * 2 ** attempt;
  const jitter = policy.jitter ? base * policy.jitter * Math.random() : 0;
  return Math.min(base + jitter, 30_000);
}

/** Return true when the status is retryable according to the policy. */
function isRetryable(status: number, policy: RetryPolicy): boolean {
  const defaults = [429, 500, 502, 503, 504];
  const codes = policy.retryOn ?? defaults;
  return codes.includes(status);
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function normalizeTimeout(t: TimeoutConfig | number | undefined): TimeoutConfig {
  if (t === undefined) return { connect: 5_000, read: 30_000, write: 30_000 };
  if (typeof t === 'number') return { connect: t, read: t, write: t };
  return { connect: 5_000, read: 30_000, write: 30_000, ...t };
}

// ---------------------------------------------------------------------------
// HttpClient
// ---------------------------------------------------------------------------

const DEFAULT_RETRY: RetryPolicy = {
  maxRetries: 3,
  baseDelayMs: 200,
  jitter: 0.25,
  retryOn: [429, 500, 502, 503, 504],
};

/**
 * Thin, transport-agnostic HTTP client for browser and Node environments.
 *
 * @example
 * const client = new HttpClient({ baseUrl: 'https://api.example.com' });
 * const user = await client.get<User>('/users/1');
 */
export class HttpClient extends EventEmitter {
  private readonly baseUrl: URL | null;
  private readonly defaultHeaders: Headers;
  private readonly defaultTimeout: TimeoutConfig;
  private readonly defaultRetry: RetryPolicy | null;

  constructor(options: {
    baseUrl?: string;
    headers?: HeadersInit;
    timeout?: TimeoutConfig | number;
    retry?: RetryPolicy | false;
  } = {}) {
    super();

    this.baseUrl = options.baseUrl ? new URL(options.baseUrl) : null;
    this.defaultHeaders = new Headers(options.headers);
    this.defaultTimeout = normalizeTimeout(options.timeout);
    this.defaultRetry = options.retry === false ? null : (options.retry ?? DEFAULT_RETRY);

    // Ensure a sane default Content-Type for POST/PUT/PATCH.
    if (!this.defaultHeaders.has('content-type')) {
      this.defaultHeaders.set('content-type', 'application/json');
    }
  }

  // -------------------------------------------------------------------------
  // Public API
  // -------------------------------------------------------------------------

  /** Issue a GET request and decode the JSON body as \`T\`. */
  get<T = unknown>(path: string, opts?: Omit<RequestOptions, 'method' | 'body'>): Promise<HttpResponse<T>> {
    return this.request<T>(path, { ...opts, method: 'GET' });
  }

  /** Issue a POST request with an optional JSON body. */
  post<T = unknown>(path: string, body?: unknown, opts?: Omit<RequestOptions, 'method'>): Promise<HttpResponse<T>> {
    return this.request<T>(path, {
      ...opts,
      method: 'POST',
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
  }

  /** Issue a PUT request. */
  put<T = unknown>(path: string, body?: unknown, opts?: Omit<RequestOptions, 'method'>): Promise<HttpResponse<T>> {
    return this.request<T>(path, {
      ...opts,
      method: 'PUT',
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
  }

  /** Issue a PATCH request. */
  patch<T = unknown>(path: string, body?: unknown, opts?: Omit<RequestOptions, 'method'>): Promise<HttpResponse<T>> {
    return this.request<T>(path, {
      ...opts,
      method: 'PATCH',
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
  }

  /** Issue a DELETE request. */
  delete<T = unknown>(path: string, opts?: Omit<RequestOptions, 'method' | 'body'>): Promise<HttpResponse<T>> {
    return this.request<T>(path, { ...opts, method: 'DELETE' });
  }

  // -------------------------------------------------------------------------
  // Core dispatch
  // -------------------------------------------------------------------------

  async request<T = unknown>(
    path: string,
    options: RequestOptions = {}
  ): Promise<HttpResponse<T>> {
    const url = this.resolveUrl(path);
    const method: HttpMethod = options.method ?? 'GET';
    const headers = mergeHeaders(this.defaultHeaders, options.headers);
    const timeout = normalizeTimeout(options.timeout ?? this.defaultTimeout);
    const retry = options.retry ?? this.defaultRetry;

    let attempt = 0;
    const maxAttempts = retry ? retry.maxRetries + 1 : 1;

    while (attempt < maxAttempts) {
      const controller = new AbortController();
      const connectTimer = setTimeout(
        () => controller.abort(),
        timeout.connect ?? 5_000
      );

      const mergedSignal = options.signal
        ? AbortSignal.any([options.signal, controller.signal])
        : controller.signal;

      const start = performance.now();

      try {
        const raw = await fetch(url.toString(), {
          method,
          headers,
          body: options.body ?? null,
          signal: mergedSignal,
          redirect: options.followRedirects !== false ? 'follow' : 'manual',
        });

        clearTimeout(connectTimer);
        const elapsed = performance.now() - start;

        // Parse body
        const contentType = raw.headers.get('content-type') ?? '';
        let body: T;
        if (contentType.includes('application/json')) {
          body = (await raw.json()) as T;
        } else if (contentType.startsWith('text/')) {
          body = (await raw.text()) as unknown as T;
        } else {
          body = (await raw.arrayBuffer()) as unknown as T;
        }

        const response: HttpResponse<T> = {
          status: raw.status,
          statusText: raw.statusText,
          headers: raw.headers,
          body,
          elapsed,
          url,
        };

        this.emit('response', response);

        // Retry on configured status codes.
        if (retry && attempt < retry.maxRetries && isRetryable(raw.status, retry)) {
          const delay = backoffDelay(attempt, retry);
          this.emit('retry', { attempt, status: raw.status, delay });
          await sleep(delay);
          attempt++;
          continue;
        }

        return response;
      } catch (err: unknown) {
        clearTimeout(connectTimer);

        if ((err as Error).name === 'AbortError') {
          throw new Error(\`Request timed out after \${timeout.connect}ms: \${url}\`);
        }

        if (attempt < (retry?.maxRetries ?? 0)) {
          const delay = backoffDelay(attempt, retry!);
          this.emit('retry', { attempt, error: err, delay });
          await sleep(delay);
          attempt++;
          continue;
        }

        throw err;
      }
    }

    throw new Error('Exceeded maximum retry attempts');
  }

  // -------------------------------------------------------------------------
  // Utilities
  // -------------------------------------------------------------------------

  private resolveUrl(path: string): URL {
    if (!this.baseUrl) return new URL(path);
    // Avoid double-slash when path already starts with /.
    const base = this.baseUrl.toString().replace(/\\/$/, '');
    const tail = path.startsWith('/') ? path : '/' + path;
    return new URL(base + tail);
  }

  /** Build an HttpClient pre-configured with a Bearer token. */
  static withBearer(token: string, opts?: { baseUrl?: string }): HttpClient {
    return new HttpClient({
      ...opts,
      headers: { Authorization: \`Bearer \${token}\` },
    });
  }
}

export default HttpClient;
`;

// ---------------------------------------------------------------------------
// Python  (~400 lines) — from httpx/_client.py patterns + real docstrings
// ---------------------------------------------------------------------------
export const PY_LARGE = `
"""
httpx-style async HTTP client — adapted from the real httpx source.
"""
from __future__ import annotations

import datetime
import enum
import logging
import time
import typing
import warnings
from contextlib import asynccontextmanager, contextmanager
from types import TracebackType

logger = logging.getLogger(__name__)

USER_AGENT = "python-httpx/0.28"
DEFAULT_MAX_REDIRECTS = 20
DEFAULT_TIMEOUT = 5.0  # seconds


class ClientState(enum.Enum):
    UNOPENED = 1
    OPENED = 2
    CLOSED = 3


def _is_https_redirect(url: str, location: str) -> bool:
    """Return True if 'location' is an HTTPS upgrade of 'url'."""
    from urllib.parse import urlparse
    parsed_url = urlparse(url)
    parsed_loc = urlparse(location)
    if parsed_url.hostname != parsed_loc.hostname:
        return False
    return (
        parsed_url.scheme == "http"
        and (parsed_url.port or 80) == 80
        and parsed_loc.scheme == "https"
        and (parsed_loc.port or 443) == 443
    )


class Timeout:
    """
    Timeout configuration.

    Configuration via a dictionary:
        timeout = httpx.Timeout(connect=5.0, read=10.0, write=10.0)

    Using a single float:
        timeout = httpx.Timeout(5.0)
    """

    def __init__(
        self,
        timeout: typing.Union[None, float, "Timeout"] = None,
        *,
        connect: typing.Optional[float] = None,
        read: typing.Optional[float] = None,
        write: typing.Optional[float] = None,
        pool: typing.Optional[float] = None,
    ) -> None:
        if isinstance(timeout, Timeout):
            # Prefer instance copy constructor.
            self.connect = timeout.connect
            self.read = timeout.read
            self.write = timeout.write
            self.pool = timeout.pool
        elif isinstance(timeout, (int, float)) or timeout is None:
            self.connect = connect if connect is not None else timeout
            self.read = read if read is not None else timeout
            self.write = write if write is not None else timeout
            self.pool = pool if pool is not None else timeout
        else:
            msg = (
                f"Invalid type for timeout. "
                f"Expected float, int, None or httpx.Timeout, "
                f"but got {type(timeout)!r}."
            )
            raise TypeError(msg)

    def __eq__(self, other: typing.Any) -> bool:
        return (
            isinstance(other, self.__class__)
            and self.connect == other.connect
            and self.read == other.read
            and self.write == other.write
            and self.pool == other.pool
        )

    def __repr__(self) -> str:
        timeout_args = ""
        if all(
            v == self.connect for v in (self.read, self.write, self.pool)
        ):
            timeout_args = f"{self.connect!r}"
        else:
            timeout_args = (
                f"connect={self.connect!r}, "
                f"read={self.read!r}, "
                f"write={self.write!r}, "
                f"pool={self.pool!r}"
            )
        return f"{self.__class__.__name__}({timeout_args})"


class Limits:
    """
    Configuration for connection pooling limits.

    Parameters:
        max_connections: Maximum number of allowable connections.
        max_keepalive_connections: Number of allowable keep-alive connections.
        keepalive_expiry: Time limit on idle keep-alive connections in seconds.
    """

    def __init__(
        self,
        *,
        max_connections: typing.Optional[int] = None,
        max_keepalive_connections: typing.Optional[int] = None,
        keepalive_expiry: typing.Optional[float] = 5.0,
    ) -> None:
        self.max_connections = max_connections
        self.max_keepalive_connections = max_keepalive_connections
        self.keepalive_expiry = keepalive_expiry

    def __eq__(self, other: typing.Any) -> bool:
        return (
            isinstance(other, self.__class__)
            and self.max_connections == other.max_connections
            and self.max_keepalive_connections == other.max_keepalive_connections
            and self.keepalive_expiry == other.keepalive_expiry
        )


class BaseClient:
    """
    Base class for both sync and async HTTP clients.

    Provides shared init logic, URL merging, header handling,
    redirect following, and retry policy.
    """

    def __init__(
        self,
        *,
        auth: typing.Any = None,
        params: typing.Optional[typing.Dict[str, str]] = None,
        headers: typing.Optional[typing.Dict[str, str]] = None,
        cookies: typing.Optional[typing.Dict[str, str]] = None,
        timeout: typing.Union[None, float, Timeout] = DEFAULT_TIMEOUT,
        follow_redirects: bool = False,
        max_redirects: int = DEFAULT_MAX_REDIRECTS,
        base_url: typing.Union[str, None] = None,
        trust_env: bool = True,
    ) -> None:
        self._base_url = base_url or ""
        self._auth = auth
        self._params = params or {}
        self._headers: typing.Dict[str, str] = {
            "user-agent": USER_AGENT,
            "accept": "*/*",
            "accept-encoding": "gzip, deflate, br",
            "connection": "keep-alive",
        }
        if headers:
            self._headers.update(headers)

        self._cookies = cookies or {}
        self._timeout = Timeout(timeout)
        self.follow_redirects = follow_redirects
        self.max_redirects = max_redirects
        self._trust_env = trust_env
        self._state = ClientState.UNOPENED

    @property
    def is_closed(self) -> bool:
        """Check if the client has been closed."""
        return self._state == ClientState.CLOSED

    @property
    def trust_env(self) -> bool:
        return self._trust_env

    def _merge_url(self, url: str) -> str:
        """Merge a relative URL with the base URL."""
        if not self._base_url:
            return url
        if url.startswith(("http://", "https://")):
            return url
        base = self._base_url.rstrip("/")
        path = url.lstrip("/")
        return f"{base}/{path}"

    def _merge_headers(
        self,
        headers: typing.Optional[typing.Dict[str, str]],
    ) -> typing.Dict[str, str]:
        """Merge per-request headers over the client defaults."""
        merged = dict(self._headers)
        if headers:
            merged.update(headers)
        return merged

    def _merge_cookies(
        self,
        cookies: typing.Optional[typing.Dict[str, str]],
    ) -> typing.Dict[str, str]:
        merged = dict(self._cookies)
        if cookies:
            merged.update(cookies)
        return merged

    def _build_auth(self, auth: typing.Any) -> typing.Any:
        if auth is None:
            return self._auth
        return auth


class Client(BaseClient):
    """
    Synchronous HTTP client.

    Usage::

        with httpx.Client() as client:
            response = client.get("https://example.com")
    """

    def __init__(self, **kwargs: typing.Any) -> None:
        super().__init__(**kwargs)

    def __enter__(self: "Client") -> "Client":
        if self._state != ClientState.UNOPENED:
            msg = {
                ClientState.OPENED: "Cannot open a client instance more than once.",
                ClientState.CLOSED: "Cannot reopen a client instance, once it has been closed.",
            }[self._state]
            raise RuntimeError(msg)
        self._state = ClientState.OPENED
        return self

    def __exit__(
        self,
        exc_type: typing.Optional[typing.Type[BaseException]] = None,
        exc_value: typing.Optional[BaseException] = None,
        traceback: typing.Optional[TracebackType] = None,
    ) -> None:
        self.close()

    def close(self) -> None:
        """
        Close HTTP connections that were opened in keep-alive state.
        Threads that are currently making requests will complete normally.
        """
        if self._state != ClientState.CLOSED:
            self._state = ClientState.CLOSED

    def get(
        self,
        url: str,
        *,
        params: typing.Optional[typing.Dict[str, str]] = None,
        headers: typing.Optional[typing.Dict[str, str]] = None,
        cookies: typing.Optional[typing.Dict[str, str]] = None,
        auth: typing.Any = None,
        follow_redirects: bool = True,
        timeout: typing.Union[None, float, Timeout] = None,
    ) -> typing.Dict[str, typing.Any]:
        """Send a GET request."""
        return self.request(
            "GET",
            url,
            params=params,
            headers=headers,
            cookies=cookies,
            auth=auth,
            follow_redirects=follow_redirects,
            timeout=timeout,
        )

    def post(
        self,
        url: str,
        *,
        content: typing.Optional[bytes] = None,
        data: typing.Optional[typing.Dict[str, str]] = None,
        json: typing.Any = None,
        headers: typing.Optional[typing.Dict[str, str]] = None,
        timeout: typing.Union[None, float, Timeout] = None,
    ) -> typing.Dict[str, typing.Any]:
        """Send a POST request."""
        return self.request(
            "POST",
            url,
            content=content,
            data=data,
            json=json,
            headers=headers,
            timeout=timeout,
        )

    def request(
        self,
        method: str,
        url: str,
        *,
        content: typing.Optional[bytes] = None,
        data: typing.Optional[typing.Dict[str, str]] = None,
        json: typing.Any = None,
        params: typing.Optional[typing.Dict[str, str]] = None,
        headers: typing.Optional[typing.Dict[str, str]] = None,
        cookies: typing.Optional[typing.Dict[str, str]] = None,
        auth: typing.Any = None,
        follow_redirects: bool = True,
        timeout: typing.Union[None, float, Timeout] = None,
    ) -> typing.Dict[str, typing.Any]:
        """Send an HTTP request.

        :param method: HTTP method (GET, POST, ...).
        :param url: Target URL.
        :returns: Parsed response dict with status, headers, body.
        :raises RuntimeError: If client is closed.
        """
        if self._state == ClientState.CLOSED:
            raise RuntimeError("Cannot send a request, as the client has been closed.")

        full_url = self._merge_url(url)
        merged_headers = self._merge_headers(headers)
        effective_timeout = Timeout(timeout) if timeout is not None else self._timeout

        # Placeholder — real implementation would dispatch to transport layer.
        return {
            "url": full_url,
            "method": method.upper(),
            "headers": merged_headers,
            "timeout": effective_timeout,
        }
`;

// ---------------------------------------------------------------------------
// Go  (~400 lines) — net/http client patterns from real stdlib
// ---------------------------------------------------------------------------
export const GO_LARGE = `
// Package httpclient provides a configurable HTTP client with retry and
// timeout support. Adapted from the Go standard library net/http patterns.
package httpclient

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"math/rand"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

// RetryPolicy controls how the client retries failed requests.
type RetryPolicy struct {
	// MaxRetries is the maximum number of retry attempts after the initial try.
	MaxRetries int

	// BaseDelay is the base duration for exponential back-off.
	BaseDelay time.Duration

	// MaxDelay caps the computed back-off duration.
	MaxDelay time.Duration

	// Jitter adds random noise [0, Jitter) to each delay to reduce thundering herds.
	Jitter time.Duration

	// RetryOn is the list of HTTP status codes that trigger a retry.
	// Defaults to [429, 500, 502, 503, 504] when empty.
	RetryOn []int
}

// Timeout groups per-phase timeouts for a single request.
type Timeout struct {
	// Connect is the TCP connection establishment timeout.
	Connect time.Duration

	// Request is the end-to-end request/response timeout.
	Request time.Duration
}

// Option is a functional option applied to a Client at construction time.
type Option func(*Client)

// Response wraps an http.Response together with the parsed body bytes.
type Response struct {
	*http.Response
	Body    []byte
	Elapsed time.Duration
}

// ---------------------------------------------------------------------------
// Functional options
// ---------------------------------------------------------------------------

// WithRetry configures the retry policy.
func WithRetry(p RetryPolicy) Option {
	return func(c *Client) { c.retry = p }
}

// WithTimeout configures per-phase timeouts.
func WithTimeout(t Timeout) Option {
	return func(c *Client) { c.timeout = t }
}

// WithBaseURL sets a base URL that is prepended to relative request paths.
func WithBaseURL(rawURL string) Option {
	return func(c *Client) {
		u, err := url.Parse(rawURL)
		if err != nil {
			panic(fmt.Sprintf("httpclient: invalid base URL %q: %v", rawURL, err))
		}
		c.baseURL = u
	}
}

// WithHeader adds a default header sent with every request.
func WithHeader(key, value string) Option {
	return func(c *Client) {
		if c.headers == nil {
			c.headers = make(http.Header)
		}
		c.headers.Set(key, value)
	}
}

// WithLogger attaches a structured logger.
func WithLogger(l *slog.Logger) Option {
	return func(c *Client) { c.log = l }
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

// Client is a thin wrapper around http.Client that adds retry, timeout, and
// base-URL support without hidden magic or global state.
type Client struct {
	http    *http.Client
	baseURL *url.URL
	headers http.Header
	retry   RetryPolicy
	timeout Timeout
	log     *slog.Logger
}

// DefaultRetry is the retry policy applied when none is specified.
var DefaultRetry = RetryPolicy{
	MaxRetries: 3,
	BaseDelay:  200 * time.Millisecond,
	MaxDelay:   30 * time.Second,
	Jitter:     100 * time.Millisecond,
	RetryOn:    []int{429, 500, 502, 503, 504},
}

// New returns a new Client with sensible defaults and any supplied options.
func New(opts ...Option) *Client {
	c := &Client{
		timeout: Timeout{
			Connect: 5 * time.Second,
			Request: 30 * time.Second,
		},
		retry: DefaultRetry,
		log:   slog.Default(),
	}

	// Apply user options.
	for _, o := range opts {
		o(c)
	}

	c.http = &http.Client{
		Timeout: c.timeout.Request,
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			if len(via) >= 10 {
				return errors.New("httpclient: stopped after 10 redirects")
			}
			return nil
		},
	}

	return c
}

// ---------------------------------------------------------------------------
// Request dispatch
// ---------------------------------------------------------------------------

// Get sends an HTTP GET request and returns the raw response.
func (c *Client) Get(ctx context.Context, path string) (*Response, error) {
	return c.Do(ctx, http.MethodGet, path, nil)
}

// Post sends an HTTP POST request with a JSON body.
func (c *Client) Post(ctx context.Context, path string, body any) (*Response, error) {
	b, err := json.Marshal(body)
	if err != nil {
		return nil, fmt.Errorf("httpclient: marshal body: %w", err)
	}
	return c.Do(ctx, http.MethodPost, path, bytes.NewReader(b))
}

// Put sends an HTTP PUT request with a JSON body.
func (c *Client) Put(ctx context.Context, path string, body any) (*Response, error) {
	b, err := json.Marshal(body)
	if err != nil {
		return nil, fmt.Errorf("httpclient: marshal body: %w", err)
	}
	return c.Do(ctx, http.MethodPut, path, bytes.NewReader(b))
}

// Delete sends an HTTP DELETE request.
func (c *Client) Delete(ctx context.Context, path string) (*Response, error) {
	return c.Do(ctx, http.MethodDelete, path, nil)
}

// Do sends an HTTP request with the given method, path, and optional body,
// retrying according to the client's retry policy.
func (c *Client) Do(ctx context.Context, method, path string, body io.Reader) (*Response, error) {
	u, err := c.resolveURL(path)
	if err != nil {
		return nil, err
	}

	var (
		attempt int
		lastErr error
		resp    *Response
	)

	maxAttempts := c.retry.MaxRetries + 1
	for attempt = 0; attempt < maxAttempts; attempt++ {
		if attempt > 0 {
			delay := c.backoffDelay(attempt - 1)
			c.log.Debug("retrying request",
				"attempt", attempt,
				"delay", delay,
				"url", u.String(),
			)
			select {
			case <-ctx.Done():
				return nil, ctx.Err()
			case <-time.After(delay):
			}
		}

		resp, lastErr = c.doOnce(ctx, method, u.String(), body)
		if lastErr != nil {
			if !isTransient(lastErr) {
				return nil, lastErr
			}
			continue
		}

		if !c.shouldRetry(resp.StatusCode) {
			return resp, nil
		}
		lastErr = fmt.Errorf("httpclient: server returned %d", resp.StatusCode)
	}

	return nil, fmt.Errorf("httpclient: all %d attempts failed: %w", maxAttempts, lastErr)
}

func (c *Client) doOnce(ctx context.Context, method, rawURL string, body io.Reader) (*Response, error) {
	req, err := http.NewRequestWithContext(ctx, method, rawURL, body)
	if err != nil {
		return nil, fmt.Errorf("httpclient: build request: %w", err)
	}

	// Copy default headers.
	for k, vs := range c.headers {
		for _, v := range vs {
			req.Header.Set(k, v)
		}
	}

	// Ensure JSON content-type for bodies.
	if body != nil && req.Header.Get("Content-Type") == "" {
		req.Header.Set("Content-Type", "application/json")
	}

	start := time.Now()
	raw, err := c.http.Do(req)
	elapsed := time.Since(start)

	if err != nil {
		return nil, err
	}
	defer raw.Body.Close()

	b, err := io.ReadAll(raw.Body)
	if err != nil {
		return nil, fmt.Errorf("httpclient: read body: %w", err)
	}

	return &Response{Response: raw, Body: b, Elapsed: elapsed}, nil
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

func (c *Client) resolveURL(path string) (*url.URL, error) {
	if c.baseURL == nil {
		return url.Parse(path)
	}
	ref, err := url.Parse(path)
	if err != nil {
		return nil, fmt.Errorf("httpclient: parse path %q: %w", path, err)
	}
	return c.baseURL.ResolveReference(ref), nil
}

func (c *Client) backoffDelay(attempt int) time.Duration {
	// Exponential back-off: base * 2^attempt, capped at MaxDelay.
	delay := c.retry.BaseDelay * (1 << uint(attempt))
	if delay > c.retry.MaxDelay {
		delay = c.retry.MaxDelay
	}
	if c.retry.Jitter > 0 {
		delay += time.Duration(rand.Int63n(int64(c.retry.Jitter)))
	}
	return delay
}

func (c *Client) shouldRetry(statusCode int) bool {
	codes := c.retry.RetryOn
	if len(codes) == 0 {
		codes = []int{429, 500, 502, 503, 504}
	}
	for _, code := range codes {
		if code == statusCode {
			return true
		}
	}
	return false
}

// isTransient reports whether err is a transient network error worth retrying.
func isTransient(err error) bool {
	if err == nil {
		return false
	}
	s := strings.ToLower(err.Error())
	for _, phrase := range []string{"connection reset", "broken pipe", "timeout", "eof"} {
		if strings.Contains(s, phrase) {
			return true
		}
	}
	return false
}
`;

// ---------------------------------------------------------------------------
// Java  (~400 lines) — Spring-style StringUtils with JavaDoc
// ---------------------------------------------------------------------------
export const JAVA_LARGE = `
package org.example.util;

import java.nio.charset.Charset;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.StringTokenizer;
import java.util.regex.Pattern;

/**
 * Miscellaneous {@link String} utility methods, adapted from
 * {@code org.springframework.util.StringUtils}.
 *
 * <p>This class delivers some simple functionality that should really be
 * provided by the core Java {@link String} and {@link StringBuilder} classes.
 * It also provides easy-to-use methods to convert between delimited strings,
 * such as CSV strings, and collections and arrays.
 *
 * @author Rod Johnson
 * @author Juergen Hoeller
 * @author Keith Donald
 */
public abstract class StringUtils {

    private static final String FOLDER_SEPARATOR = "/";
    private static final char FOLDER_SEPARATOR_CHAR = '/';
    private static final String WINDOWS_FOLDER_SEPARATOR = "\\\\";
    private static final String TOP_PATH = "..";
    private static final String CURRENT_PATH = ".";
    private static final char EXTENSION_SEPARATOR = '.';
    private static final Pattern WHITESPACE = Pattern.compile("\\\\s+");

    // -----------------------------------------------------------------------
    // General convenience methods for working with Strings
    // -----------------------------------------------------------------------

    /**
     * Check whether the given object (possibly a {@code String}) is empty.
     * This is effectively a shortcut for
     * {@code !hasLength(String)}, respectively {@code !hasText(String)}.
     *
     * @param str the candidate object (possibly a {@code String})
     * @deprecated as of 5.3, in favor of {@link #hasLength(String)} and
     *   {@link #hasText(String)} (or {@link org.springframework.util.ObjectUtils#isEmpty(Object)})
     */
    @Deprecated
    public static boolean isEmpty(Object str) {
        return (str == null || "".equals(str));
    }

    /**
     * Check that the given {@code String} is neither {@code null} nor of length 0.
     * <p>Note: this method returns {@code true} for a {@code String} that
     * purely consists of whitespace.
     *
     * @param str the {@code String} to check (may be {@code null})
     * @return {@code true} if the {@code String} is not {@code null} and has length
     * @see #hasText(String)
     */
    public static boolean hasLength(String str) {
        return (str != null && !str.isEmpty());
    }

    /**
     * Check whether the given {@code String} contains actual <em>text</em>.
     * <p>More specifically, this method returns {@code true} if the
     * {@code String} is not {@code null}, its length is greater than 0,
     * and it contains at least one non-whitespace character.
     *
     * @param str the {@code String} to check (may be {@code null})
     * @return {@code true} if the {@code String} is not {@code null}, its
     *   length is greater than 0, and it does not contain whitespace only
     */
    public static boolean hasText(String str) {
        return (str != null && !str.isBlank());
    }

    /**
     * Check whether the given {@code String} contains any whitespace characters.
     *
     * @param str the {@code String} to check (may be {@code null})
     * @return {@code true} if the {@code String} is not empty and
     *   contains at least 1 whitespace character
     */
    public static boolean containsWhitespace(String str) {
        if (!hasLength(str)) {
            return false;
        }
        return str.chars().anyMatch(Character::isWhitespace);
    }

    /**
     * Trim leading and trailing whitespace from the given {@code String}.
     *
     * @param str the {@code String} to check
     * @return the trimmed {@code String}
     */
    public static String trimWhitespace(String str) {
        if (!hasLength(str)) {
            return str;
        }
        return str.strip();
    }

    /**
     * Trim <em>all</em> whitespace from the given {@code String}:
     * leading, trailing, and in between characters.
     *
     * @param str the {@code String} to check
     * @return the trimmed {@code String}
     */
    public static String trimAllWhitespace(String str) {
        if (!hasLength(str)) {
            return str;
        }
        return WHITESPACE.matcher(str).replaceAll("");
    }

    /**
     * Test if the given {@code String} starts with the specified prefix,
     * ignoring upper/lower case.
     *
     * @param str    the {@code String} to check
     * @param prefix the prefix to look for
     */
    public static boolean startsWithIgnoreCase(String str, String prefix) {
        return (str != null && prefix != null && str.length() >= prefix.length()
                && str.regionMatches(true, 0, prefix, 0, prefix.length()));
    }

    /**
     * Test if the given {@code String} ends with the specified suffix,
     * ignoring upper/lower case.
     *
     * @param str    the {@code String} to check
     * @param suffix the suffix to look for
     */
    public static boolean endsWithIgnoreCase(String str, String suffix) {
        return (str != null && suffix != null && str.length() >= suffix.length()
                && str.regionMatches(true, str.length() - suffix.length(), suffix, 0, suffix.length()));
    }

    // -----------------------------------------------------------------------
    // Path operations
    // -----------------------------------------------------------------------

    /**
     * Extract the filename from the given Java resource path, e.g.
     * {@code "mypath/myfile.txt" -> "myfile.txt"}.
     *
     * @param path the file path (may be {@code null})
     * @return the extracted filename, or {@code null} if none
     */
    public static String getFilename(String path) {
        if (path == null) {
            return null;
        }
        int separatorIndex = path.lastIndexOf(FOLDER_SEPARATOR_CHAR);
        return (separatorIndex != -1 ? path.substring(separatorIndex + 1) : path);
    }

    /**
     * Extract the filename extension from the given Java resource path,
     * e.g. "mypath/myfile.txt" -> "txt".
     *
     * @param path the file path (may be {@code null})
     * @return the extracted filename extension, or {@code null} if none
     */
    public static String getFilenameExtension(String path) {
        if (path == null) {
            return null;
        }
        int extIndex = path.lastIndexOf(EXTENSION_SEPARATOR);
        if (extIndex == -1) {
            return null;
        }
        int folderIndex = path.lastIndexOf(FOLDER_SEPARATOR_CHAR);
        if (folderIndex > extIndex) {
            return null;
        }
        return path.substring(extIndex + 1);
    }

    /**
     * Strip the filename extension from the given Java resource path,
     * e.g. "mypath/myfile.txt" -> "mypath/myfile".
     *
     * @param path the file path
     * @return the path with stripped filename extension
     */
    public static String stripFilenameExtension(String path) {
        int extIndex = path.lastIndexOf(EXTENSION_SEPARATOR);
        if (extIndex == -1) {
            return path;
        }
        int folderIndex = path.lastIndexOf(FOLDER_SEPARATOR_CHAR);
        if (folderIndex > extIndex) {
            return path;
        }
        return path.substring(0, extIndex);
    }

    // -----------------------------------------------------------------------
    // String/array/collection manipulation
    // -----------------------------------------------------------------------

    /**
     * Tokenize the given {@code String} into a {@code String} array via a
     * {@link StringTokenizer}.
     *
     * @param str               the {@code String} to tokenize (may be {@code null})
     * @param delimiters        the delimiter characters, assembled as a {@code String}
     * @param trimTokens        trim the tokens via {@link String#trim()}
     * @param ignoreEmptyTokens omit empty tokens from the result array
     * @return an array of the tokens
     */
    public static String[] tokenizeToStringArray(
            String str, String delimiters, boolean trimTokens, boolean ignoreEmptyTokens) {
        if (str == null) {
            return new String[0];
        }
        StringTokenizer st = new StringTokenizer(str, delimiters);
        List<String> tokens = new ArrayList<>();
        while (st.hasMoreTokens()) {
            String token = st.nextToken();
            if (trimTokens) {
                token = token.trim();
            }
            if (!ignoreEmptyTokens || !token.isEmpty()) {
                tokens.add(token);
            }
        }
        return tokens.toArray(new String[0]);
    }

    /**
     * Convert a comma-delimited list (e.g., a row from a CSV file) into an
     * array of strings.
     *
     * @param str the input {@code String} (potentially {@code null} or empty)
     * @return an array of strings, or the empty array in case of empty input
     */
    public static String[] commaDelimitedListToStringArray(String str) {
        return delimitedListToStringArray(str, ",");
    }

    /**
     * Take a {@code String} that is a delimited list and convert it into a
     * {@code String} array.
     *
     * <p>A single {@code delimiter} may consist of more than one character,
     * but it will still be considered as a single delimiter string, rather
     * than as a bunch of potential delimiter characters, in contrast to
     * {@link #tokenizeToStringArray}.
     *
     * @param str       the input {@code String} (potentially {@code null} or empty)
     * @param delimiter the delimiter between elements (this is a single delimiter,
     *                  rather than a bunch of individual delimiter characters)
     * @return an array of the tokens in the list
     */
    public static String[] delimitedListToStringArray(String str, String delimiter) {
        return delimitedListToStringArray(str, delimiter, null);
    }

    /**
     * Take a {@code String} that is a delimited list and convert it into a
     * {@code String} array.
     *
     * @param str           the input {@code String} (potentially {@code null} or empty)
     * @param delimiter     the delimiter between elements
     * @param charsToDelete a set of characters to delete; useful for deleting unwanted
     *                      line breaks: e.g. "\\r\\n\\f" will delete all new lines and
     *                      line feeds in a {@code String}
     * @return an array of the tokens in the list
     */
    public static String[] delimitedListToStringArray(
            String str, String delimiter, String charsToDelete) {
        if (str == null) {
            return new String[0];
        }
        if (delimiter == null) {
            return new String[]{str};
        }

        List<String> result = new ArrayList<>();
        if (delimiter.isEmpty()) {
            for (int i = 0; i < str.length(); i++) {
                result.add(deleteAny(String.valueOf(str.charAt(i)), charsToDelete));
            }
        } else {
            int pos = 0;
            int delPos;
            while ((delPos = str.indexOf(delimiter, pos)) != -1) {
                result.add(deleteAny(str.substring(pos, delPos), charsToDelete));
                pos = delPos + delimiter.length();
            }
            if (!str.isEmpty() && pos <= str.length()) {
                result.add(deleteAny(str.substring(pos), charsToDelete));
            }
        }
        return result.toArray(new String[0]);
    }

    /**
     * Convert a comma-delimited list (e.g., a row from a CSV file) into a set.
     * Note that this will suppress duplicates, and as of 4.2, the elements in
     * the returned set will preserve the original order in a {@link LinkedHashSet}.
     *
     * @param str the input {@code String} (potentially {@code null} or empty)
     * @return a set of {@code String} entries in the list
     */
    public static Set<String> commaDelimitedListToSet(String str) {
        String[] tokens = commaDelimitedListToStringArray(str);
        return new LinkedHashSet<>(Arrays.asList(tokens));
    }

    /**
     * Convert a {@link Collection} to a delimited {@code String} (e.g. CSV).
     *
     * @param coll  the {@code Collection} to convert (potentially {@code null} or empty)
     * @param delim the delimiter to use (typically a ",")
     * @return the delimited {@code String}
     */
    public static String collectionToDelimitedString(Collection<?> coll, String delim) {
        return collectionToDelimitedString(coll, delim, "", "");
    }

    /**
     * Convert a {@link Collection} to a delimited {@code String} (e.g. CSV).
     *
     * @param coll   the {@code Collection} to convert (potentially {@code null} or empty)
     * @param delim  the delimiter to use (typically a ",")
     * @param prefix the {@code String} to start each element with
     * @param suffix the {@code String} to end each element with
     * @return the delimited {@code String}
     */
    public static String collectionToDelimitedString(
            Collection<?> coll, String delim, String prefix, String suffix) {
        if (coll == null || coll.isEmpty()) {
            return "";
        }
        int totalLength = coll.size();
        StringBuilder sb = new StringBuilder();
        int i = 0;
        for (Object obj : coll) {
            sb.append(prefix).append(obj).append(suffix);
            if (i < totalLength - 1) {
                sb.append(delim);
            }
            i++;
        }
        return sb.toString();
    }

    /**
     * Delete any character in a given {@code String}.
     *
     * @param inString      the original {@code String}
     * @param charsToDelete a set of characters to delete
     *                      (e.g. "az\\n" will delete 'a's, 'z's and new lines)
     * @return the resulting {@code String}
     */
    public static String deleteAny(String inString, String charsToDelete) {
        if (!hasLength(inString) || !hasLength(charsToDelete)) {
            return inString;
        }
        StringBuilder sb = new StringBuilder(inString.length());
        for (int i = 0; i < inString.length(); i++) {
            char c = inString.charAt(i);
            if (charsToDelete.indexOf(c) == -1) {
                sb.append(c);
            }
        }
        return sb.toString();
    }

    /**
     * Quote the given {@code String} with single quotes.
     *
     * @param str the input {@code String} (e.g. "myString")
     * @return the quoted {@code String} (e.g. "'myString'"),
     *   or {@code null} if the input is {@code null}
     */
    public static String quote(String str) {
        return (str != null ? "'" + str + "'" : null);
    }
}
`;

// ---------------------------------------------------------------------------
// Rust  (~400 lines) — async task runtime, real tokio/async-std patterns
// ---------------------------------------------------------------------------
export const RUST_LARGE = `
//! Async task executor and scheduler.
//!
//! This module provides a lightweight cooperative-multitasking runtime built
//! on top of Rust's standard \`Future\` trait. The design follows the
//! \`tokio::runtime\` model: a fixed-size thread pool drives a work-stealing
//! deque, while per-thread queues reduce cross-thread contention.
//!
//! # Example
//!
//! \`\`\`rust
//! use crate::runtime::Runtime;
//!
//! let rt = Runtime::builder()
//!     .worker_threads(4)
//!     .build()
//!     .unwrap();
//!
//! rt.block_on(async {
//!     println!("Hello from async context");
//! });
//! \`\`\`

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Task representation
// ---------------------------------------------------------------------------

/// A boxed, pinned future that the scheduler can poll.
type BoxFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Internal state of a single async task.
struct TaskInner {
    /// The future driving this task.
    future: Mutex<Option<BoxFuture<'static>>>,
    /// Reference to the executor that spawned this task.
    executor: Arc<Executor>,
    /// True once the task has been scheduled for polling.
    scheduled: AtomicBool,
}

/// A reference-counted task handle.
pub struct Task(Arc<TaskInner>);

impl Task {
    /// Spawn a new task on the given executor.
    fn spawn(future: impl Future<Output = ()> + Send + 'static, executor: Arc<Executor>) -> Self {
        let inner = Arc::new(TaskInner {
            future: Mutex::new(Some(Box::pin(future))),
            executor,
            scheduled: AtomicBool::new(false),
        });
        Task(inner)
    }

    /// Schedule this task for polling if it is not already queued.
    fn schedule(self: &Arc<TaskInner>) {
        // Only enqueue once; the flag is cleared by the worker that picks it up.
        if self
            .scheduled
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            self.executor.enqueue(Task(self.clone()));
        }
    }
}

impl Wake for TaskInner {
    fn wake(self: Arc<Self>) {
        self.schedule();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.schedule();
    }
}

// ---------------------------------------------------------------------------
// Thread pool
// ---------------------------------------------------------------------------

/// Configuration for the runtime thread pool.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Number of worker threads.  Defaults to the number of logical CPUs.
    pub worker_threads: usize,
    /// Idle keep-alive before a thread is terminated.
    pub keep_alive: Duration,
    /// Maximum tasks that can sit in the global queue before back-pressure is applied.
    pub queue_capacity: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            worker_threads: num_cpus(),
            keep_alive: Duration::from_secs(60),
            queue_capacity: 4_096,
        }
    }
}

/// Shared state owned by all worker threads.
struct Executor {
    /// Global task queue protected by a mutex for push/pop.
    queue: Mutex<Vec<Task>>,
    /// Signals waiting workers that a new task is available.
    condvar: Condvar,
    /// Total number of live (not yet completed) tasks.
    active: AtomicUsize,
    /// Set to true to request an orderly shutdown.
    shutdown: AtomicBool,
}

impl Executor {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(Vec::new()),
            condvar: Condvar::new(),
            active: AtomicUsize::new(0),
            shutdown: AtomicBool::new(false),
        })
    }

    /// Push a task onto the global queue and wake a sleeping worker.
    fn enqueue(&self, task: Task) {
        self.queue.lock().unwrap().push(task);
        self.condvar.notify_one();
    }

    /// Block until a task is available or shutdown is requested.
    ///
    /// Returns \`None\` on shutdown.
    fn dequeue(&self, timeout: Duration) -> Option<Task> {
        let mut guard = self.queue.lock().unwrap();
        let deadline = Instant::now() + timeout;

        loop {
            if let Some(task) = guard.pop() {
                return Some(task);
            }
            if self.shutdown.load(Ordering::Acquire) {
                return None;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (g, _timeout) = self
                .condvar
                .wait_timeout(guard, remaining)
                .unwrap();
            guard = g;
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

/// Async task runtime backed by a thread pool.
pub struct Runtime {
    executor: Arc<Executor>,
    workers: Vec<JoinHandle<()>>,
}

/// Builder for \`Runtime\`.
pub struct Builder {
    config: RuntimeConfig,
}

impl Builder {
    /// Create a builder with default settings.
    pub fn new() -> Self {
        Builder {
            config: RuntimeConfig::default(),
        }
    }

    /// Override the worker thread count.
    pub fn worker_threads(mut self, n: usize) -> Self {
        assert!(n > 0, "worker_threads must be > 0");
        self.config.worker_threads = n;
        self
    }

    /// Override the idle keep-alive duration.
    pub fn keep_alive(mut self, d: Duration) -> Self {
        self.config.keep_alive = d;
        self
    }

    /// Build the \`Runtime\`, spawning worker threads immediately.
    pub fn build(self) -> Result<Runtime, std::io::Error> {
        let executor = Executor::new();
        let mut workers = Vec::with_capacity(self.config.worker_threads);

        for id in 0..self.config.worker_threads {
            let exec = Arc::clone(&executor);
            let keep_alive = self.config.keep_alive;
            let handle = thread::Builder::new()
                .name(format!("runtime-worker-{id}"))
                .spawn(move || worker_loop(exec, keep_alive))?;
            workers.push(handle);
        }

        Ok(Runtime { executor, workers })
    }
}

impl Default for Builder {
    fn default() -> Self {
        Builder::new()
    }
}

impl Runtime {
    /// Create a \`Builder\` with default settings.
    pub fn builder() -> Builder {
        Builder::new()
    }

    /// Spawn an async task.
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        let task = Task::spawn(future, Arc::clone(&self.executor));
        self.executor.active.fetch_add(1, Ordering::Relaxed);
        self.executor.enqueue(task);
    }

    /// Block the current thread until \`future\` resolves.
    ///
    /// This drives the future to completion using the runtime's thread pool,
    /// returning the output value.
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        // Park the calling thread and pin the future on the stack.
        struct ThreadWaker(thread::Thread);
        impl Wake for ThreadWaker {
            fn wake(self: Arc<Self>) { self.0.unpark(); }
            fn wake_by_ref(self: &Arc<Self>) { self.0.unpark(); }
        }

        let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(output) => return output,
                Poll::Pending => thread::park(),
            }
        }
    }

    /// Shut down the runtime, waiting for all in-flight tasks to complete.
    pub fn shutdown(self) {
        self.executor.shutdown.store(true, Ordering::Release);
        self.executor.condvar.notify_all();
        for worker in self.workers {
            let _ = worker.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Worker loop
// ---------------------------------------------------------------------------

fn worker_loop(executor: Arc<Executor>, keep_alive: Duration) {
    loop {
        let Some(task) = executor.dequeue(keep_alive) else {
            // Timed out or shutdown requested.
            if executor.shutdown.load(Ordering::Acquire) {
                return;
            }
            continue;
        };

        // Clear the scheduled flag so the waker can re-queue it later.
        task.0.scheduled.store(false, Ordering::Release);

        // Poll the future once.
        let waker = Waker::from(Arc::clone(&task.0));
        let mut cx = Context::from_waker(&waker);

        let mut future_guard = task.0.future.lock().unwrap();
        if let Some(future) = future_guard.as_mut() {
            if future.as_mut().poll(&mut cx).is_ready() {
                // Task is done; drop the future and decrement the counter.
                *future_guard = None;
                executor.active.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Platform helper
// ---------------------------------------------------------------------------

fn num_cpus() -> usize {
    // std doesn't expose this directly; fall back to 1 for no_std targets.
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count()
            .max(1)
    }
    #[cfg(not(target_os = "linux"))]
    {
        1
    }
}
`;

// ---------------------------------------------------------------------------
// CSS  (~400 lines) — design-system tokens + component styles
// ---------------------------------------------------------------------------
export const CSS_LARGE = `
/*
 * Design System — core tokens and component styles.
 * Generated from design tokens v2.4.1.
 * Do not edit manually; run 'yarn tokens:build' to regenerate.
 */

/* ============================================================
 * 1. Custom properties (tokens)
 * ============================================================ */

:root {
  /* Color palette */
  --color-primary-50:  #eff6ff;
  --color-primary-100: #dbeafe;
  --color-primary-200: #bfdbfe;
  --color-primary-300: #93c5fd;
  --color-primary-400: #60a5fa;
  --color-primary-500: #3b82f6;
  --color-primary-600: #2563eb;
  --color-primary-700: #1d4ed8;
  --color-primary-800: #1e40af;
  --color-primary-900: #1e3a8a;

  --color-neutral-50:  #f9fafb;
  --color-neutral-100: #f3f4f6;
  --color-neutral-200: #e5e7eb;
  --color-neutral-300: #d1d5db;
  --color-neutral-400: #9ca3af;
  --color-neutral-500: #6b7280;
  --color-neutral-600: #4b5563;
  --color-neutral-700: #374151;
  --color-neutral-800: #1f2937;
  --color-neutral-900: #111827;

  --color-danger-400:  #f87171;
  --color-danger-500:  #ef4444;
  --color-danger-600:  #dc2626;

  --color-success-400: #4ade80;
  --color-success-500: #22c55e;
  --color-success-600: #16a34a;

  --color-warning-400: #fbbf24;
  --color-warning-500: #f59e0b;

  /* Typography scale */
  --font-sans: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont,
    "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  --font-mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
    "Liberation Mono", "Courier New", monospace;

  --text-xs:   0.75rem;   /* 12px */
  --text-sm:   0.875rem;  /* 14px */
  --text-base: 1rem;      /* 16px */
  --text-lg:   1.125rem;  /* 18px */
  --text-xl:   1.25rem;   /* 20px */
  --text-2xl:  1.5rem;    /* 24px */
  --text-3xl:  1.875rem;  /* 30px */
  --text-4xl:  2.25rem;
  /* Spacing scale (4px grid) */
  --space-1:  0.25rem;
  --space-2:  0.5rem;
  --space-3:  0.75rem;
  --space-4:  1rem;
  --space-5:  1.25rem;
  --space-6:  1.5rem;
  --space-8:  2rem;
  --space-10: 2.5rem;
  --space-12: 3rem;
  --space-16: 4rem;

  /* Border radius */
  --radius-sm: 0.125rem;
  --radius-md: 0.375rem;
  --radius-lg: 0.5rem;
  --radius-xl: 0.75rem;
  --radius-full: 9999px;

  /* Shadows */
  --shadow-sm: 0 1px 2px 0 rgb(0 0 0 / 0.05);
  --shadow-md: 0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1);
  --shadow-lg: 0 10px 15px -3px rgb(0 0 0 / 0.1), 0 4px 6px -4px rgb(0 0 0 / 0.1);
  --shadow-xl: 0 20px 25px -5px rgb(0 0 0 / 0.1), 0 8px 10px -6px rgb(0 0 0 / 0.1);

  /* Transitions */
  --transition-fast:   150ms ease;
  --transition-base:   200ms ease;
  --transition-slow:   300ms ease;
  --transition-slower: 500ms ease;
}

/* ============================================================
 * 2. Reset
 * ============================================================ */

*, *::before, *::after {
  box-sizing: border-box;
}

html {
  line-height: 1.5;
  -webkit-text-size-adjust: 100%;
  tab-size: 4;
}

body {
  margin: 0;
  font-family: var(--font-sans);
  font-size: var(--text-base);
  color: var(--color-neutral-900);
  background-color: var(--color-neutral-50);
}

/* ============================================================
 * 3. Typography utilities
 * ============================================================ */

.text-xs   { font-size: var(--text-xs);   line-height: 1rem; }
.text-sm   { font-size: var(--text-sm);   line-height: 1.25rem; }
.text-base { font-size: var(--text-base); line-height: 1.5rem; }
.text-lg   { font-size: var(--text-lg);   line-height: 1.75rem; }
.text-xl   { font-size: var(--text-xl);   line-height: 1.75rem; }
.text-2xl  { font-size: var(--text-2xl);  line-height: 2rem; }
.text-3xl  { font-size: var(--text-3xl);  line-height: 2.25rem; }
.text-4xl  { font-size: var(--text-4xl);  line-height: 2.5rem; }

.font-normal   { font-weight: 400; }
.font-medium   { font-weight: 500; }
.font-semibold { font-weight: 600; }
.font-bold     { font-weight: 700; }

/* ============================================================
 * 4. Button component
 * ============================================================ */

/* Base button — shared across all variants */
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-4);
  border: 1px solid transparent;
  border-radius: var(--radius-md);
  font-family: inherit;
  font-size: var(--text-sm);
  font-weight: 500;
  line-height: 1.5;
  cursor: pointer;
  user-select: none;
  transition: background-color var(--transition-fast),
              border-color var(--transition-fast),
              color var(--transition-fast),
              box-shadow var(--transition-fast);
}

.btn:focus-visible {
  outline: 2px solid var(--color-primary-500);
  outline-offset: 2px;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  pointer-events: none;
}

/* Primary */
.btn-primary {
  color: #fff;
  background-color: var(--color-primary-600);
  border-color: var(--color-primary-600);
}

.btn-primary:hover { background-color: var(--color-primary-700); border-color: var(--color-primary-700); }
.btn-primary:active { background-color: var(--color-primary-800); }

/* Secondary */
.btn-secondary {
  color: var(--color-neutral-700);
  background-color: #fff;
  border-color: var(--color-neutral-300);
}

.btn-secondary:hover { background-color: var(--color-neutral-50); }
.btn-secondary:active { background-color: var(--color-neutral-100); }

/* Danger */
.btn-danger {
  color: #fff;
  background-color: var(--color-danger-600);
  border-color: var(--color-danger-600);
}

.btn-danger:hover { background-color: #b91c1c; }

/* Sizes */
.btn-sm { padding: var(--space-1) var(--space-3); font-size: var(--text-xs); }
.btn-lg { padding: var(--space-3) var(--space-6); font-size: var(--text-base); }

/* ============================================================
 * 5. Card component
 * ============================================================ */

.card {
  background-color: #fff;
  border: 1px solid var(--color-neutral-200);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
  overflow: hidden;
}

.card-header {
  padding: var(--space-4) var(--space-6);
  border-bottom: 1px solid var(--color-neutral-200);
}

.card-body {
  padding: var(--space-6);
}

.card-footer {
  padding: var(--space-4) var(--space-6);
  background-color: var(--color-neutral-50);
  border-top: 1px solid var(--color-neutral-200);
}

/* ============================================================
 * 6. Form controls
 * ============================================================ */

.form-label {
  display: block;
  margin-bottom: var(--space-1);
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--color-neutral-700);
}

.form-input,
.form-select,
.form-textarea {
  display: block;
  width: 100%;
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-neutral-300);
  border-radius: var(--radius-md);
  font-family: inherit;
  font-size: var(--text-sm);
  line-height: 1.5;
  color: var(--color-neutral-900);
  background-color: #fff;
  transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
}

.form-input:focus,
.form-select:focus,
.form-textarea:focus {
  outline: none;
  border-color: var(--color-primary-500);
  box-shadow: 0 0 0 3px rgb(59 130 246 / 0.15);
}

.form-input.is-invalid,
.form-select.is-invalid,
.form-textarea.is-invalid {
  border-color: var(--color-danger-500);
}

.form-error {
  margin-top: var(--space-1);
  font-size: var(--text-xs);
  color: var(--color-danger-600);
}

/* ============================================================
 * 7. Badge
 * ============================================================ */

.badge {
  display: inline-flex;
  align-items: center;
  padding: 0.125rem 0.5rem;
  border-radius: var(--radius-full);
  font-size: var(--text-xs);
  font-weight: 500;
  line-height: 1.25rem;
}

.badge-primary { color: var(--color-primary-700); background-color: var(--color-primary-100); }
.badge-success { color: var(--color-success-600); background-color: #dcfce7; }
.badge-danger  { color: var(--color-danger-600);  background-color: #fee2e2; }
.badge-warning { color: #92400e;                  background-color: #fef3c7; }
.badge-neutral { color: var(--color-neutral-700); background-color: var(--color-neutral-100); }

/* ============================================================
 * 8. Layout utilities
 * ============================================================ */

.container {
  width: 100%;
  margin-inline: auto;
  padding-inline: var(--space-4);
}

@media (min-width: 640px)  { .container { max-width: 640px; } }
@media (min-width: 768px)  { .container { max-width: 768px; } }
@media (min-width: 1024px) { .container { max-width: 1024px; } }
@media (min-width: 1280px) { .container { max-width: 1280px; } }

.flex         { display: flex; }
.flex-col     { flex-direction: column; }
.items-center { align-items: center; }
.justify-between { justify-content: space-between; }
.gap-2 { gap: var(--space-2); }
.gap-4 { gap: var(--space-4); }
.gap-6 { gap: var(--space-6); }

/* Grid */
.grid { display: grid; }
.grid-cols-2 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.grid-cols-3 { grid-template-columns: repeat(3, minmax(0, 1fr)); }
.grid-cols-4 { grid-template-columns: repeat(4, minmax(0, 1fr)); }

/* Spacing helpers */
.m-0  { margin: 0; }
.p-0  { padding: 0; }
.p-4  { padding: var(--space-4); }
.p-6  { padding: var(--space-6); }
.mt-4 { margin-top: var(--space-4); }
.mb-4 { margin-bottom: var(--space-4); }

/* ============================================================
 * 9. Dark mode overrides
 * ============================================================ */

@media (prefers-color-scheme: dark) {
  :root {
    --color-neutral-50:  #111827;
    --color-neutral-100: #1f2937;
    --color-neutral-900: #f9fafb;
  }

  body {
    color: var(--color-neutral-900);
    background-color: var(--color-neutral-50);
  }

  .card {
    background-color: var(--color-neutral-100);
    border-color: #374151;
  }

  .form-input,
  .form-select,
  .form-textarea {
    background-color: #1f2937;
    border-color: #374151;
    color: #f9fafb;
  }
}
`;

// ---------------------------------------------------------------------------
// SQL  (~400 lines) — schema + stored procs, PostgreSQL patterns
// ---------------------------------------------------------------------------
export const SQL_LARGE = `
-- =============================================================================
-- E-commerce platform schema — PostgreSQL 15
-- Generated by schema-tool v3.1.2
-- =============================================================================

-- Enable extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- =============================================================================
-- 1. Core tables
-- =============================================================================

-- Users and authentication
CREATE TABLE users (
    id            UUID          PRIMARY KEY DEFAULT uuid_generate_v4(),
    email         VARCHAR(255)  UNIQUE NOT NULL,
    password_hash VARCHAR(255)  NOT NULL,
    display_name  VARCHAR(100),
    avatar_url    TEXT,
    role          VARCHAR(50)   NOT NULL DEFAULT 'customer',
    -- Soft-delete support
    deleted_at    TIMESTAMPTZ,
    created_at    TIMESTAMPTZ   NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ   NOT NULL DEFAULT now()
);

CREATE INDEX idx_users_email ON users (email);
CREATE INDEX idx_users_role  ON users (role) WHERE deleted_at IS NULL;

-- Product catalogue
CREATE TABLE categories (
    id          SERIAL        PRIMARY KEY,
    parent_id   INT           REFERENCES categories (id) ON DELETE SET NULL,
    slug        VARCHAR(100)  UNIQUE NOT NULL,
    name        VARCHAR(200)  NOT NULL,
    description TEXT,
    sort_order  INT           NOT NULL DEFAULT 0
);

CREATE TABLE products (
    id           UUID          PRIMARY KEY DEFAULT uuid_generate_v4(),
    category_id  INT           REFERENCES categories (id) ON DELETE SET NULL,
    sku          VARCHAR(100)  UNIQUE NOT NULL,
    name         VARCHAR(500)  NOT NULL,
    description  TEXT,
    price        NUMERIC(12,2) NOT NULL CHECK (price >= 0),
    compare_at   NUMERIC(12,2),           -- original / crossed-out price
    stock        INT           NOT NULL DEFAULT 0 CHECK (stock >= 0),
    weight_g     INT,                     -- weight in grams for shipping calc
    is_active    BOOLEAN       NOT NULL DEFAULT true,
    metadata     JSONB,
    created_at   TIMESTAMPTZ   NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ   NOT NULL DEFAULT now()
);

CREATE INDEX idx_products_category    ON products (category_id) WHERE is_active;
CREATE INDEX idx_products_sku         ON products (sku);
CREATE INDEX idx_products_name_trgm   ON products USING GIN (name gin_trgm_ops);
CREATE INDEX idx_products_metadata    ON products USING GIN (metadata);

-- Product images (ordered)
CREATE TABLE product_images (
    id          SERIAL  PRIMARY KEY,
    product_id  UUID    NOT NULL REFERENCES products (id) ON DELETE CASCADE,
    url         TEXT    NOT NULL,
    alt_text    VARCHAR(255),
    sort_order  INT     NOT NULL DEFAULT 0
);

-- Orders and fulfilment
CREATE TABLE orders (
    id              UUID          PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         UUID          NOT NULL REFERENCES users (id),
    status          VARCHAR(50)   NOT NULL DEFAULT 'pending',
    subtotal        NUMERIC(12,2) NOT NULL,
    discount        NUMERIC(12,2) NOT NULL DEFAULT 0,
    tax             NUMERIC(12,2) NOT NULL DEFAULT 0,
    shipping        NUMERIC(12,2) NOT NULL DEFAULT 0,
    total           NUMERIC(12,2) NOT NULL,
    currency        CHAR(3)       NOT NULL DEFAULT 'USD',
    -- Shipping address (denormalised for immutability)
    ship_name       VARCHAR(200),
    ship_address1   VARCHAR(300),
    ship_address2   VARCHAR(300),
    ship_city       VARCHAR(100),
    ship_state      VARCHAR(100),
    ship_country    CHAR(2),
    ship_postal     VARCHAR(20),
    notes           TEXT,
    placed_at       TIMESTAMPTZ,
    shipped_at      TIMESTAMPTZ,
    delivered_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ   NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ   NOT NULL DEFAULT now()
);

CREATE INDEX idx_orders_user   ON orders (user_id, created_at DESC);
CREATE INDEX idx_orders_status ON orders (status, placed_at DESC);

CREATE TABLE order_lines (
    id          SERIAL        PRIMARY KEY,
    order_id    UUID          NOT NULL REFERENCES orders (id) ON DELETE CASCADE,
    product_id  UUID          NOT NULL REFERENCES products (id),
    sku         VARCHAR(100)  NOT NULL,   -- snapshot at purchase time
    name        VARCHAR(500)  NOT NULL,   -- snapshot
    unit_price  NUMERIC(12,2) NOT NULL,
    quantity    INT           NOT NULL CHECK (quantity > 0),
    line_total  NUMERIC(12,2) GENERATED ALWAYS AS (unit_price * quantity) STORED
);

-- =============================================================================
-- 2. Triggers: updated_at auto-maintenance
-- =============================================================================

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS
$$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

-- Attach trigger to every table that has updated_at
DO $$
DECLARE
    tbl TEXT;
BEGIN
    FOR tbl IN SELECT unnest(ARRAY['users','products','orders']) LOOP
        EXECUTE format(
            'CREATE TRIGGER trg_%s_updated_at
             BEFORE UPDATE ON %s
             FOR EACH ROW EXECUTE FUNCTION set_updated_at();',
            tbl, tbl
        );
    END LOOP;
END;
$$;

-- =============================================================================
-- 3. Stored procedures
-- =============================================================================

-- Place order: creates an order record together with all order lines and
-- atomically decrements stock. Rolls back on insufficient stock.
CREATE OR REPLACE PROCEDURE place_order(
    p_user_id   UUID,
    p_lines     JSONB,          -- [{sku, quantity}]
    p_currency  CHAR(3)  DEFAULT 'USD',
    p_notes     TEXT     DEFAULT NULL,
    INOUT p_order_id UUID DEFAULT NULL
)
LANGUAGE plpgsql AS
$$
DECLARE
    v_line       JSONB;
    v_product    products%ROWTYPE;
    v_subtotal   NUMERIC(12,2) := 0;
    v_order_id   UUID;
BEGIN
    -- Validate user exists and is not deleted
    IF NOT EXISTS (SELECT 1 FROM users WHERE id = p_user_id AND deleted_at IS NULL) THEN
        RAISE EXCEPTION 'User % not found or deactivated', p_user_id;
    END IF;

    -- Create a draft order
    INSERT INTO orders (user_id, status, subtotal, total, currency, notes)
    VALUES (p_user_id, 'pending', 0, 0, p_currency, p_notes)
    RETURNING id INTO v_order_id;

    -- Process each line item
    FOR v_line IN SELECT * FROM jsonb_array_elements(p_lines)
    LOOP
        SELECT * INTO STRICT v_product
          FROM products
         WHERE sku = (v_line->>'sku') AND is_active
           FOR UPDATE;  -- lock row to prevent concurrent oversell

        IF v_product.stock < (v_line->>'quantity')::INT THEN
            RAISE EXCEPTION 'Insufficient stock for SKU %: requested %, available %',
                v_line->>'sku',
                (v_line->>'quantity')::INT,
                v_product.stock;
        END IF;

        -- Deduct stock
        UPDATE products
           SET stock = stock - (v_line->>'quantity')::INT
         WHERE id = v_product.id;

        -- Record line
        INSERT INTO order_lines (order_id, product_id, sku, name, unit_price, quantity)
        VALUES (
            v_order_id,
            v_product.id,
            v_product.sku,
            v_product.name,
            v_product.price,
            (v_line->>'quantity')::INT
        );

        v_subtotal := v_subtotal + v_product.price * (v_line->>'quantity')::INT;
    END LOOP;

    -- Update order totals
    UPDATE orders
       SET subtotal   = v_subtotal,
           total      = v_subtotal,
           placed_at  = now(),
           status     = 'confirmed'
     WHERE id = v_order_id;

    p_order_id := v_order_id;
END;
$$;

-- Search products using full-text + trigram similarity
CREATE OR REPLACE FUNCTION search_products(
    p_query      TEXT,
    p_category   INT  DEFAULT NULL,
    p_min_price  NUMERIC DEFAULT NULL,
    p_max_price  NUMERIC DEFAULT NULL,
    p_limit      INT  DEFAULT 20,
    p_offset     INT  DEFAULT 0
)
RETURNS TABLE (
    id           UUID,
    sku          VARCHAR,
    name         VARCHAR,
    price        NUMERIC,
    stock        INT,
    similarity   FLOAT
)
LANGUAGE sql STABLE AS
$$
    SELECT
        p.id,
        p.sku,
        p.name,
        p.price,
        p.stock,
        similarity(p.name, p_query) AS similarity
    FROM products p
    WHERE p.is_active
      AND (p_category IS NULL OR p.category_id = p_category)
      AND (p_min_price IS NULL OR p.price >= p_min_price)
      AND (p_max_price IS NULL OR p.price <= p_max_price)
      AND (
          p_query IS NULL
          OR p.name ILIKE '%' || p_query || '%'
          OR similarity(p.name, p_query) > 0.2
      )
    ORDER BY
        CASE WHEN p_query IS NOT NULL THEN similarity(p.name, p_query) END DESC NULLS LAST,
        p.created_at DESC
    LIMIT  p_limit
    OFFSET p_offset;
$$;
`;

export const SHELL_LARGE = `
#!/usr/bin/env bash
# =============================================================================
# deploy.sh — Application deployment script
# Usage: ./deploy.sh [--env staging|production] [--branch <name>] [--dry-run]
# =============================================================================

set -euo pipefail
IFS=$'\\n\\t'

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

readonly SCRIPT_DIR="$(cd "$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
readonly TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
readonly LOG_FILE="/var/log/deploy/deploy_\${TIMESTAMP}.log"
readonly LOCK_FILE="/var/run/deploy.lock"

# Default values — overridable via flags
ENV="staging"
BRANCH="main"
DRY_RUN=false
ROLLBACK=false
SKIP_TESTS=false

# App-specific
APP_NAME="myapp"
DEPLOY_DIR="/opt/\${APP_NAME}"
RELEASES_DIR="\${DEPLOY_DIR}/releases"
SHARED_DIR="\${DEPLOY_DIR}/shared"
CURRENT_LINK="\${DEPLOY_DIR}/current"
KEEP_RELEASES=5

# Remote
STAGING_HOST="staging.example.com"
PRODUCTION_HOST="production.example.com"
DEPLOY_USER="deploy"
SSH_KEY="\${HOME}/.ssh/deploy_rsa"

# ---------------------------------------------------------------------------
# Colour helpers
# ---------------------------------------------------------------------------

RED='\\033[0;31m'
GREEN='\\033[0;32m'
YELLOW='\\033[1;33m'
BLUE='\\033[0;34m'
BOLD='\\033[1m'
RESET='\\033[0m'

log_info()    { echo -e "\${BLUE}[INFO]\${RESET}  $*" | tee -a "\${LOG_FILE}"; }
log_success() { echo -e "\${GREEN}[OK]\${RESET}    $*" | tee -a "\${LOG_FILE}"; }
log_warn()    { echo -e "\${YELLOW}[WARN]\${RESET}  $*" | tee -a "\${LOG_FILE}"; }
log_error()   { echo -e "\${RED}[ERROR]\${RESET} $*" | tee -a "\${LOG_FILE}" >&2; }
log_step()    { echo -e "\\n\${BOLD}===> $*\${RESET}" | tee -a "\${LOG_FILE}"; }

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --env <environment>   Target environment: staging or production (default: staging)
  --branch <name>       Git branch to deploy (default: main)
  --dry-run             Print actions without executing them
  --rollback            Roll back to the previous release
  --skip-tests          Skip pre-deployment test suite
  -h, --help            Show this help and exit
EOF
    exit 0
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --env)       ENV="$2";        shift 2 ;;
            --branch)    BRANCH="$2";     shift 2 ;;
            --dry-run)   DRY_RUN=true;    shift   ;;
            --rollback)  ROLLBACK=true;   shift   ;;
            --skip-tests) SKIP_TESTS=true; shift  ;;
            -h|--help)   usage                    ;;
            *)           log_error "Unknown argument: $1"; usage ;;
        esac
    done

    case "\${ENV}" in
        staging)    REMOTE_HOST="\${STAGING_HOST}"    ;;
        production) REMOTE_HOST="\${PRODUCTION_HOST}" ;;
        *)          log_error "Unknown environment: \${ENV}"; exit 1 ;;
    esac
}

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------

check_dependencies() {
    local deps=(git rsync ssh curl jq)
    local missing=()

    for dep in "\${deps[@]}"; do
        if ! command -v "\${dep}" &>/dev/null; then
            missing+=("\${dep}")
        fi
    done

    if [[ \${#missing[@]} -gt 0 ]]; then
        log_error "Missing dependencies: \${missing[*]}"
        exit 1
    fi
    log_success "All dependencies present"
}

acquire_lock() {
    if [[ -f "\${LOCK_FILE}" ]]; then
        local pid
        pid=$(cat "\${LOCK_FILE}")
        if kill -0 "\${pid}" 2>/dev/null; then
            log_error "Another deploy is in progress (PID \${pid})"
            exit 1
        fi
        log_warn "Stale lock file found — removing"
        rm -f "\${LOCK_FILE}"
    fi
    echo $$ > "\${LOCK_FILE}"
    trap 'rm -f "\${LOCK_FILE}"' EXIT
    log_success "Acquired deploy lock"
}

check_remote_connectivity() {
    if ! ssh -i "\${SSH_KEY}" -o BatchMode=yes -o ConnectTimeout=5 \\
             "\${DEPLOY_USER}@\${REMOTE_HOST}" exit 2>/dev/null; then
        log_error "Cannot connect to \${REMOTE_HOST}"
        exit 1
    fi
    log_success "Remote host reachable: \${REMOTE_HOST}"
}

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

run_tests() {
    if [[ "\${SKIP_TESTS}" == true ]]; then
        log_warn "Skipping tests (--skip-tests)"
        return
    fi
    log_step "Running test suite"
    \${DRY_RUN} && { log_info "[dry-run] yarn test"; return; }
    yarn install --frozen-lockfile
    yarn test --ci --coverage
    log_success "Tests passed"
}

build_artefact() {
    log_step "Building application (\${ENV})"
    \${DRY_RUN} && { log_info "[dry-run] yarn build:\${ENV}"; return; }

    NODE_ENV="\${ENV}" yarn build
    ARTEFACT="\${SCRIPT_DIR}/dist_\${TIMESTAMP}.tar.gz"
    tar -czf "\${ARTEFACT}" -C "\${SCRIPT_DIR}/dist" .
    log_success "Build artefact: \${ARTEFACT}"
}

# ---------------------------------------------------------------------------
# Remote operations (executed via SSH)
# ---------------------------------------------------------------------------

remote_exec() {
    local cmd="$1"
    if "\${DRY_RUN}"; then
        log_info "[dry-run] ssh \${REMOTE_HOST}: \${cmd}"
        return
    fi
    ssh -i "\${SSH_KEY}" "\${DEPLOY_USER}@\${REMOTE_HOST}" "\${cmd}"
}

prepare_release_dir() {
    local release_dir="\${RELEASES_DIR}/\${TIMESTAMP}"
    log_step "Creating release directory \${release_dir}"
    remote_exec "mkdir -p \${release_dir}"

    log_info "Uploading artefact"
    if ! "\${DRY_RUN}"; then
        rsync -az --delete \\
              -e "ssh -i \${SSH_KEY}" \\
              "\${ARTEFACT}" \\
              "\${DEPLOY_USER}@\${REMOTE_HOST}:\${RELEASES_DIR}/"
        remote_exec "tar -xzf \${RELEASES_DIR}/dist_\${TIMESTAMP}.tar.gz -C \${release_dir}"
        remote_exec "rm \${RELEASES_DIR}/dist_\${TIMESTAMP}.tar.gz"
    fi

    # Link shared directories (logs, uploads, .env)
    for shared_path in logs uploads .env; do
        remote_exec "ln -sfn \${SHARED_DIR}/\${shared_path} \${release_dir}/\${shared_path}"
    done

    log_success "Release directory prepared"
}

activate_release() {
    local release_dir="\${RELEASES_DIR}/\${TIMESTAMP}"
    log_step "Activating release"
    remote_exec "ln -sfn \${release_dir} \${CURRENT_LINK}"
    remote_exec "systemctl reload \${APP_NAME} || systemctl restart \${APP_NAME}"
    log_success "Release activated"
}

cleanup_old_releases() {
    log_step "Cleaning up old releases (keeping \${KEEP_RELEASES})"
    remote_exec "ls -dt \${RELEASES_DIR}/* | tail -n +$((\${KEEP_RELEASES}+1)) | xargs rm -rf --"
    log_success "Cleanup done"
}

# ---------------------------------------------------------------------------
# Rollback
# ---------------------------------------------------------------------------

do_rollback() {
    log_step "Rolling back to previous release"
    local prev
    prev=$(remote_exec "ls -dt \${RELEASES_DIR}/* | sed -n '2p'")
    if [[ -z "\${prev}" ]]; then
        log_error "No previous release found — cannot roll back"
        exit 1
    fi
    remote_exec "ln -sfn \${prev} \${CURRENT_LINK}"
    remote_exec "systemctl reload \${APP_NAME} || systemctl restart \${APP_NAME}"
    log_success "Rolled back to \${prev}"
}

# ---------------------------------------------------------------------------
# Health check
# ---------------------------------------------------------------------------

health_check() {
    log_step "Running health check"
    local url="https://\${REMOTE_HOST}/health"
    local max_attempts=10
    local attempt=0
    local wait=5

    while (( attempt < max_attempts )); do
        local status
        status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "\${url}" || true)
        if [[ "\${status}" == "200" ]]; then
            log_success "Health check passed (\${url})"
            return 0
        fi
        attempt=$(( attempt + 1 ))
        log_info "Attempt \${attempt}/\${max_attempts} — HTTP \${status}; retrying in \${wait}s"
        sleep "\${wait}"
    done

    log_error "Health check failed after \${max_attempts} attempts"
    return 1
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    mkdir -p "$(dirname "\${LOG_FILE}")"

    log_step "Deploy started — env=\${ENV} branch=\${BRANCH} dry-run=\${DRY_RUN}"

    parse_args "$@"
    check_dependencies
    acquire_lock
    check_remote_connectivity

    if "\${ROLLBACK}"; then
        do_rollback
        health_check
        exit 0
    fi

    run_tests
    build_artefact
    prepare_release_dir
    activate_release
    health_check
    cleanup_old_releases

    log_success "\\nDeploy complete — \${ENV} @ \${TIMESTAMP}"
}

main "$@"
`;

// ---------------------------------------------------------------------------
// Ruby  (~400 lines) — ActiveRecord-style model + validations
// ---------------------------------------------------------------------------
export const RUBY_LARGE = `
# frozen_string_literal: true

# == Schema Information
#
# Table name: users
#
#  id              :bigint           not null, primary key
#  email           :string           not null
#  password_digest :string           not null
#  display_name    :string
#  role            :string           default("customer"), not null
#  confirmed_at    :datetime
#  locked_at       :datetime
#  failed_attempts :integer          default(0), not null
#  reset_token     :string
#  reset_sent_at   :datetime
#  created_at      :datetime         not null
#  updated_at      :datetime         not null
#
# Indexes
#  index_users_on_email (email) UNIQUE
#

class User < ApplicationRecord
  # ---------------------------------------------------------------------------
  # Concerns
  # ---------------------------------------------------------------------------
  include Auditable
  include Tokenizable

  # ---------------------------------------------------------------------------
  # Constants
  # ---------------------------------------------------------------------------
  ROLES          = %w[customer staff admin].freeze
  MAX_ATTEMPTS   = 5
  LOCK_DURATION  = 1.hour
  TOKEN_EXPIRY   = 2.hours

  # ---------------------------------------------------------------------------
  # Associations
  # ---------------------------------------------------------------------------
  has_many  :orders, dependent: :restrict_with_error
  has_many  :addresses, dependent: :destroy
  has_one   :profile, dependent: :destroy
  has_many  :audit_logs, dependent: :destroy, as: :auditable

  # ---------------------------------------------------------------------------
  # Authentication
  # ---------------------------------------------------------------------------
  has_secure_password

  # ---------------------------------------------------------------------------
  # Normalisation
  # ---------------------------------------------------------------------------
  before_validation :normalise_email

  # ---------------------------------------------------------------------------
  # Validations
  # ---------------------------------------------------------------------------
  validates :email,
            presence: true,
            uniqueness: { case_sensitive: false },
            format: { with: URI::MailTo::EMAIL_REGEXP }

  validates :display_name, length: { maximum: 100 }, allow_blank: true
  validates :role, inclusion: { in: ROLES }
  validates :failed_attempts, numericality: { greater_than_or_equal_to: 0 }

  # ---------------------------------------------------------------------------
  # Scopes
  # ---------------------------------------------------------------------------
  scope :active,       -> { where(locked_at: nil) }
  scope :confirmed,    -> { where.not(confirmed_at: nil) }
  scope :unconfirmed,  -> { where(confirmed_at: nil) }
  scope :admins,       -> { where(role: "admin") }
  scope :staff,        -> { where(role: %w[staff admin]) }
  scope :recent,       -> { order(created_at: :desc) }
  scope :locked,       -> { where.not(locked_at: nil) }

  # ---------------------------------------------------------------------------
  # Delegations
  # ---------------------------------------------------------------------------
  delegate :full_name, :avatar_url, to: :profile, allow_nil: true

  # ---------------------------------------------------------------------------
  # Class methods
  # ---------------------------------------------------------------------------
  class << self
    # Authenticate by email and password; returns the user or nil.
    #
    # @param email     [String]
    # @param password  [String]
    # @return [User, nil]
    def authenticate(email, password)
      user = find_by(email: email.downcase.strip)
      return unless user&.authenticate(password)
      return if user.locked?

      user.tap(&:record_successful_login!)
    end

    # Find a user by a password-reset token (not expired).
    #
    # @param token [String]
    # @return [User, nil]
    def find_by_reset_token(token)
      return unless token.present?

      find_by(reset_token: token)
        &.then { |u| u.reset_sent_at > TOKEN_EXPIRY.ago ? u : nil }
    end
  end

  # ---------------------------------------------------------------------------
  # Instance methods
  # ---------------------------------------------------------------------------

  # Whether the account is confirmed (email verified).
  def confirmed?
    confirmed_at.present?
  end

  # Whether the account is locked due to too many failed login attempts.
  def locked?
    locked_at.present? && locked_at > LOCK_DURATION.ago
  end

  # Whether the user has the given role or a higher-privilege role.
  def has_role?(check_role)
    ROLES.index(role.to_s).to_i >= ROLES.index(check_role.to_s).to_i
  end
  alias_method :role?, :has_role?

  # Confirm the email address.
  def confirm!
    update!(confirmed_at: Time.current)
  end

  # Record a successful authentication; resets the failure counter.
  def record_successful_login!
    return unless failed_attempts.positive?

    update_columns(failed_attempts: 0, locked_at: nil)
  end

  # Record a failed authentication attempt; locks the account when threshold reached.
  def record_failed_login!
    new_count = failed_attempts + 1
    attrs = { failed_attempts: new_count }
    attrs[:locked_at] = Time.current if new_count >= MAX_ATTEMPTS
    update_columns(attrs)
  end

  # Unlock a previously locked account.
  def unlock!
    update!(locked_at: nil, failed_attempts: 0)
  end

  # Generate and persist a password-reset token.
  #
  # @return [String] the plaintext token (only available immediately after generation)
  def generate_reset_token!
    token = SecureRandom.urlsafe_base64(32)
    update!(reset_token: token, reset_sent_at: Time.current)
    token
  end

  # Clear the password-reset token after successful password change.
  def clear_reset_token!
    update!(reset_token: nil, reset_sent_at: nil)
  end

  # ---------------------------------------------------------------------------
  # Serialisation helpers
  # ---------------------------------------------------------------------------

  # Returns a hash safe to expose via the public API (no sensitive fields).
  def to_public_hash
    {
      id:           id,
      email:        email,
      display_name: display_name || full_name,
      role:         role,
      confirmed:    confirmed?,
      avatar_url:   avatar_url,
      created_at:   created_at.iso8601,
    }
  end

  # ---------------------------------------------------------------------------
  # Private
  # ---------------------------------------------------------------------------

  private

  def normalise_email
    self.email = email.downcase.strip if email.present?
  end
end
`;

export const KOTLIN_LARGE = `
package com.example.data.repository

import com.example.data.api.ProductApiService
import com.example.data.db.ProductDao
import com.example.data.mapper.toDomain
import com.example.data.mapper.toEntity
import com.example.domain.model.Category
import com.example.domain.model.Product
import com.example.domain.model.ProductFilter
import com.example.domain.repository.ProductRepository
import com.example.util.Result
import com.example.util.runCatching
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import timber.log.Timber
import javax.inject.Inject

/**
 * Default implementation of [ProductRepository].
 *
 * Implements a cache-first strategy: data is served from the local Room
 * database and refreshed from the remote API on a configurable TTL.
 *
 * @param api          Retrofit service for product endpoints.
 * @param dao          Room DAO for local persistence.
 * @param dispatcher   Coroutine dispatcher for IO work.
 */
class ProductRepositoryImpl @Inject constructor(
    private val api: ProductApiService,
    private val dao: ProductDao,
    private val dispatcher: CoroutineDispatcher,
) : ProductRepository {

    // -------------------------------------------------------------------------
    // Products
    // -------------------------------------------------------------------------

    /**
     * Observe products in the given category.
     *
     * Emits cached data immediately, then refreshes from the network in the
     * background. Errors during refresh are logged but do not interrupt the flow.
     */
    override fun observeProducts(categoryId: Int): Flow<List<Product>> =
        dao.observeByCategory(categoryId)
            .map { entities -> entities.map { it.toDomain() } }
            .catch { e ->
                Timber.e(e, "Error observing products for category %d", categoryId)
                emit(emptyList())
            }
            .flowOn(dispatcher)

    /**
     * Fetch a single product by [id], trying the cache first.
     *
     * @return [Result.Success] with the product, or [Result.Error] if not found.
     */
    override suspend fun getProduct(id: String): Result<Product> =
        withContext(dispatcher) {
            runCatching {
                // Try cache first
                dao.getById(id)?.toDomain()
                    ?: run {
                        // Cache miss — fetch from API and persist
                        val dto = api.getProduct(id)
                        dao.upsert(dto.toEntity())
                        dto.toDomain()
                    }
            }
        }

    /**
     * Search products matching [filter].
     *
     * Always fetches from the network for freshness; results are also persisted
     * so they appear in subsequent offline observations.
     *
     * @return [Result.Success] with matching products, or [Result.Error].
     */
    override suspend fun searchProducts(filter: ProductFilter): Result<List<Product>> =
        withContext(dispatcher) {
            runCatching {
                val response = api.searchProducts(
                    query      = filter.query,
                    categoryId = filter.categoryId,
                    minPrice   = filter.minPrice?.toDouble(),
                    maxPrice   = filter.maxPrice?.toDouble(),
                    page       = filter.page,
                    pageSize   = filter.pageSize,
                )
                val entities = response.items.map { it.toEntity() }
                dao.upsertAll(entities)
                entities.map { it.toDomain() }
            }
        }

    /**
     * Refresh the cached product list for [categoryId].
     *
     * Called by the WorkManager periodic sync job.
     *
     * @return [Result.Success] with count of updated records, or [Result.Error].
     */
    override suspend fun refreshProducts(categoryId: Int): Result<Int> =
        withContext(dispatcher) {
            runCatching {
                val dtos = api.listProducts(categoryId = categoryId)
                val entities = dtos.map { it.toEntity() }
                dao.upsertAll(entities)
                entities.size
            }
        }

    // -------------------------------------------------------------------------
    // Categories
    // -------------------------------------------------------------------------

    /**
     * Observe the full category tree as a flat list.
     */
    override fun observeCategories(): Flow<List<Category>> =
        dao.observeCategories()
            .map { entities -> entities.map { it.toDomain() } }
            .catch { e ->
                Timber.e(e, "Error observing categories")
                emit(emptyList())
            }
            .flowOn(dispatcher)

    /**
     * Fetch categories from the network and update the local cache.
     *
     * @return [Result.Success] with the refreshed list.
     */
    override suspend fun refreshCategories(): Result<List<Category>> =
        withContext(dispatcher) {
            runCatching {
                val dtos = api.listCategories()
                val entities = dtos.map { it.toCategoryEntity() }
                dao.upsertCategories(entities)
                entities.map { it.toDomain() }
            }
        }

    // -------------------------------------------------------------------------
    // Favourites
    // -------------------------------------------------------------------------

    /** Observe IDs of products the user has favourited. */
    override fun observeFavouriteIds(): Flow<Set<String>> =
        dao.observeFavouriteIds()
            .map { it.toSet() }
            .flowOn(dispatcher)

    /**
     * Toggle the favourite state of a product.
     *
     * @return [Result.Success] with the new favourite state (true = added).
     */
    override suspend fun toggleFavourite(productId: String): Result<Boolean> =
        withContext(dispatcher) {
            runCatching {
                val isFav = dao.isFavourite(productId)
                if (isFav) {
                    dao.removeFavourite(productId)
                    false
                } else {
                    dao.addFavourite(productId)
                    true
                }
            }
        }

    // -------------------------------------------------------------------------
    // Cache management
    // -------------------------------------------------------------------------

    /**
     * Remove cached products older than [maxAgeMs] milliseconds.
     *
     * Called by the periodic maintenance job.
     */
    override suspend fun pruneExpiredCache(maxAgeMs: Long): Result<Int> =
        withContext(dispatcher) {
            runCatching {
                val threshold = System.currentTimeMillis() - maxAgeMs
                dao.deleteOlderThan(threshold)
            }
        }

    /**
     * Clear the entire product cache.
     *
     * Use with care — forces a full network refresh on next access.
     */
    override suspend fun clearCache(): Result<Unit> =
        withContext(dispatcher) {
            runCatching { dao.deleteAll() }
        }
}
`;

// ---------------------------------------------------------------------------
// YAML  (~400 lines) — GitHub Actions CI/CD workflow, realistic patterns
// ---------------------------------------------------------------------------
export const YAML_LARGE = `
# =============================================================================
# values.yaml — Production Helm chart values
# Full Kubernetes deployment configuration for a Node.js microservice
# =============================================================================

# ---------------------------------------------------------------------------
# Global settings shared across all sub-charts and templates
# ---------------------------------------------------------------------------
global:
  # Docker registry prefix — override per environment
  imageRegistry: "registry.example.com"
  # Storage class for all PersistentVolumeClaims
  storageClass: "gp3"
  # TLS secret name (must exist in the target namespace before install)
  tlsSecret: "wildcard-cert"
  # Common labels applied to every resource created by this chart
  commonLabels:
    team: platform
    project: api-gateway
    managed-by: helm

# ---------------------------------------------------------------------------
# Application image configuration
# ---------------------------------------------------------------------------
image:
  # Full image repository path (without tag)
  repository: registry.example.com/platform/api-gateway
  # Image pull policy. Use IfNotPresent for prod; Always during active dev.
  pullPolicy: IfNotPresent
  # Semver tag. Defaults to chart appVersion; set explicitly for pinned deploys.
  tag: ""
  # Digest-pinned SHA (preferred for production — overrides tag when set)
  # sha256: "abc123..."

# Image pull secrets for private registries
imagePullSecrets:
  - name: registry-credentials

# ---------------------------------------------------------------------------
# Replica and autoscaling settings
# ---------------------------------------------------------------------------

# Number of replicas when autoscaling is disabled
replicaCount: 3

# Horizontal Pod Autoscaler configuration
autoscaling:
  # Enable HPA — when true, replicaCount is treated as minReplicas
  enabled: true
  minReplicas: 3
  maxReplicas: 20
  # Target CPU utilisation percentage across all pods
  targetCPUUtilizationPercentage: 70
  # Target memory utilisation percentage (requires metrics-server >=0.6)
  targetMemoryUtilizationPercentage: 80

# Pod Disruption Budget — ensures at least N replicas available during rolling updates
podDisruptionBudget:
  enabled: true
  # Minimum available pods; use percentage string or integer
  minAvailable: "50%"

# ---------------------------------------------------------------------------
# Resource requests and limits
# ---------------------------------------------------------------------------
resources:
  requests:
    # CPU request — Kubernetes will only schedule on nodes with this capacity
    cpu: "250m"
    # Memory request — base allocation per pod
    memory: "256Mi"
  limits:
    # CPU limit — hard cap; throttled if exceeded
    cpu: "1000m"
    # Memory limit — OOMKilled if exceeded; tune based on profiling
    memory: "512Mi"

# ---------------------------------------------------------------------------
# Service configuration
# ---------------------------------------------------------------------------
service:
  # Service type: ClusterIP (internal), NodePort, LoadBalancer, ExternalName
  type: ClusterIP
  # Port exposed on the service
  port: 80
  # Container port the application listens on
  targetPort: 3000
  # Override protocol (TCP / UDP)
  protocol: TCP
  # Annotations for cloud-specific integrations (e.g. AWS NLB, GCP NEG)
  annotations: {}

# ---------------------------------------------------------------------------
# Ingress configuration
# ---------------------------------------------------------------------------
ingress:
  # Enable Kubernetes Ingress resource
  enabled: true
  # Ingress class name — matches installed ingress controller
  className: "nginx"
  # Annotations control NGINX behaviour, cert-manager, etc.
  annotations:
    nginx.ingress.kubernetes.io/proxy-body-size: "50m"
    nginx.ingress.kubernetes.io/proxy-read-timeout: "300"
    # TLS certificate issuer — requires cert-manager CRDs
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
    # Force HTTPS redirect
    nginx.ingress.kubernetes.io/force-ssl-redirect: "true"
  # Virtual hosts
  hosts:
    - host: api.example.com
      paths:
        - path: /
          pathType: Prefix
  # TLS termination — certificates managed by cert-manager
  tls:
    - secretName: api-example-tls
      hosts:
        - api.example.com

# ---------------------------------------------------------------------------
# Health checks
# ---------------------------------------------------------------------------
# Liveness probe: restart container if it fails consecutively
livenessProbe:
  httpGet:
    path: /healthz/live
    port: 3000
  # Wait this many seconds after container start before first probe
  initialDelaySeconds: 15
  # Probe interval in seconds
  periodSeconds: 10
  # Probe timeout — container must respond within this window
  timeoutSeconds: 5
  # Number of consecutive failures before restart
  failureThreshold: 3

# Readiness probe: remove pod from load-balancer endpoints if it fails
readinessProbe:
  httpGet:
    path: /healthz/ready
    port: 3000
  initialDelaySeconds: 5
  periodSeconds: 5
  timeoutSeconds: 3
  # Allow 1 failure before marking not-ready (avoids flapping on startup)
  failureThreshold: 1

# Startup probe: extra slack for slow-starting containers (overrides liveness)
startupProbe:
  httpGet:
    path: /healthz/ready
    port: 3000
  # Fail after 30 * 10s = 5 minutes — container must be ready by then
  failureThreshold: 30
  periodSeconds: 10

# ---------------------------------------------------------------------------
# Environment variables
# ---------------------------------------------------------------------------
env:
  # Application runtime environment
  - name: NODE_ENV
    value: "production"
  # Port the app binds to (must match targetPort)
  - name: PORT
    value: "3000"
  # Log verbosity: error | warn | info | debug
  - name: LOG_LEVEL
    value: "info"

# Secret environment variables (sourced from Kubernetes Secrets)
envFrom:
  - secretRef:
      # Must exist before helm install; created by external-secrets-operator
      name: api-gateway-secrets

# ---------------------------------------------------------------------------
# Persistent storage
# ---------------------------------------------------------------------------
persistence:
  # Enable persistent volume for ephemeral cache / session storage
  enabled: false
  # Storage class override (falls back to global.storageClass)
  storageClass: ""
  accessMode: ReadWriteOnce
  # Size of the volume
  size: 10Gi
  # Mount path inside the container
  mountPath: /data

# ---------------------------------------------------------------------------
# Security context
# ---------------------------------------------------------------------------
# Pod-level security context
podSecurityContext:
  # Run all containers as non-root GID 1001
  fsGroup: 1001
  # Prevent privilege escalation at the pod level
  runAsNonRoot: true

# Container-level security context
securityContext:
  # Drop all Linux capabilities — add back only what is needed
  capabilities:
    drop:
      - ALL
  # Read-only root filesystem — app must write to mounted volumes or /tmp
  readOnlyRootFilesystem: true
  # Do not allow privilege escalation within the container
  allowPrivilegeEscalation: false
  runAsNonRoot: true
  # Specific UID for the process — must match image USER directive
  runAsUser: 1001

# ---------------------------------------------------------------------------
# Affinity, tolerations, node selection
# ---------------------------------------------------------------------------

# Node affinity rules — prefer nodes in availability zone a or b
affinity:
  nodeAffinity:
    preferredDuringSchedulingIgnoredDuringExecution:
      - weight: 100
        preference:
          matchExpressions:
            - key: topology.kubernetes.io/zone
              operator: In
              values:
                - us-east-1a
                - us-east-1b

# Pod anti-affinity — spread replicas across nodes for HA
podAntiAffinity:
  preferredDuringSchedulingIgnoredDuringExecution:
    - weight: 100
      podAffinityTerm:
        labelSelector:
          matchLabels:
            app.kubernetes.io/name: api-gateway
        topologyKey: kubernetes.io/hostname

# Tolerate nodes tainted as dedicated
tolerations:
  - key: "dedicated"
    operator: "Equal"
    value: "api"
    effect: "NoSchedule"

# Node selector — additional label constraints on schedulable nodes
nodeSelector:
  kubernetes.io/os: linux
  node-role: worker

# ---------------------------------------------------------------------------
# ConfigMap — application configuration mounted as a volume
# ---------------------------------------------------------------------------
config:
  # Rate limiting window in milliseconds
  rateLimitWindowMs: 60000
  # Maximum requests per window per IP
  rateLimitMax: 100
  # JWT access token TTL
  jwtExpiresIn: "15m"
  # CORS allowed origins (comma-separated)
  corsOrigins: "https://app.example.com,https://admin.example.com"

# ---------------------------------------------------------------------------
# Service account
# ---------------------------------------------------------------------------
serviceAccount:
  # Create a dedicated service account (recommended over default)
  create: true
  # Annotations for IRSA / Workload Identity
  annotations:
    eks.amazonaws.com/role-arn: "arn:aws:iam::123456789012:role/api-gateway"
  # Override auto-generated name
  name: ""

# ---------------------------------------------------------------------------
# Monitoring
# ---------------------------------------------------------------------------
metrics:
  # Expose /metrics endpoint for Prometheus scraping
  enabled: true
  # ServiceMonitor CRD — requires prometheus-operator
  serviceMonitor:
    enabled: true
    # Scrape interval — align with global Prometheus scrape_interval
    interval: "30s"
    # Scrape timeout must be less than interval
    scrapeTimeout: "10s"
    # Labels matched by Prometheus serviceMonitorSelector
    labels:
      release: kube-prometheus-stack
`;

// ---------------------------------------------------------------------------
// Markdown (~380 lines) — Project README with badges, HTML comments, blockquotes
// ---------------------------------------------------------------------------
export const MD_LARGE = `
![CI](https://github.com/example/project/workflows/ci/badge.svg)
![Coverage](https://img.shields.io/codecov/c/github/example/project?color=green)
![npm version](https://img.shields.io/npm/v/@example/project.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Downloads](https://img.shields.io/npm/dm/@example/project.svg)
![Bundle Size](https://img.shields.io/bundlephobia/minzip/@example/project)

# Project Name

<!-- PROJECT DESCRIPTION -->
<!-- This section is auto-generated by the release workflow. Do not edit manually. -->

A high-performance TypeScript library for data transformation and validation.
Zero runtime dependencies. Fully tree-shakable.

<!-- SHIELDS END -->

<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**

- [Installation](#installation)
- [Quick Start](#quick-start)
- [API Reference](#api-reference)
- [Configuration](#configuration)
- [Examples](#examples)
- [Contributing](#contributing)
- [License](#license)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

[//]: # (The following content is synced from the documentation site.)
[//]: # (Changes made here will be overwritten on the next sync.)

## Installation

\`\`\`bash
npm install @example/project
# or
yarn add @example/project
# or
pnpm add @example/project
\`\`\`

## Requirements

- Node.js >= 18
- TypeScript >= 5.0 (for type definitions)

> **Note:** This library requires ES2020+ support. Use a bundler (Vite, webpack 5, esbuild) if targeting older environments.

> **Warning:** The v2 API is not backward-compatible with v1. See the [migration guide](#migration) for details.

## Quick Start

\`\`\`typescript
import { transform, validate } from '@example/project';

const schema = {
  name: { type: 'string', required: true },
  age:  { type: 'number', min: 0, max: 150 },
};

const result = validate({ name: 'Alice', age: 30 }, schema);
console.log(result.valid); // true
\`\`\`

## API Reference

<!-- API_START -->
<!-- Auto-generated from JSDoc. Run \`yarn docs\` to regenerate. -->

### \`transform(input, options?)\`

Transforms input data according to the provided options.

**Parameters:**

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| input | \`unknown\` | yes | — | Raw input value |
| options | \`TransformOptions\` | no | \`{}\` | Transformation options |

**Returns:** \`TransformResult<T>\`

**Throws:** \`ValidationError\` — when input violates a required constraint.

**Example:**

\`\`\`typescript
const result = transform(['a', 'b', 'b', 'c'], { unique: true });
// result.value === ['a', 'b', 'c']
\`\`\`

---

### \`validate(input, schema, options?)\`

Validates input against a schema definition.

| Parameter | Type | Description |
| --- | --- | --- |
| input | \`unknown\` | Value to validate |
| schema | \`Schema\` | Validation schema |
| options | \`ValidateOptions\` | Optional flags |

<!-- API_END -->

## Configuration

\`\`\`typescript
import { configure } from '@example/project';

configure({
  // Strict mode throws on the first validation error (default: false)
  strict: true,
  // Locale for error messages (default: 'en')
  locale: 'en',
  // Custom error formatter
  formatError: (err) => \`[\${err.field}] \${err.message}\`,
});
\`\`\`

### Environment Variables

| Variable | Description | Default |
| --- | --- | --- |
| \`PROJECT_STRICT\` | Enable strict mode | \`false\` |
| \`PROJECT_LOCALE\` | Error message locale | \`en\` |
| \`PROJECT_LOG_LEVEL\` | Log verbosity | \`warn\` |

## Examples

<!-- EXAMPLES_START -->

### Basic Validation

\`\`\`typescript
import { validate } from '@example/project';

const { valid, errors } = validate(userInput, userSchema);
if (!valid) {
  console.error(errors);
}
\`\`\`

### Async Transforms

\`\`\`typescript
import { transformAsync } from '@example/project';

const result = await transformAsync(rawData, {
  fetch: async (id) => await db.find(id),
});
\`\`\`

### Custom Rules

\`\`\`typescript
import { addRule } from '@example/project';

addRule('phoneNumber', (value) => {
  return /^[0-9+()\\ -]{7,15}$/.test(String(value));
});
\`\`\`

<!-- EXAMPLES_END -->

## Performance

Benchmark results on Node.js 20, Apple M2:

| Operation | Ops/sec | Relative |
| --- | ---: | --- |
| validate (small) | 2,450,000 | baseline |
| validate (large) | 89,000 | 27× slower |
| transform (small) | 1,800,000 | — |
| transform (large) | 61,000 | — |

> Results vary by schema complexity and input size. Run \`yarn bench\` for local benchmarks.

## Migration from v1

<!-- MIGRATION_START -->
<!-- This section is generated from CHANGELOG.md -->

The following breaking changes require migration:

1. **\`validate()\` now returns \`{ valid, errors }\`** instead of throwing directly.
2. **\`transform()\` options renamed**: \`deduplicate\` → \`unique\`, \`cast\` → \`coerce\`.
3. **Removed**: \`validateSync()\` — use \`validate()\` (it is synchronous by default).

\`\`\`diff
- import { validateSync } from '@example/project';
+ import { validate } from '@example/project';

- try { validateSync(data, schema); } catch (e) { ... }
+ const { valid, errors } = validate(data, schema);
+ if (!valid) { ... }
\`\`\`

<!-- MIGRATION_END -->

## Contributing

We welcome contributions! Please read [CONTRIBUTING.md](./CONTRIBUTING.md) before submitting a pull request.

### Development Setup

\`\`\`bash
git clone https://github.com/example/project.git
cd project
pnpm install
pnpm test
\`\`\`

### Code Style

- Run \`pnpm lint\` before committing.
- All public APIs must have JSDoc.
- Coverage must stay above 90%.

> **Important:** Sign the CLA before your first contribution.
> Bot will prompt you automatically on the PR.

## Security

Found a vulnerability? Please report it privately via [security@example.com](mailto:security@example.com) or the GitHub Security Advisory tab.
Do NOT open a public issue for security bugs.

<!-- SECURITY POLICY SYNCED FROM SECURITY.md -->
<!-- Last sync: 2026-06-01 -->

## License

MIT © Example Corp

<!-- FOOTER START -->
<!-- Auto-generated. Do not edit. -->
Made with ❤️ by the Platform team.
<!-- FOOTER END -->
`;

// ---------------------------------------------------------------------------
// PHP (~380 lines) — Laravel-style controller with PHPDoc
// ---------------------------------------------------------------------------
export const PHP_LARGE = `
<?php

declare(strict_types=1);

namespace App\\Http\\Controllers\\Api;

use App\\Http\\Controllers\\Controller;
use App\\Http\\Requests\\StoreUserRequest;
use App\\Http\\Requests\\UpdateUserRequest;
use App\\Http\\Resources\\UserResource;
use App\\Models\\User;
use App\\Services\\UserService;
use Illuminate\\Http\\JsonResponse;
use Illuminate\\Http\\Request;
use Illuminate\\Http\\Resources\\Json\\AnonymousResourceCollection;
use Illuminate\\Support\\Facades\\Auth;
use Illuminate\\Support\\Facades\\Cache;
use Illuminate\\Support\\Facades\\Log;

/**
 * User API controller.
 *
 * Handles CRUD operations for user accounts via the REST API.
 * All endpoints require authentication via Bearer token (see routes/api.php).
 *
 * @package App\\Http\\Controllers\\Api
 * @author  Platform Team <platform@example.com>
 * @version 2.3.0
 */
class UserController extends Controller
{
    /**
     * Number of items per paginated response.
     */
    private const PER_PAGE = 20;

    /**
     * Cache TTL for individual user records (seconds).
     */
    private const CACHE_TTL = 300;

    /**
     * Create a new controller instance.
     *
     * @param UserService $userService Injected user domain service
     */
    public function __construct(
        private readonly UserService $userService
    ) {
        // Apply auth middleware to all routes except index/show
        $this->middleware('auth:api')->except(['index', 'show']);
        // Rate-limit mutating operations to 60 requests per minute
        $this->middleware('throttle:60,1')->only(['store', 'update', 'destroy']);
    }

    /**
     * Return a paginated list of users.
     *
     * GET /api/users
     *
     * Supports the following query parameters:
     *   - search (string): full-text search on name and email
     *   - role   (string): filter by role slug
     *   - sort   (string): field name, prefix with - for descending (e.g. -created_at)
     *   - page   (int):    page number, 1-indexed (default: 1)
     *
     * @param  Request  $request
     * @return AnonymousResourceCollection
     */
    public function index(Request $request): AnonymousResourceCollection
    {
        $query = User::query()->active();

        if ($search = $request->query('search')) {
            $query->search($search);
        }

        if ($role = $request->query('role')) {
            $query->whereHas('roles', fn ($q) => $q->where('slug', $role));
        }

        if ($sort = $request->query('sort')) {
            $direction = str_starts_with($sort, '-') ? 'desc' : 'asc';
            $query->orderBy(ltrim($sort, '-'), $direction);
        } else {
            // Default sort: newest first
            $query->latest();
        }

        $users = $query->paginate(self::PER_PAGE)->withQueryString();

        return UserResource::collection($users);
    }

    /**
     * Return a single user by ID.
     *
     * GET /api/users/{id}
     *
     * Responses are cached for CACHE_TTL seconds keyed on the user ID.
     * Cache is invalidated on update or delete.
     *
     * @param  int  $id
     * @return UserResource
     *
     * @throws \\Illuminate\\Database\\Eloquent\\ModelNotFoundException
     */
    public function show(int $id): UserResource
    {
        $user = Cache::remember(
            "user:{$id}",
            self::CACHE_TTL,
            fn () => User::findOrFail($id)->load(['roles', 'profile'])
        );

        return new UserResource($user);
    }

    /**
     * Create a new user account.
     *
     * POST /api/users
     *
     * Validation is handled by StoreUserRequest (see app/Http/Requests/).
     * On success, fires the UserCreated event and sends a welcome email.
     *
     * @param  StoreUserRequest  $request  Validated request (auto-injected)
     * @return JsonResponse  HTTP 201 with the created user resource
     */
    public function store(StoreUserRequest $request): JsonResponse
    {
        $user = $this->userService->create($request->validated());

        Log::info('User created', [
            'id'         => $user->id,
            'created_by' => Auth::id(),
        ]);

        return (new UserResource($user))
            ->response()
            ->setStatusCode(201);
    }

    /**
     * Update an existing user account.
     *
     * PATCH /api/users/{id}
     *
     * Only the authenticated user may update their own account unless the
     * caller has the admin role.
     *
     * @param  UpdateUserRequest  $request  Validated request (auto-injected)
     * @param  int                $id       Target user ID
     * @return UserResource
     *
     * @throws \\Illuminate\\Auth\\Access\\AuthorizationException
     */
    public function update(UpdateUserRequest $request, int $id): UserResource
    {
        $user = User::findOrFail($id);

        // Authorise via policy — defined in app/Policies/UserPolicy.php
        $this->authorize('update', $user);

        $updated = $this->userService->update($user, $request->validated());

        // Invalidate the cached resource
        Cache::forget("user:{$id}");

        return new UserResource($updated);
    }

    /**
     * Soft-delete a user account.
     *
     * DELETE /api/users/{id}
     *
     * Soft deletion sets deleted_at; hard deletion is handled by the
     * scheduled PruneStaleUsers artisan command after 90 days.
     *
     * @param  int  $id
     * @return JsonResponse  HTTP 204 No Content
     *
     * @throws \\Illuminate\\Auth\\Access\\AuthorizationException
     */
    public function destroy(int $id): JsonResponse
    {
        $user = User::findOrFail($id);

        $this->authorize('delete', $user);

        $this->userService->softDelete($user);

        Cache::forget("user:{$id}");

        Log::info('User deleted', [
            'id'         => $id,
            'deleted_by' => Auth::id(),
        ]);

        return response()->json(null, 204);
    }

    /**
     * Restore a previously soft-deleted user.
     *
     * POST /api/users/{id}/restore
     *
     * @param  int  $id
     * @return UserResource
     *
     * @throws \\Illuminate\\Auth\\Access\\AuthorizationException
     */
    public function restore(int $id): UserResource
    {
        $user = User::withTrashed()->findOrFail($id);

        $this->authorize('restore', $user);

        $user->restore();

        return new UserResource($user);
    }

    /**
     * Bulk update user roles.
     *
     * POST /api/users/bulk-role
     *
     * Request body: { user_ids: int[], role_id: int }
     * Requires the admin role.
     *
     * @param  Request  $request
     * @return JsonResponse  HTTP 200 with update counts
     */
    public function bulkRole(Request $request): JsonResponse
    {
        $request->validate([
            'user_ids'   => 'required|array|min:1|max:500',
            'user_ids.*' => 'integer|exists:users,id',
            'role_id'    => 'required|integer|exists:roles,id',
        ]);

        $this->authorize('bulkUpdate', User::class);

        $count = $this->userService->assignRole(
            $request->input('user_ids'),
            $request->integer('role_id')
        );

        return response()->json(['updated' => $count]);
    }
}
`;

// ---------------------------------------------------------------------------
// C# (~360 lines) — ASP.NET Core service with XML docs
// ---------------------------------------------------------------------------
export const CS_LARGE = `
using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.Caching.Memory;
using Microsoft.Extensions.Logging;

namespace Example.Services
{
    /// <summary>
    /// Provides CRUD operations and business logic for <see cref="Order"/> entities.
    /// </summary>
    /// <remarks>
    /// This service is registered as a scoped dependency.
    /// It owns the <see cref="IMemoryCache"/> keying convention for orders.
    /// All mutating methods raise domain events via <see cref="IDomainEventDispatcher"/>.
    /// </remarks>
    public sealed class OrderService : IOrderService
    {
        /// <summary>Cache key prefix for individual orders.</summary>
        private const string CachePrefix = "order:";

        /// <summary>Default cache duration for order lookups.</summary>
        private static readonly TimeSpan CacheDuration = TimeSpan.FromMinutes(5);

        private readonly AppDbContext _db;
        private readonly IMemoryCache _cache;
        private readonly ILogger<OrderService> _logger;
        private readonly IDomainEventDispatcher _events;

        /// <summary>
        /// Initialises a new instance of <see cref="OrderService"/>.
        /// </summary>
        /// <param name="db">Entity Framework Core database context.</param>
        /// <param name="cache">In-process memory cache.</param>
        /// <param name="logger">Structured logger.</param>
        /// <param name="events">Domain-event dispatcher.</param>
        public OrderService(
            AppDbContext db,
            IMemoryCache cache,
            ILogger<OrderService> logger,
            IDomainEventDispatcher events)
        {
            _db     = db     ?? throw new ArgumentNullException(nameof(db));
            _cache  = cache  ?? throw new ArgumentNullException(nameof(cache));
            _logger = logger ?? throw new ArgumentNullException(nameof(logger));
            _events = events ?? throw new ArgumentNullException(nameof(events));
        }

        /// <inheritdoc />
        public async Task<Order?> GetByIdAsync(Guid id, CancellationToken ct = default)
        {
            var cacheKey = CachePrefix + id;

            if (_cache.TryGetValue<Order>(cacheKey, out var cached))
            {
                return cached;
            }

            var order = await _db.Orders
                .AsNoTracking()
                .Include(o => o.Lines)
                .FirstOrDefaultAsync(o => o.Id == id, ct);

            if (order is not null)
            {
                _cache.Set(cacheKey, order, CacheDuration);
            }

            return order;
        }

        /// <summary>
        /// Returns a paginated list of orders for a given customer.
        /// </summary>
        /// <param name="customerId">The customer whose orders to list.</param>
        /// <param name="page">1-based page index.</param>
        /// <param name="pageSize">Items per page (max 100).</param>
        /// <param name="ct">Cancellation token.</param>
        /// <returns>A page of orders sorted by creation date descending.</returns>
        public async Task<PagedResult<Order>> ListByCustomerAsync(
            Guid customerId,
            int page = 1,
            int pageSize = 20,
            CancellationToken ct = default)
        {
            // Clamp page size to prevent excessively large queries
            pageSize = Math.Min(pageSize, 100);

            var query = _db.Orders
                .AsNoTracking()
                .Where(o => o.CustomerId == customerId)
                .OrderByDescending(o => o.CreatedAt);

            var total = await query.CountAsync(ct);
            var items = await query
                .Skip((page - 1) * pageSize)
                .Take(pageSize)
                .ToListAsync(ct);

            return new PagedResult<Order>(items, total, page, pageSize);
        }

        /// <summary>
        /// Places a new order for the specified customer.
        /// </summary>
        /// <param name="customerId">Customer placing the order.</param>
        /// <param name="lines">One or more order line items.</param>
        /// <param name="ct">Cancellation token.</param>
        /// <returns>The newly created order.</returns>
        /// <exception cref="ArgumentException">
        /// Thrown when <paramref name="lines"/> is empty.
        /// </exception>
        /// <exception cref="InvalidOperationException">
        /// Thrown when a product is out of stock.
        /// </exception>
        public async Task<Order> PlaceOrderAsync(
            Guid customerId,
            IEnumerable<OrderLineRequest> lines,
            CancellationToken ct = default)
        {
            var lineList = lines?.ToList()
                ?? throw new ArgumentNullException(nameof(lines));

            if (lineList.Count == 0)
            {
                throw new ArgumentException("Order must contain at least one line.", nameof(lines));
            }

            // Validate stock levels before opening a transaction
            foreach (var line in lineList)
            {
                var stock = await _db.Products
                    .Where(p => p.Id == line.ProductId)
                    .Select(p => p.StockQuantity)
                    .FirstOrDefaultAsync(ct);

                if (stock < line.Quantity)
                {
                    throw new InvalidOperationException(
                        $"Product {line.ProductId} has insufficient stock ({stock} available).");
                }
            }

            await using var tx = await _db.Database.BeginTransactionAsync(ct);

            try
            {
                var order = Order.Create(customerId, lineList);
                _db.Orders.Add(order);

                // Decrement stock atomically
                foreach (var line in lineList)
                {
                    await _db.Products
                        .Where(p => p.Id == line.ProductId)
                        .ExecuteUpdateAsync(s =>
                            s.SetProperty(p => p.StockQuantity,
                                p => p.StockQuantity - line.Quantity), ct);
                }

                await _db.SaveChangesAsync(ct);
                await tx.CommitAsync(ct);

                _logger.LogInformation(
                    "Order {OrderId} placed for customer {CustomerId} with {LineCount} lines.",
                    order.Id, customerId, lineList.Count);

                await _events.DispatchAsync(new OrderPlacedEvent(order), ct);

                return order;
            }
            catch
            {
                await tx.RollbackAsync(ct);
                throw;
            }
        }

        /// <summary>
        /// Cancels an order if it is still in a cancellable state.
        /// </summary>
        /// <param name="id">Order to cancel.</param>
        /// <param name="reason">Human-readable cancellation reason (audit log).</param>
        /// <param name="ct">Cancellation token.</param>
        /// <exception cref="InvalidOperationException">
        /// Thrown when the order is not in a cancellable state.
        /// </exception>
        public async Task CancelAsync(Guid id, string reason, CancellationToken ct = default)
        {
            var order = await _db.Orders
                .Include(o => o.Lines)
                .FirstOrDefaultAsync(o => o.Id == id, ct)
                ?? throw new KeyNotFoundException($"Order {id} not found.");

            if (!order.CanCancel())
            {
                throw new InvalidOperationException(
                    $"Order {id} in status '{order.Status}' cannot be cancelled.");
            }

            order.Cancel(reason);

            // Restore stock for each cancelled line
            foreach (var line in order.Lines)
            {
                await _db.Products
                    .Where(p => p.Id == line.ProductId)
                    .ExecuteUpdateAsync(s =>
                        s.SetProperty(p => p.StockQuantity,
                            p => p.StockQuantity + line.Quantity), ct);
            }

            await _db.SaveChangesAsync(ct);

            // Bust the cache so callers see the updated status immediately
            _cache.Remove(CachePrefix + id);

            await _events.DispatchAsync(new OrderCancelledEvent(order, reason), ct);
        }
    }
}
`;

// ---------------------------------------------------------------------------
// GraphQL (~310 lines) — schema with comments and descriptions
// ---------------------------------------------------------------------------
export const GRAPHQL_LARGE = `
"""
Root schema for the e-commerce API.
Generated by GraphQL Code Generator from TypeScript source.
See docs/graphql.md for naming conventions and pagination patterns.
"""

# --------------------------------------------------------------------------
# Scalars
# --------------------------------------------------------------------------

"""ISO-8601 date-time string, always in UTC."""
scalar DateTime

"""Opaque cursor used for keyset pagination."""
scalar Cursor

"""URL string validated against RFC 3986."""
scalar URL

"""Non-negative integer representing a monetary amount in minor units (cents)."""
scalar Money

# --------------------------------------------------------------------------
# Enums
# --------------------------------------------------------------------------

"""
Lifecycle state of an order.

Transitions: PENDING → CONFIRMED → SHIPPED → DELIVERED
             PENDING → CANCELLED
             CONFIRMED → CANCELLED
"""
enum OrderStatus {
  """Order received but payment not yet confirmed."""
  PENDING
  """Payment confirmed; awaiting fulfilment."""
  CONFIRMED
  """Package handed to carrier."""
  SHIPPED
  """Package received by customer."""
  DELIVERED
  """Order cancelled before delivery."""
  CANCELLED
}

"""User account role — controls API access permissions."""
enum Role {
  """Standard customer account."""
  CUSTOMER
  """Seller with product management access."""
  VENDOR
  """Full administrative access."""
  ADMIN
}

"""Sort direction for list queries."""
enum SortDirection {
  ASC
  DESC
}

# --------------------------------------------------------------------------
# Connection/pagination types (Relay spec)
# --------------------------------------------------------------------------

"""
Pagination metadata returned with every paginated list.
Follows the Relay Connection specification.
"""
type PageInfo {
  """Whether additional pages exist after the current page."""
  hasNextPage: Boolean!
  """Whether additional pages exist before the current page."""
  hasPreviousPage: Boolean!
  """Cursor of the first item in this page. Use with \`before:\` to paginate back."""
  startCursor: Cursor
  """Cursor of the last item in this page. Use with \`after:\` to paginate forward."""
  endCursor: Cursor
}

"""Wrapper around a single node in a paginated list."""
type OrderEdge {
  cursor: Cursor!
  node: Order!
}

"""Paginated list of orders."""
type OrderConnection {
  edges: [OrderEdge!]!
  pageInfo: PageInfo!
  """Total number of orders matching the query (ignores pagination)."""
  totalCount: Int!
}

# --------------------------------------------------------------------------
# Core domain types
# --------------------------------------------------------------------------

"""Registered user account."""
type User {
  id: ID!
  """Display name (not unique)."""
  name: String!
  """Primary email address (unique, lowercased on write)."""
  email: String!
  role: Role!
  createdAt: DateTime!
  updatedAt: DateTime!
  """Profile photo, resized to 256×256 on upload."""
  avatarUrl: URL
  """Orders placed by this user (most recent first)."""
  orders(
    first: Int
    after: Cursor
    status: OrderStatus
  ): OrderConnection!
}

"""A product available for purchase."""
type Product {
  id: ID!
  """Short product title — max 120 characters."""
  title: String!
  """Full HTML description. Sanitised server-side."""
  description: String
  """Price in minor units (cents). Always positive."""
  priceAmount: Money!
  """ISO-4217 currency code."""
  priceCurrency: String!
  """Remaining inventory. Null means inventory tracking is disabled."""
  stock: Int
  """Primary product image."""
  imageUrl: URL
  """Whether the product is currently listed and purchasable."""
  active: Boolean!
  createdAt: DateTime!
  updatedAt: DateTime!
}

"""A single line item within an order."""
type OrderLine {
  id: ID!
  product: Product!
  """Quantity ordered."""
  quantity: Int!
  """Unit price captured at order time (in minor units)."""
  unitPrice: Money!
  """Line total = quantity × unitPrice."""
  totalPrice: Money!
}

"""A customer order."""
type Order {
  id: ID!
  status: OrderStatus!
  customer: User!
  lines: [OrderLine!]!
  """Sum of all line totals."""
  totalAmount: Money!
  """Currency of totalAmount."""
  currency: String!
  """Shipping address as a free-text block."""
  shippingAddress: String
  """Carrier tracking number (available once SHIPPED)."""
  trackingNumber: String
  createdAt: DateTime!
  updatedAt: DateTime!
}

# --------------------------------------------------------------------------
# Input types
# --------------------------------------------------------------------------

"""Input for creating a new order."""
input CreateOrderInput {
  """Customer ID placing the order."""
  customerId: ID!
  """One or more line items."""
  lines: [OrderLineInput!]!
  """Optional shipping address override."""
  shippingAddress: String
}

"""A single product line within CreateOrderInput."""
input OrderLineInput {
  productId: ID!
  quantity: Int!
}

"""Filter options for listing orders."""
input OrdersFilter {
  status: OrderStatus
  customerId: ID
  """Only orders created on or after this timestamp."""
  createdAfter: DateTime
  """Only orders created before this timestamp."""
  createdBefore: DateTime
}

# --------------------------------------------------------------------------
# Queries
# --------------------------------------------------------------------------

type Query {
  """Look up a user by ID. Returns null if the user does not exist."""
  user(id: ID!): User

  """Return the currently authenticated user."""
  me: User

  """List users. Requires ADMIN role."""
  users(first: Int, after: Cursor, role: Role): OrderConnection!

  """Look up a product by ID."""
  product(id: ID!): Product

  """Search active products by title."""
  products(query: String, first: Int, after: Cursor): OrderConnection!

  """Look up an order by ID."""
  order(id: ID!): Order

  """List orders with optional filters."""
  orders(
    filter: OrdersFilter
    first: Int
    after: Cursor
    sortBy: String
    sortDirection: SortDirection
  ): OrderConnection!
}

# --------------------------------------------------------------------------
# Mutations
# --------------------------------------------------------------------------

type Mutation {
  """Place a new order. Returns the created order."""
  createOrder(input: CreateOrderInput!): Order!

  """Cancel a pending or confirmed order."""
  cancelOrder(id: ID!, reason: String): Order!

  """Update the authenticated user's profile."""
  updateProfile(name: String, avatarUrl: URL): User!
}
`;

// ---------------------------------------------------------------------------
// HTML (~300 lines) — documentation page with inline comments
// ---------------------------------------------------------------------------
export const HTML_LARGE = `<!DOCTYPE html>
<!-- Generated by docs-builder v4.2.1 — DO NOT EDIT MANUALLY -->
<!-- Build timestamp: 2026-06-11T12:00:00Z -->
<html lang="en" data-theme="light">
<head>
  <!-- Required meta tags -->
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta name="description" content="API Reference — Example Project">
  <meta name="author" content="Platform Team">
  <!-- Open Graph / Twitter card meta -->
  <meta property="og:type" content="website">
  <meta property="og:title" content="API Reference">
  <meta property="og:description" content="Complete API reference for Example Project">
  <meta property="og:url" content="https://docs.example.com/api">
  <meta name="twitter:card" content="summary_large_image">
  <title>API Reference — Example Project</title>
  <!-- Canonical stylesheet -->
  <link rel="stylesheet" href="/assets/docs.css">
  <!-- Prism syntax highlighting — loaded async to avoid render blocking -->
  <link rel="stylesheet" href="/assets/prism.css" media="print" onload="this.media='all'">
  <!-- Favicon set -->
  <link rel="icon" type="image/png" sizes="32x32" href="/favicon-32.png">
  <link rel="icon" type="image/png" sizes="16x16" href="/favicon-16.png">
  <!-- Preconnect to CDN for performance -->
  <link rel="preconnect" href="https://cdn.example.com" crossorigin>
</head>
<body>
  <!-- ===================================================================
       Top navigation bar
       Sticky on scroll; collapses to hamburger on mobile (< 768px)
       =================================================================== -->
  <header id="top-nav" role="banner">
    <nav aria-label="Primary navigation">
      <a href="/" class="nav-logo" aria-label="Home">
        <img src="/logo.svg" alt="Example Project" width="120" height="32">
      </a>
      <!-- Desktop nav links -->
      <ul class="nav-links" role="list">
        <li><a href="/docs/guide">Guide</a></li>
        <li><a href="/docs/api" aria-current="page">API</a></li>
        <li><a href="/docs/examples">Examples</a></li>
        <li><a href="https://github.com/example/project" rel="noopener">GitHub</a></li>
      </ul>
      <!-- Version picker — JS-powered dropdown -->
      <div id="version-picker" aria-label="Documentation version">
        <button type="button" aria-haspopup="listbox" aria-expanded="false">
          v2.3 <span aria-hidden="true">▾</span>
        </button>
        <ul role="listbox" hidden>
          <li role="option" aria-selected="true">v2.3 (current)</li>
          <li role="option">v2.2</li>
          <li role="option">v1.x (legacy)</li>
        </ul>
      </div>
    </nav>
  </header>

  <!-- ===================================================================
       Sidebar — auto-generated from page headings by docs-builder
       =================================================================== -->
  <aside id="sidebar" aria-label="Page outline">
    <nav aria-label="Table of contents">
      <h2 class="sidebar-title">On this page</h2>
      <ul id="toc" role="list">
        <!-- Populated at runtime by toc.js -->
      </ul>
    </nav>
  </aside>

  <!-- ===================================================================
       Main content
       =================================================================== -->
  <main id="content" tabindex="-1">
    <article>
      <header class="page-header">
        <h1 id="api-reference">API Reference</h1>
        <p class="lead">
          Complete reference for all public functions, types, and configuration options.
        </p>
        <!-- Edit-on-GitHub link — hidden until JS sets the href -->
        <a id="edit-link" href="#" hidden>Edit this page</a>
      </header>

      <!-- Section: Installation -->
      <section id="installation" aria-labelledby="installation-heading">
        <h2 id="installation-heading">Installation</h2>
        <div class="code-block" data-language="bash">
          <pre><code class="language-bash">npm install @example/project</code></pre>
          <!-- Copy button injected by clipboard.js -->
          <button class="copy-btn" aria-label="Copy code to clipboard" data-clipboard-target="#install-snippet">
            Copy
          </button>
        </div>
      </section>

      <!-- Section: Core API -->
      <section id="core-api" aria-labelledby="core-api-heading">
        <h2 id="core-api-heading">Core API</h2>

        <!-- transform() -->
        <div id="transform" class="api-entry">
          <h3 class="api-name">
            <code>transform(input, options?)</code>
            <!-- Stability badge — values: stable | experimental | deprecated -->
            <span class="badge badge--stable" title="Stable API">stable</span>
          </h3>
          <p>Transforms input data according to the provided options.</p>
          <h4>Parameters</h4>
          <table class="params-table">
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">Type</th>
                <th scope="col">Required</th>
                <th scope="col">Description</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td><code>input</code></td>
                <td><code>unknown</code></td>
                <td>Yes</td>
                <td>Raw value to transform.</td>
              </tr>
              <tr>
                <td><code>options</code></td>
                <td><code>TransformOptions</code></td>
                <td>No</td>
                <td>Configuration options (see below).</td>
              </tr>
            </tbody>
          </table>
        </div>

        <!-- validate() -->
        <div id="validate" class="api-entry">
          <h3 class="api-name">
            <code>validate(input, schema, options?)</code>
            <span class="badge badge--stable">stable</span>
          </h3>
          <p>Validates <code>input</code> against a JSON Schema-compatible <code>schema</code>.</p>
          <!-- Deprecation notice — rendered by docs-builder when deprecated: true -->
          <!-- <div class="notice notice--warning">...</div> -->
        </div>
      </section>

      <!-- Section: Types -->
      <section id="types" aria-labelledby="types-heading">
        <h2 id="types-heading">Types</h2>
        <div id="transform-options" class="api-entry">
          <h3><code>TransformOptions</code></h3>
          <!-- Auto-linked to TypeScript source by docs-builder -->
          <dl class="type-props">
            <dt><code>unique</code> <span class="type-badge">boolean</span></dt>
            <dd>Deduplicate array items (default: <code>false</code>).</dd>
            <dt><code>coerce</code> <span class="type-badge">boolean</span></dt>
            <dd>Coerce primitive types (default: <code>false</code>).</dd>
            <dt><code>strict</code> <span class="type-badge">boolean</span></dt>
            <dd>Throw on the first error instead of collecting all errors.</dd>
          </dl>
        </div>
      </section>
    </article>
  </main>

  <!-- ===================================================================
       Footer
       =================================================================== -->
  <footer id="site-footer" role="contentinfo">
    <p>
      &copy; 2026 Example Corp.
      <a href="/legal/privacy">Privacy</a> ·
      <a href="/legal/terms">Terms</a>
    </p>
    <!-- Build info injected at deploy time -->
    <!-- build: v2.3.1 / sha: abc1234 -->
  </footer>

  <!-- Scripts — deferred to avoid blocking render -->
  <script src="/assets/docs.js" defer></script>
  <!-- Prism.js for syntax highlighting in code blocks -->
  <script src="/assets/prism.js" defer></script>
</body>
</html>`;

// ---------------------------------------------------------------------------
// Protobuf (~260 lines) — gRPC service definition with comments
// ---------------------------------------------------------------------------
export const PROTO_LARGE = `
syntax = "proto3";

// ==========================================================================
// Package: example.commerce.v1
//
// This package defines the gRPC service contracts for the commerce domain.
// All messages conform to the Google AIP field naming conventions.
// See: https://google.aip.dev/
// ==========================================================================

package example.commerce.v1;

option java_package = "com.example.commerce.v1";
option java_multiple_files = true;
option go_package = "github.com/example/commerce/gen/go;commercev1";
option csharp_namespace = "Example.Commerce.V1";

import "google/protobuf/timestamp.proto";
import "google/protobuf/field_mask.proto";
import "google/protobuf/wrappers.proto";
import "google/api/annotations.proto";
import "validate/validate.proto";

// --------------------------------------------------------------------------
// Enumerations
// --------------------------------------------------------------------------

// OrderStatus represents the lifecycle state of an order.
// Transitions are enforced server-side; the API rejects illegal transitions.
enum OrderStatus {
  // Default value — must not be used by clients.
  ORDER_STATUS_UNSPECIFIED = 0;
  // Order received; awaiting payment confirmation.
  ORDER_STATUS_PENDING = 1;
  // Payment confirmed; order queued for fulfilment.
  ORDER_STATUS_CONFIRMED = 2;
  // Order picked and handed to carrier.
  ORDER_STATUS_SHIPPED = 3;
  // Carrier confirmed delivery.
  ORDER_STATUS_DELIVERED = 4;
  // Order cancelled by customer or admin.
  ORDER_STATUS_CANCELLED = 5;
}

// --------------------------------------------------------------------------
// Messages
// --------------------------------------------------------------------------

// Money represents a monetary amount.
// Using explicit amount + currency to avoid floating-point rounding issues.
message Money {
  // Amount in the minor unit of the currency (e.g. cents for USD).
  int64 amount_minor = 1;
  // ISO-4217 currency code (e.g. "USD", "EUR").
  string currency_code = 2 [(validate.rules).string = {min_len: 3, max_len: 3}];
}

// Address represents a postal address.
message Address {
  string line1       = 1 [(validate.rules).string.min_len = 1];
  string line2       = 2;
  string city        = 3 [(validate.rules).string.min_len = 1];
  string state       = 4;
  // Postal code — format varies by country; validated at the application layer.
  string postal_code = 5;
  // ISO-3166-1 alpha-2 country code.
  string country_code = 6 [(validate.rules).string = {min_len: 2, max_len: 2}];
}

// OrderLine represents a single product line within an order.
message OrderLine {
  // Server-assigned immutable identifier.
  string id = 1;
  // Referenced product identifier.
  string product_id = 2 [(validate.rules).string.min_len = 1];
  // Human-readable product title captured at order time.
  string product_title = 3;
  // Quantity ordered — must be positive.
  int32 quantity = 4 [(validate.rules).int32.gt = 0];
  // Unit price at the time of order (snapshot, not live).
  Money unit_price = 5;
  // Computed: quantity × unit_price.
  Money total_price = 6;
}

// Order is the aggregate root for a customer purchase.
message Order {
  // Server-assigned UUID (output only).
  string id = 1;
  // ID of the customer who placed the order.
  string customer_id = 2;
  // Current lifecycle status.
  OrderStatus status = 3;
  // One or more line items.
  repeated OrderLine lines = 4;
  // Sum of all line totals.
  Money total_amount = 5;
  // Shipping destination.
  Address shipping_address = 6;
  // Carrier tracking number — populated after SHIPPED transition.
  google.protobuf.StringValue tracking_number = 7;
  // ISO-8601 timestamp when the order was created.
  google.protobuf.Timestamp created_at = 8;
  // ISO-8601 timestamp of the most recent update.
  google.protobuf.Timestamp updated_at = 9;
}

// --------------------------------------------------------------------------
// Request / Response messages
// --------------------------------------------------------------------------

// Request message for GetOrder.
message GetOrderRequest {
  // Required. The order resource name: "orders/{id}".
  string name = 1 [(validate.rules).string.min_len = 1];
}

// Request message for ListOrders.
message ListOrdersRequest {
  // Customer filter — return only orders for this customer.
  string customer_id = 1;
  // Status filter — omit to return all statuses.
  OrderStatus status = 2;
  // Maximum number of orders to return (default: 20, max: 100).
  int32 page_size = 3 [(validate.rules).int32 = {gte: 0, lte: 100}];
  // Pagination token from a previous ListOrders response.
  string page_token = 4;
}

// Response message for ListOrders.
message ListOrdersResponse {
  // The returned orders.
  repeated Order orders = 1;
  // Pagination token for the next page. Empty if no more pages.
  string next_page_token = 2;
  // Total count of matching orders (regardless of page_size).
  int32 total_count = 3;
}

// Request message for CreateOrder.
message CreateOrderRequest {
  // Required. The customer placing the order.
  string customer_id = 1 [(validate.rules).string.min_len = 1];
  // Required. At least one line item.
  repeated OrderLine lines = 2 [(validate.rules).repeated.min_items = 1];
  // Optional shipping address — falls back to customer default.
  Address shipping_address = 3;
}

// Request message for CancelOrder.
message CancelOrderRequest {
  // Required. The order resource name.
  string name = 1 [(validate.rules).string.min_len = 1];
  // Optional free-text reason stored in the audit log.
  string reason = 2;
}

// --------------------------------------------------------------------------
// Service definition
// --------------------------------------------------------------------------

// OrderService provides CRUD and lifecycle operations on orders.
//
// Authentication: all RPCs require a valid Bearer token (see auth.proto).
// Rate limits:    GET methods 600 rpm; mutations 60 rpm per client.
service OrderService {
  // Retrieve a single order by resource name.
  rpc GetOrder(GetOrderRequest) returns (Order) {
    option (google.api.http) = {
      get: "/v1/{name=orders/*}"
    };
  }

  // List orders with optional filtering and pagination.
  rpc ListOrders(ListOrdersRequest) returns (ListOrdersResponse) {
    option (google.api.http) = {
      get: "/v1/orders"
    };
  }

  // Place a new order.
  rpc CreateOrder(CreateOrderRequest) returns (Order) {
    option (google.api.http) = {
      post: "/v1/orders"
      body: "*"
    };
  }

  // Cancel an order.
  rpc CancelOrder(CancelOrderRequest) returns (Order) {
    option (google.api.http) = {
      post: "/v1/{name=orders/*}:cancel"
      body: "*"
    };
  }
}
`;

// ---------------------------------------------------------------------------
// Bash (~290 lines) — deployment script with heavy comment documentation
// ---------------------------------------------------------------------------
export const BASH_LARGE = `#!/usr/bin/env bash
# =============================================================================
# deploy.sh — Zero-downtime blue/green deployment script
#
# Usage:
#   ./deploy.sh [OPTIONS] <environment>
#
# Arguments:
#   environment   Target environment: staging | production
#
# Options:
#   -i, --image TAG     Docker image tag to deploy (required)
#   -n, --namespace NS  Kubernetes namespace (default: derived from env)
#   -d, --dry-run       Print actions without executing
#   -v, --verbose       Enable verbose logging
#   -h, --help          Show this help message
#
# Examples:
#   ./deploy.sh -i v1.4.2 staging
#   ./deploy.sh --image v2.0.0 --namespace prod-east production
#   ./deploy.sh --dry-run -i v1.4.2 production
#
# Requirements:
#   kubectl  >= 1.28
#   helm     >= 3.12
#   jq       >= 1.6
#   aws-cli  >= 2.x  (for ECR login)
#
# Exit codes:
#   0  Success
#   1  General error
#   2  Precondition failed (missing tool / wrong env)
#   3  Deployment failed
#   4  Health-check timeout
# =============================================================================

set -euo pipefail
IFS=$'\\n\\t'

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

readonly SCRIPT_DIR="$(cd "$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
readonly CHART_PATH="\${SCRIPT_DIR}/../helm/api-gateway"
readonly MAX_WAIT_SECONDS=300   # 5 minutes rollout timeout
readonly HEALTH_CHECK_INTERVAL=10
readonly LOG_PREFIX="[deploy]"

# ---------------------------------------------------------------------------
# Logging helpers
# ---------------------------------------------------------------------------

log()  { echo "\${LOG_PREFIX} $*" >&2; }
info() { echo "\${LOG_PREFIX} INFO  $*" >&2; }
warn() { echo "\${LOG_PREFIX} WARN  $*" >&2; }
err()  { echo "\${LOG_PREFIX} ERROR $*" >&2; }

die() {
  err "$1"
  exit "\${2:-1}"
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

DRY_RUN=false
VERBOSE=false
IMAGE_TAG=""
NAMESPACE=""
ENVIRONMENT=""

usage() {
  grep '^#' "\${BASH_SOURCE[0]}" | sed 's/^# \\?//'
  exit 0
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      -i|--image)     IMAGE_TAG="$2";    shift 2 ;;
      -n|--namespace) NAMESPACE="$2";   shift 2 ;;
      -d|--dry-run)   DRY_RUN=true;      shift   ;;
      -v|--verbose)   VERBOSE=true;      shift   ;;
      -h|--help)      usage ;;
      --) shift; break ;;
      -*) die "Unknown option: $1" 2 ;;
      *)  ENVIRONMENT="$1"; shift ;;
    esac
  done

  [[ -z "\${IMAGE_TAG}"    ]] && die "Missing required option: --image" 2
  [[ -z "\${ENVIRONMENT}"  ]] && die "Missing required argument: environment" 2
}

# ---------------------------------------------------------------------------
# Preconditions
# ---------------------------------------------------------------------------

check_tools() {
  # Verify all required tools are installed and reachable on PATH
  local tools=(kubectl helm jq aws)
  for tool in "\${tools[@]}"; do
    if ! command -v "$tool" &>/dev/null; then
      die "Required tool not found: $tool" 2
    fi
  done
  info "All required tools found."
}

check_cluster_access() {
  # Verify kubectl is authenticated and can reach the API server
  if ! kubectl cluster-info &>/dev/null; then
    die "kubectl cannot reach the cluster. Check KUBECONFIG / VPN." 2
  fi
  info "Cluster access confirmed: $(kubectl config current-context)"
}

# Resolve namespace from environment when not explicitly overridden
resolve_namespace() {
  if [[ -z "\${NAMESPACE}" ]]; then
    case "\${ENVIRONMENT}" in
      staging)    NAMESPACE="staging"    ;;
      production) NAMESPACE="production" ;;
      *)          die "Unknown environment: \${ENVIRONMENT}" 2 ;;
    esac
  fi
  info "Deploying to namespace: \${NAMESPACE}"
}

# ---------------------------------------------------------------------------
# ECR login
# ---------------------------------------------------------------------------

ecr_login() {
  # Log in to AWS Elastic Container Registry before pulling/pushing images.
  # Credentials come from the instance role (no explicit AWS_* vars needed in CI).
  local region
  region="$(aws configure get region 2>/dev/null || echo us-east-1)"
  local account_id
  account_id="$(aws sts get-caller-identity --query Account --output text)"

  info "Logging in to ECR (\${account_id}.dkr.ecr.\${region}.amazonaws.com)..."
  aws ecr get-login-password --region "\${region}" \\
    | docker login --username AWS --password-stdin \\
        "\${account_id}.dkr.ecr.\${region}.amazonaws.com"
}

# ---------------------------------------------------------------------------
# Helm deploy
# ---------------------------------------------------------------------------

helm_deploy() {
  local release="api-gateway-\${ENVIRONMENT}"
  local values_file="\${CHART_PATH}/values-\${ENVIRONMENT}.yaml"

  [[ -f "\${values_file}" ]] || die "Values file not found: \${values_file}" 2

  local helm_cmd=(
    helm upgrade --install "\${release}" "\${CHART_PATH}"
    --namespace "\${NAMESPACE}"
    --create-namespace
    --values "\${values_file}"
    --set "image.tag=\${IMAGE_TAG}"
    --set "deploy.timestamp=$(date -u +%Y%m%dT%H%M%SZ)"
    --atomic
    --timeout "\${MAX_WAIT_SECONDS}s"
    --wait
  )

  if [[ "\${DRY_RUN}" == "true" ]]; then
    info "DRY RUN — would execute: \${helm_cmd[*]}"
    return 0
  fi

  info "Running helm upgrade/install for release '\${release}'..."
  "\${helm_cmd[@]}" || die "Helm deploy failed." 3
  info "Helm deploy succeeded."
}

# ---------------------------------------------------------------------------
# Health verification
# ---------------------------------------------------------------------------

wait_for_rollout() {
  # Poll the deployment rollout status until all pods are ready or timeout.
  local deployment="api-gateway"
  local elapsed=0

  info "Waiting for rollout of '\${deployment}' in '\${NAMESPACE}'..."

  while [[ \${elapsed} -lt \${MAX_WAIT_SECONDS} ]]; do
    local ready
    ready="$(kubectl get deployment "\${deployment}" -n "\${NAMESPACE}" \\
              -o jsonpath='{.status.readyReplicas}' 2>/dev/null || echo 0)"
    local desired
    desired="$(kubectl get deployment "\${deployment}" -n "\${NAMESPACE}" \\
               -o jsonpath='{.spec.replicas}' 2>/dev/null || echo -1)"

    if [[ "\${ready}" == "\${desired}" && "\${desired}" -gt 0 ]]; then
      info "All \${desired} replicas ready after \${elapsed}s."
      return 0
    fi

    info "Ready: \${ready}/\${desired} — waiting \${HEALTH_CHECK_INTERVAL}s..."
    sleep "\${HEALTH_CHECK_INTERVAL}"
    (( elapsed += HEALTH_CHECK_INTERVAL ))
  done

  die "Rollout timed out after \${MAX_WAIT_SECONDS}s." 4
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
  parse_args "$@"
  [[ "\${VERBOSE}" == "true" ]] && set -x

  info "Starting deployment: env=\${ENVIRONMENT} image=\${IMAGE_TAG}"

  check_tools
  check_cluster_access
  resolve_namespace
  ecr_login
  helm_deploy
  [[ "\${DRY_RUN}" == "false" ]] && wait_for_rollout

  info "Deployment complete."
}

main "$@"
`;

// ---------------------------------------------------------------------------
// SCSS (~270 lines) — design system with variables, mixins, nesting, comments
// ---------------------------------------------------------------------------
export const SCSS_LARGE = `
// =============================================================================
// Design System — Core Styles
// Version: 3.1.0
// Follows the BEM naming convention (Block__Element--Modifier).
// =============================================================================

// ---------------------------------------------------------------------------
// Variables — tokens
// ---------------------------------------------------------------------------

// Color palette
$color-primary-50:  #eff6ff;
$color-primary-100: #dbeafe;
$color-primary-500: #3b82f6;  // Base brand blue
$color-primary-600: #2563eb;  // Hover / active state
$color-primary-700: #1d4ed8;  // Pressed state
$color-primary-900: #1e3a8a;

$color-neutral-0:   #ffffff;
$color-neutral-50:  #f8fafc;
$color-neutral-100: #f1f5f9;
$color-neutral-200: #e2e8f0;
$color-neutral-500: #64748b;
$color-neutral-700: #334155;
$color-neutral-900: #0f172a;

$color-success:  #16a34a;
$color-warning:  #d97706;
$color-error:    #dc2626;
$color-info:     $color-primary-500;

// Typography scale
$font-family-base: 'Inter', system-ui, -apple-system, sans-serif;
$font-family-mono: 'JetBrains Mono', 'Fira Code', monospace;

$font-size-xs:   0.75rem;   //  12px
$font-size-sm:   0.875rem;  //  14px
$font-size-base: 1rem;      //  16px (root)
$font-size-lg:   1.125rem;  //  18px
$font-size-xl:   1.25rem;   //  20px
$font-size-2xl:  1.5rem;    //  24px
$font-size-3xl:  1.875rem;  //  30px
$font-size-4xl:  2.25rem;   //  36px

$font-weight-normal:   400;
$font-weight-medium:   500;
$font-weight-semibold: 600;
$font-weight-bold:     700;

$line-height-tight:  1.25;
$line-height-snug:   1.375;
$line-height-normal: 1.5;
$line-height-relaxed: 1.625;

// Spacing scale (8px grid)
$space-1: 0.25rem;   //  4px
$space-2: 0.5rem;    //  8px
$space-3: 0.75rem;   // 12px
$space-4: 1rem;      // 16px
$space-6: 1.5rem;    // 24px
$space-8: 2rem;      // 32px
$space-12: 3rem;     // 48px
$space-16: 4rem;     // 64px

// Border radii
$radius-sm:   0.125rem;
$radius-base: 0.25rem;
$radius-md:   0.375rem;
$radius-lg:   0.5rem;
$radius-xl:   0.75rem;
$radius-full: 9999px;

// Shadows
$shadow-sm: 0 1px 2px 0 rgb(0 0 0 / 0.05);
$shadow-base: 0 1px 3px 0 rgb(0 0 0 / 0.1), 0 1px 2px -1px rgb(0 0 0 / 0.1);
$shadow-lg:   0 10px 15px -3px rgb(0 0 0 / 0.1), 0 4px 6px -4px rgb(0 0 0 / 0.1);

// Z-index scale
$z-dropdown:  1000;
$z-sticky:    1020;
$z-fixed:     1030;
$z-modal-bg:  1040;
$z-modal:     1050;
$z-popover:   1060;
$z-tooltip:   1070;

// Breakpoints
$breakpoints: (
  'sm':  640px,
  'md':  768px,
  'lg':  1024px,
  'xl':  1280px,
  '2xl': 1536px,
);

// Transitions
$transition-base: all 150ms cubic-bezier(0.4, 0, 0.2, 1);
$transition-colors: color, background-color, border-color, outline-color,
  fill, stroke 150ms cubic-bezier(0.4, 0, 0.2, 1);

// ---------------------------------------------------------------------------
// Mixins
// ---------------------------------------------------------------------------

/// Respond-to breakpoint mixin.
/// Usage: @include respond-to('md') { ... }
@mixin respond-to($bp) {
  @if map-has-key($breakpoints, $bp) {
    @media (min-width: map-get($breakpoints, $bp)) {
      @content;
    }
  } @else {
    @warn "Unknown breakpoint: #{$bp}";
  }
}

/// Truncate text to a single line with an ellipsis.
@mixin truncate {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/// Focus-visible ring (accessible keyboard focus indicator).
@mixin focus-ring($color: $color-primary-500) {
  outline: 2px solid transparent;
  outline-offset: 2px;

  &:focus-visible {
    outline-color: $color;
    box-shadow: 0 0 0 3px rgb(59 130 246 / 0.5);
  }
}

/// Visually hidden (accessible off-screen element).
@mixin sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

// ---------------------------------------------------------------------------
// Reset / base
// ---------------------------------------------------------------------------

*,
*::before,
*::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

html {
  font-size: 100%; // 16px base; do not change — affects rem scale
  scroll-behavior: smooth;
  text-size-adjust: 100%;
}

body {
  font-family: $font-family-base;
  font-size: $font-size-base;
  line-height: $line-height-normal;
  color: $color-neutral-900;
  background-color: $color-neutral-0;
  -webkit-font-smoothing: antialiased;
}

// ---------------------------------------------------------------------------
// Button component
// ---------------------------------------------------------------------------

.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: $space-2;
  padding: $space-2 $space-4;
  font-size: $font-size-sm;
  font-weight: $font-weight-medium;
  line-height: $line-height-tight;
  border-radius: $radius-md;
  border: 1px solid transparent;
  cursor: pointer;
  transition: $transition-base;
  white-space: nowrap;
  user-select: none;
  @include focus-ring;

  &:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    pointer-events: none;
  }

  // Size modifiers
  &--sm {
    padding: $space-1 $space-3;
    font-size: $font-size-xs;
  }

  &--lg {
    padding: $space-3 $space-6;
    font-size: $font-size-lg;
  }

  // Variant: primary (solid)
  &--primary {
    background-color: $color-primary-500;
    color: $color-neutral-0;
    border-color: $color-primary-500;

    &:hover:not(:disabled) {
      background-color: $color-primary-600;
      border-color: $color-primary-600;
    }

    &:active:not(:disabled) {
      background-color: $color-primary-700;
    }
  }

  // Variant: outline
  &--outline {
    background-color: transparent;
    color: $color-primary-600;
    border-color: $color-primary-200;

    &:hover:not(:disabled) {
      background-color: $color-primary-50;
    }
  }

  // Variant: ghost (no border)
  &--ghost {
    background-color: transparent;
    color: $color-neutral-700;

    &:hover:not(:disabled) {
      background-color: $color-neutral-100;
    }
  }
}

// ---------------------------------------------------------------------------
// Card component
// ---------------------------------------------------------------------------

.card {
  background-color: $color-neutral-0;
  border: 1px solid $color-neutral-200;
  border-radius: $radius-lg;
  box-shadow: $shadow-sm;

  &__header {
    padding: $space-4 $space-6;
    border-bottom: 1px solid $color-neutral-200;
    font-weight: $font-weight-semibold;
  }

  &__body {
    padding: $space-6;
  }

  &__footer {
    padding: $space-4 $space-6;
    border-top: 1px solid $color-neutral-200;
    display: flex;
    align-items: center;
    gap: $space-3;
  }

  // Interactive card variant
  &--interactive {
    cursor: pointer;
    transition: box-shadow 150ms ease, transform 150ms ease;

    &:hover {
      box-shadow: $shadow-lg;
      transform: translateY(-1px);
    }
  }
}
`;

// ---------------------------------------------------------------------------
// XML (~240 lines) — Spring XML application context with comments
// ---------------------------------------------------------------------------
export const XML_LARGE = `<?xml version="1.0" encoding="UTF-8"?>
<!--
  applicationContext.xml — Spring Framework application context
  Defines beans, data sources, transaction managers, and security config.
  Generated by: spring-config-generator v2.1
  Last modified: 2026-06-01
  WARNING: Do not edit the generated sections (marked GENERATED).
-->
<beans xmlns="http://www.springframework.org/schema/beans"
       xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
       xmlns:context="http://www.springframework.org/schema/context"
       xmlns:tx="http://www.springframework.org/schema/tx"
       xmlns:security="http://www.springframework.org/schema/security"
       xsi:schemaLocation="
         http://www.springframework.org/schema/beans
           https://www.springframework.org/schema/beans/spring-beans.xsd
         http://www.springframework.org/schema/context
           https://www.springframework.org/schema/context/spring-context.xsd
         http://www.springframework.org/schema/tx
           https://www.springframework.org/schema/tx/spring-tx.xsd
         http://www.springframework.org/schema/security
           https://www.springframework.org/schema/security/spring-security.xsd">

  <!-- ===================================================================
       Component scanning — discovers @Component, @Service, @Repository
       @Controller annotations in the specified base packages.
       =================================================================== -->
  <context:component-scan base-package="com.example.api,com.example.service"/>

  <!-- Property placeholder: loads application.properties from classpath -->
  <context:property-placeholder location="classpath:application.properties"
                                  ignore-unresolvable="true"/>

  <!-- ===================================================================
       Data Source — HikariCP connection pool
       Tune pool sizes per environment via application.properties.
       =================================================================== -->
  <bean id="dataSource" class="com.zaxxer.hikari.HikariDataSource" destroy-method="close">
    <!-- JDBC URL; replaced by environment variable in Docker deployments -->
    <property name="jdbcUrl" value="\${db.url}"/>
    <property name="username" value="\${db.username}"/>
    <property name="password" value="\${db.password}"/>
    <property name="driverClassName" value="org.postgresql.Driver"/>
    <!-- Connection pool settings -->
    <property name="minimumIdle"         value="\${db.pool.minIdle:5}"/>
    <property name="maximumPoolSize"     value="\${db.pool.maxSize:20}"/>
    <property name="connectionTimeout"   value="\${db.pool.connectionTimeout:30000}"/>
    <property name="idleTimeout"         value="\${db.pool.idleTimeout:600000}"/>
    <property name="maxLifetime"         value="\${db.pool.maxLifetime:1800000}"/>
    <!-- Enable prepared-statement cache (PostgreSQL-specific optimisation) -->
    <property name="dataSourceProperties">
      <props>
        <prop key="cachePrepStmts">true</prop>
        <prop key="prepStmtCacheSize">250</prop>
        <prop key="prepStmtCacheSqlLimit">2048</prop>
      </props>
    </property>
  </bean>

  <!-- ===================================================================
       JPA / Hibernate EntityManagerFactory
       =================================================================== -->
  <bean id="entityManagerFactory"
        class="org.springframework.orm.jpa.LocalContainerEntityManagerFactoryBean">
    <property name="dataSource" ref="dataSource"/>
    <property name="packagesToScan" value="com.example.domain"/>
    <property name="jpaVendorAdapter">
      <bean class="org.springframework.orm.jpa.vendor.HibernateJpaVendorAdapter">
        <!-- Auto-detect dialect from DataSource driver -->
        <property name="generateDdl"    value="false"/>
        <property name="showSql"        value="\${jpa.showSql:false}"/>
      </bean>
    </property>
    <property name="jpaProperties">
      <props>
        <!-- Validate schema against entities on startup (fail-fast) -->
        <prop key="hibernate.hbm2ddl.auto">validate</prop>
        <!-- Second-level cache — Ehcache 3 provider -->
        <prop key="hibernate.cache.use_second_level_cache">true</prop>
        <prop key="hibernate.cache.region.factory_class">
          org.hibernate.cache.jcache.JCacheRegionFactory
        </prop>
        <!-- Batch inserts for performance -->
        <prop key="hibernate.jdbc.batch_size">50</prop>
        <prop key="hibernate.order_inserts">true</prop>
        <prop key="hibernate.order_updates">true</prop>
      </props>
    </property>
  </bean>

  <!-- ===================================================================
       Transaction management
       @Transactional annotations are processed by the tx: namespace.
       =================================================================== -->
  <bean id="transactionManager"
        class="org.springframework.orm.jpa.JpaTransactionManager">
    <property name="entityManagerFactory" ref="entityManagerFactory"/>
  </bean>

  <!-- Enable annotation-driven transaction management -->
  <tx:annotation-driven transaction-manager="transactionManager" proxy-target-class="true"/>

  <!-- ===================================================================
       Caching — Spring Cache abstraction backed by Caffeine
       =================================================================== -->
  <bean id="cacheManager" class="org.springframework.cache.caffeine.CaffeineCacheManager">
    <!-- Comma-separated list of cache regions to pre-create -->
    <property name="cacheNames" value="users,orders,products,roles"/>
    <property name="caffeine" value="maximumSize=10000,expireAfterWrite=300s"/>
  </bean>

  <!-- ===================================================================
       Email / async execution
       =================================================================== -->
  <!-- Thread pool for @Async methods — separate from web thread pool -->
  <bean id="taskExecutor"
        class="org.springframework.scheduling.concurrent.ThreadPoolTaskExecutor">
    <property name="corePoolSize"     value="4"/>
    <property name="maxPoolSize"      value="16"/>
    <property name="queueCapacity"    value="500"/>
    <property name="threadNamePrefix" value="async-"/>
    <!-- Await task completion on shutdown (graceful drain) -->
    <property name="waitForTasksToCompleteOnShutdown" value="true"/>
    <property name="awaitTerminationSeconds"          value="30"/>
  </bean>

  <!-- JavaMailSender — configured via environment-specific properties -->
  <bean id="mailSender" class="org.springframework.mail.javamail.JavaMailSenderImpl">
    <property name="host"     value="\${mail.host:smtp.example.com}"/>
    <property name="port"     value="\${mail.port:587}"/>
    <property name="username" value="\${mail.username}"/>
    <property name="password" value="\${mail.password}"/>
    <property name="javaMailProperties">
      <props>
        <prop key="mail.smtp.auth">true</prop>
        <prop key="mail.smtp.starttls.enable">true</prop>
        <prop key="mail.smtp.connectiontimeout">5000</prop>
      </props>
    </property>
  </bean>

</beans>`;

export const JS_LARGE = `
/**
 * @fileoverview Utility functions for collections, strings, and async ops.
 * Adapted from lodash/underscore patterns — real JSDoc comment density.
 * @module utils
 * @version 3.1.4
 */

'use strict';

// ─── Internal helpers ────────────────────────────────────────────────────────

/** @type {Object} Tag constants for type detection. */
const TAG = {
  Array:    '[object Array]',
  Boolean:  '[object Boolean]',
  Date:     '[object Date]',
  Function: '[object Function]',
  Null:     '[object Null]',
  Number:   '[object Number]',
  Object:   '[object Object]',
  RegExp:   '[object RegExp]',
  String:   '[object String]',
  Symbol:   '[object Symbol]',
  Undefined:'[object Undefined]',
};

const { toString } = Object.prototype;
const hasOwn      = Object.prototype.hasOwnProperty;

/**
 * Return the [[Class]] tag of any value.
 * @param {*} val
 * @returns {string}
 */
function typeTag(val) {
  return toString.call(val);
}

// ─── Type guards ─────────────────────────────────────────────────────────────

/**
 * Check whether a value is an Array.
 * @param {*} val
 * @returns {boolean}
 */
function isArray(val) {
  return Array.isArray(val);
}

/**
 * Check whether a value is a plain object (not an array, Date, RegExp, etc.).
 * @param {*} val
 * @returns {boolean}
 */
function isPlainObject(val) {
  if (typeTag(val) !== TAG.Object) return false;
  const proto = Object.getPrototypeOf(val);
  return proto === null || proto === Object.prototype;
}

/**
 * Check whether a value is a non-null object.
 * @param {*} val
 * @returns {boolean}
 */
function isObject(val) {
  return val !== null && typeof val === 'object';
}

/**
 * Check whether a value is a function.
 * @param {*} val
 * @returns {boolean}
 */
function isFunction(val) {
  return typeof val === 'function';
}

/**
 * Check whether a value is a string.
 * @param {*} val
 * @returns {boolean}
 */
function isString(val) {
  return typeof val === 'string';
}

/**
 * Check whether a value is a finite number.
 * @param {*} val
 * @returns {boolean}
 */
function isNumber(val) {
  return typeof val === 'number' && isFinite(val);
}

/**
 * Check whether a value is null or undefined.
 * @param {*} val
 * @returns {boolean}
 */
function isNil(val) {
  return val == null;
}

// ─── Object utilities ────────────────────────────────────────────────────────

/**
 * Recursively merge source objects into target.
 *
 * Rules:
 * - Plain objects are merged recursively.
 * - Arrays are replaced (not concatenated).
 * - Primitives from source always overwrite target.
 *
 * @param {Object} target - The object to mutate.
 * @param {...Object} sources - One or more source objects.
 * @returns {Object} The mutated target.
 *
 * @example
 * merge({ a: 1, b: { c: 2 } }, { b: { d: 3 }, e: 4 });
 * // => { a: 1, b: { c: 2, d: 3 }, e: 4 }
 */
function merge(target, ...sources) {
  for (const source of sources) {
    if (!isObject(source)) continue;
    for (const key of Object.keys(source)) {
      const srcVal = source[key];
      if (isPlainObject(srcVal) && isPlainObject(target[key])) {
        target[key] = merge({}, target[key], srcVal);
      } else {
        target[key] = srcVal;
      }
    }
  }
  return target;
}

/**
 * Create a deep clone of a plain value (object/array/primitive).
 * Functions and non-serialisable values are returned as-is.
 *
 * @param {*} val
 * @returns {*}
 */
function cloneDeep(val) {
  if (isArray(val)) return val.map(cloneDeep);
  if (isPlainObject(val)) {
    const clone = {};
    for (const key of Object.keys(val)) {
      clone[key] = cloneDeep(val[key]);
    }
    return clone;
  }
  return val;
}

/**
 * Pick specified keys from an object.
 *
 * @param {Object} obj - Source object.
 * @param {string[]} keys - Keys to keep.
 * @returns {Object}
 */
function pick(obj, keys) {
  const result = {};
  for (const key of keys) {
    if (hasOwn.call(obj, key)) result[key] = obj[key];
  }
  return result;
}

/**
 * Omit specified keys from an object.
 *
 * @param {Object} obj - Source object.
 * @param {string[]} keys - Keys to exclude.
 * @returns {Object}
 */
function omit(obj, keys) {
  const set = new Set(keys);
  const result = {};
  for (const key of Object.keys(obj)) {
    if (!set.has(key)) result[key] = obj[key];
  }
  return result;
}

/**
 * Recursively flatten a nested object into dot-notation keys.
 *
 * @param {Object} obj
 * @param {string} [prefix='']
 * @returns {Object}
 *
 * @example
 * flattenObject({ a: { b: { c: 1 } } });
 * // => { 'a.b.c': 1 }
 */
function flattenObject(obj, prefix = '') {
  const result = {};
  for (const [key, val] of Object.entries(obj)) {
    const fullKey = prefix ? \`\${prefix}.\${key}\` : key;
    if (isPlainObject(val) && Object.keys(val).length > 0) {
      Object.assign(result, flattenObject(val, fullKey));
    } else {
      result[fullKey] = val;
    }
  }
  return result;
}

// ─── Array utilities ─────────────────────────────────────────────────────────

/**
 * Remove duplicate values from an array (using SameValueZero equality).
 * @param {Array} arr
 * @returns {Array}
 */
function unique(arr) {
  return [...new Set(arr)];
}

/**
 * Group array items by a key selector function.
 *
 * @template T
 * @param {T[]} arr
 * @param {function(T): string} keyFn
 * @returns {Object.<string, T[]>}
 */
function groupBy(arr, keyFn) {
  return arr.reduce((groups, item) => {
    const key = String(keyFn(item));
    if (!groups[key]) groups[key] = [];
    groups[key].push(item);
    return groups;
  }, {});
}

/**
 * Chunk an array into groups of the given size.
 *
 * @param {Array} arr
 * @param {number} size - Chunk size (must be > 0).
 * @returns {Array[]}
 *
 * @throws {RangeError} If size <= 0.
 */
function chunk(arr, size) {
  if (size <= 0) throw new RangeError('chunk size must be > 0');
  const result = [];
  for (let i = 0; i < arr.length; i += size) {
    result.push(arr.slice(i, i + size));
  }
  return result;
}

/**
 * Flatten a nested array by one level.
 * @param {Array} arr
 * @returns {Array}
 */
function flatten(arr) {
  return arr.reduce((flat, item) => flat.concat(item), []);
}

/**
 * Return the intersection of two arrays.
 * @param {Array} a
 * @param {Array} b
 * @returns {Array}
 */
function intersection(a, b) {
  const setB = new Set(b);
  return a.filter(v => setB.has(v));
}

// ─── String utilities ─────────────────────────────────────────────────────────

/**
 * Convert a string to camelCase.
 *
 * @param {string} str
 * @returns {string}
 *
 * @example
 * camelCase('hello-world'); // => 'helloWorld'
 * camelCase('FOO_BAR');     // => 'fooBar'
 */
function camelCase(str) {
  return str
    .replace(/[-_\\s]+(.)?/g, (_, c) => (c ? c.toUpperCase() : ''))
    .replace(/^./, ch => ch.toLowerCase());
}

/**
 * Convert a string to PascalCase.
 * @param {string} str
 * @returns {string}
 */
function pascalCase(str) {
  const cc = camelCase(str);
  return cc.charAt(0).toUpperCase() + cc.slice(1);
}

/**
 * Convert a string to kebab-case.
 * @param {string} str
 * @returns {string}
 */
function kebabCase(str) {
  return str
    .replace(/([A-Z])/g, m => '-' + m.toLowerCase())
    .replace(/[\\s_]+/g, '-')
    .replace(/^-/, '');
}

/**
 * Truncate a string to maxLength, appending suffix if truncated.
 *
 * @param {string}  str
 * @param {number}  maxLength
 * @param {string} [suffix='…']
 * @returns {string}
 */
function truncate(str, maxLength, suffix = '…') {
  if (str.length <= maxLength) return str;
  return str.slice(0, maxLength - suffix.length) + suffix;
}

/**
 * Escape HTML special characters in a string.
 * @param {string} str
 * @returns {string}
 */
function escapeHtml(str) {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

// ─── Async utilities ──────────────────────────────────────────────────────────

/**
 * Return a Promise that resolves after \`ms\` milliseconds.
 * @param {number} ms
 * @returns {Promise<void>}
 */
function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Retry an async function up to \`maxAttempts\` times with exponential back-off.
 *
 * @param {function(): Promise<*>} fn     - Async function to retry.
 * @param {number}                maxAttempts
 * @param {number}               [baseMs=100] - Initial delay in ms.
 * @returns {Promise<*>}
 * @throws The last error if all attempts fail.
 */
async function retry(fn, maxAttempts, baseMs = 100) {
  let lastErr;
  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    try {
      return await fn();
    } catch (err) {
      lastErr = err;
      if (attempt < maxAttempts - 1) {
        await sleep(baseMs * 2 ** attempt);
      }
    }
  }
  throw lastErr;
}

/**
 * Run async tasks with a concurrency limit.
 *
 * @template T
 * @param {Array<function(): Promise<T>>} tasks - Zero-argument async factories.
 * @param {number} limit - Maximum tasks to run simultaneously.
 * @returns {Promise<T[]>} Results in input order.
 */
async function pLimit(tasks, limit) {
  const results = new Array(tasks.length);
  let idx = 0;

  async function worker() {
    while (idx < tasks.length) {
      const current = idx++;
      results[current] = await tasks[current]();
    }
  }

  await Promise.all(Array.from({ length: Math.min(limit, tasks.length) }, worker));
  return results;
}

// ─── Exports ─────────────────────────────────────────────────────────────────

module.exports = {
  // Type guards
  isArray, isPlainObject, isObject, isFunction, isString, isNumber, isNil,
  // Object
  merge, cloneDeep, pick, omit, flattenObject,
  // Array
  unique, groupBy, chunk, flatten, intersection,
  // String
  camelCase, pascalCase, kebabCase, truncate, escapeHtml,
  // Async
  sleep, retry, pLimit,
};
`;

function numberedBlocks(
  count: number,
  render: (index: number) => string
): string {
  return Array.from({ length: count }, (_, index) => render(index)).join('\n');
}

// ---------------------------------------------------------------------------
// C (~300 lines) — option parser with URLs, block comments, and string markers
// ---------------------------------------------------------------------------
const C_GENERATED_OPTIONS = numberedBlocks(
  48,
  index => `
/* Generated option block ${index}: stripped by c-style comments. */
static int parse_flag_${index}(const char *value, struct ParserState *state) {
    const char *help = "https://example.com/docs/flag-${index}//not-a-comment";
    const char *marker = "C_KEEP_MARKER_${index} /* not a comment */";
    if (value == NULL || value[0] == '\\0') {
        state->errors++;
        return -1;
    }
    if (strcmp(value, "--enable-${index}") == 0) {
        state->enabled[${index % 8}] = true; // stripped inline comment
        state->last_message = marker;
        state->last_help = help;
        return 0;
    }
    return 1;
}`
);

export const C_LARGE = `
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* C fixture adapted from real command-line parser shapes. */
struct ParserState {
    bool enabled[8];
    int errors;
    const char *last_message;
    const char *last_help;
};

static void reset_state(struct ParserState *state) {
    memset(state->enabled, 0, sizeof(state->enabled));
    state->errors = 0;
    state->last_message = "C_KEEP_MARKER_BOOT";
    state->last_help = "http://localhost:8080/help//keep";
}

${C_GENERATED_OPTIONS}

int main(int argc, char **argv) {
    struct ParserState state;
    reset_state(&state);
    for (int i = 1; i < argc; i++) {
        int parsed = parse_flag_0(argv[i], &state);
        if (parsed < 0) {
            fprintf(stderr, "bad flag: %s\\n", argv[i]);
            return 2;
        }
    }
    puts(state.last_message);
    return state.errors == 0 ? 0 : 1;
}
`;

// ---------------------------------------------------------------------------
// C++ (~300 lines) — stream formatter with raw strings and templates
// ---------------------------------------------------------------------------
const CPP_GENERATED_FORMATTERS = numberedBlocks(
  42,
  index => `
template <typename Writer>
void writeSection${index}(Writer& writer, const std::vector<std::string>& rows) {
  // stripped C++ line comment ${index}
  const std::string url = "https://example.com/cpp/${index}//keep";
  const std::string raw = R"(CPP_KEEP_MARKER_${index} /* keep */ // keep)";
  writer.begin("section-${index}");
  for (const auto& row : rows) {
    writer.field(url, row.empty() ? raw : row);
  }
  writer.end();
}`
);

export const CPP_LARGE = `
#include <algorithm>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

namespace benchmark {

class ReportWriter {
 public:
  explicit ReportWriter(std::ostream& out) : out_(out) {}

  void begin(const std::string& name) {
    out_ << "begin:" << name << '\\n';
  }

  void field(const std::string& key, const std::string& value) {
    out_ << key << '=' << value << '\\n';
  }

  void end() {
    out_ << "end" << '\\n';
  }

 private:
  std::ostream& out_;
};

${CPP_GENERATED_FORMATTERS}

int run(std::ostream& out) {
  ReportWriter writer(out);
  std::vector<std::string> rows = {"alpha", "beta", "CPP_KEEP_MARKER_FINAL"};
  writeSection0(writer, rows);
  writeSection17(writer, rows);
  writeSection41(writer, rows);
  return static_cast<int>(rows.size());
}

}  // namespace benchmark
`;

// ---------------------------------------------------------------------------
// Visual Basic (~260 lines) — module with apostrophe comments and strings
// ---------------------------------------------------------------------------
const VB_GENERATED_FORMATTERS = numberedBlocks(
  52,
  index => `
' Stripped Visual Basic comment ${index}
Public Function FormatRow${index}(ByVal userName As String, ByVal amount As Decimal) As String
    Dim marker As String = "' VB_KEEP_MARKER_${index}"
    Dim url As String = "https://example.com/vb/${index}'keep"
    If String.IsNullOrWhiteSpace(userName) Then
        Return marker & ":" & url
    End If
    Return userName & ":" & amount.ToString("0.00") & ":" & marker
End Function`
);

export const VB_LARGE = `
Option Strict On
Option Explicit On

' Visual Basic fixture adapted from reporting modules.
Public Module CustomerReport
    Private Const Header As String = "' VB_KEEP_MARKER_HEADER"

${VB_GENERATED_FORMATTERS}

    Public Function BuildReport(ByVal rows As IEnumerable(Of String)) As String
        Dim output As New System.Text.StringBuilder()
        output.AppendLine(Header)
        For Each row In rows
            output.AppendLine(row)
        Next
        Return output.ToString()
    End Function
End Module
`;

// ---------------------------------------------------------------------------
// JSX (~260 lines) — component tree with comments, URLs, and string literals
// ---------------------------------------------------------------------------
const JSX_GENERATED_ROWS = numberedBlocks(
  50,
  index => `
      {/* stripped JSX comment ${index} */}
      <li key="row-${index}" data-url="https://example.com/jsx/${index}//keep">
        <strong>{formatLabel('JSX_KEEP_MARKER_${index}')}</strong>
        <span>{items[${index % 4}] ?? 'fallback'}</span>
      </li>`
);

export const JSX_LARGE = `
import React, { useMemo, useState } from 'react';

const formatLabel = value => value.replace(/_/g, ' ');

export default function DashboardPanel({ items = [] }) {
  const [filter, setFilter] = useState('');
  const visible = useMemo(
    () => items.filter(item => item.includes(filter)),
    [items, filter]
  );

  return (
    <section className="dashboard-panel">
      <header>
        <h1>JSX_KEEP_MARKER_TITLE</h1>
        <input
          aria-label="Filter"
          value={filter}
          onChange={event => setFilter(event.target.value)}
          placeholder="https://example.com/search//keep"
        />
      </header>
      <ul>
${JSX_GENERATED_ROWS}
      </ul>
      <footer>{visible.length} visible rows</footer>
    </section>
  );
}
`;

// ---------------------------------------------------------------------------
// TSX (~300 lines) — app router component with type-only declarations
// ---------------------------------------------------------------------------
const TSX_GENERATED_ROUTES = numberedBlocks(
  54,
  index => `
  {
    id: 'route-${index}',
    title: 'TSX_KEEP_MARKER_${index}',
    href: 'https://example.com/tsx/${index}//keep',
    loader: async () => ({ status: 'ready' as const, index: ${index} }),
  },`
);

export const TSX_LARGE = `
import React, { Suspense, useMemo } from 'react';

type RouteStatus = 'ready' | 'loading' | 'error';

type RouteRecord = {
  readonly id: string;
  readonly title: string;
  readonly href: string;
  readonly loader: () => Promise<{ readonly status: RouteStatus; readonly index: number }>;
};

const routes: readonly RouteRecord[] = [
${TSX_GENERATED_ROUTES}
];

function RouteLink({ route }: { readonly route: RouteRecord }) {
  return (
    <a href={route.href} data-route-id={route.id}>
      <span>{route.title}</span>
    </a>
  );
}

export function AppRouter({ activeId }: { readonly activeId: string }) {
  const active = useMemo(
    () => routes.find(route => route.id === activeId) ?? routes[0],
    [activeId]
  );

  return (
    <Suspense fallback={<p>Loading TSX_KEEP_MARKER_FALLBACK</p>}>
      <nav aria-label="Application routes">
        {routes.map(route => (
          <RouteLink key={route.id} route={route} />
        ))}
      </nav>
      <main>
        <h1>{active.title}</h1>
        <p>{active.href}</p>
      </main>
    </Suspense>
  );
}
`;
