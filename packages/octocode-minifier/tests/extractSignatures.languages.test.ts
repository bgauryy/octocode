import { describe, it, expect } from 'vitest';
import {
  extractSignatures,
  SUPPORTED_SIGNATURE_EXTENSIONS,
} from '@octocodeai/octocode-minifier';

/**
 * Per-language skeleton extraction contract. For EVERY language family the
 * fixture asserts:
 *   (a) imports/dependencies kept
 *   (b) function/method names + signatures kept
 *   (c) class/struct/interface/trait/protocol names kept
 *   (d) exported/public symbols kept
 *   (e) bodies dropped
 *   (f) ZERO comment lines in the output
 *   (g) original line numbers preserved in the gutter
 *   (h) no blank gutter lines
 */

interface GutterLine {
  num: number;
  text: string;
}

describe('signature extension registry', () => {
  it('exports the full supported extension list used by resource descriptions', () => {
    expect(SUPPORTED_SIGNATURE_EXTENSIONS).toEqual(
      [...SUPPORTED_SIGNATURE_EXTENSIONS].sort()
    );
    expect(SUPPORTED_SIGNATURE_EXTENSIONS).toEqual(
      expect.arrayContaining(['hpp', 'cc', 'htm', 'zsh', 'rust', 'kotlin'])
    );
  });
});

function gutterLines(sigs: string): GutterLine[] {
  return sigs.split('\n').map(line => {
    const m = line.match(/^ *(\d+)\| (.*)$/);
    expect(m, `malformed gutter line: ${JSON.stringify(line)}`).not.toBeNull();
    return { num: Number(m![1]), text: m![2]! };
  });
}

/** (g) the gutter entry for 1-based line `num` exists and carries `substring`. */
function expectGutter(sigs: string, num: number, substring: string): void {
  const entry = gutterLines(sigs).find(l => l.num === num);
  expect(entry, `expected a gutter entry for line ${num}`).toBeDefined();
  expect(entry!.text).toContain(substring);
}

/** (h) every output line is `NNN| <non-blank content>`. */
function expectNoBlankGutterLines(sigs: string): void {
  for (const line of sigs.split('\n')) {
    expect(line).toMatch(/^ *\d+\| .*\S/);
  }
}

/** (f) zero pure-comment lines, per the family's comment syntax. */
function expectNoCommentLines(
  sigs: string,
  style: 'c' | 'hash' | 'php' | 'html' | 'sql'
): void {
  for (const { text } of gutterLines(sigs)) {
    const trimmed = text.trim();
    if (style === 'c' || style === 'php' || style === 'html') {
      expect(trimmed.startsWith('//'), `comment line leaked: ${text}`).toBe(
        false
      );
      expect(trimmed.startsWith('/*'), `comment line leaked: ${text}`).toBe(
        false
      );
      expect(trimmed.startsWith('*'), `comment line leaked: ${text}`).toBe(
        false
      );
    }
    if (style === 'hash' || style === 'php') {
      // '#include'/'#define' are C preprocessor directives and '#!' is a
      // shebang, not comments — the hash check only applies to hash-comment
      // families (py/rb/php/sh).
      if (trimmed.startsWith('#!')) continue;
      expect(trimmed.startsWith('#'), `comment line leaked: ${text}`).toBe(
        false
      );
    }
    if (style === 'html') {
      expect(trimmed.startsWith('<!--'), `comment line leaked: ${text}`).toBe(
        false
      );
      expect(trimmed.startsWith('-->'), `comment line leaked: ${text}`).toBe(
        false
      );
    }
    if (style === 'sql') {
      expect(trimmed.startsWith('--'), `comment line leaked: ${text}`).toBe(
        false
      );
      expect(trimmed.startsWith('/*'), `comment line leaked: ${text}`).toBe(
        false
      );
    }
  }
}

// ───────────────────────────── TypeScript (AST) ─────────────────────────────

const TS_CLASS = `import { Base } from './base';

/**
 * JSDoc that must vanish.
 */
export class Service extends Base {
  private readonly cache = new Map<string, number>();
  static DEFAULT_TIMEOUT = 5_000;

  constructor(
    private name: string,
    options?: { verbose?: boolean },
  ) {
    super();
    this.bootstrap();
  }

  // line comment that must vanish
  @log()
  async run<T>(
    input: T,
    retries = 3,
  ): Promise<T | null> {
    const internalSecret = compute(input);
    return internalSecret;
  }

  get size(): number {
    return this.cache.size;
  }

  private bootstrap(): void {
    this.cache.clear();
  }
}
`;

describe('extractSignatures — ts (AST)', () => {
  const sigs = extractSignatures(TS_CLASS, 'service.ts')!;

  it('keeps imports, class head, member signatures with original line numbers', () => {
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 1, "import { Base } from './base';");
    expectGutter(sigs, 6, 'export class Service extends Base {');
    expectGutter(
      sigs,
      7,
      'private readonly cache = new Map<string, number>();'
    );
    expectGutter(sigs, 8, 'static DEFAULT_TIMEOUT = 5_000;');
    expectGutter(sigs, 10, 'constructor(');
    expectGutter(sigs, 12, 'options?: { verbose?: boolean },');
    expectGutter(sigs, 19, '@log()');
    expectGutter(sigs, 20, 'async run<T>(');
    expectGutter(sigs, 23, '): Promise<T | null> {');
    expectGutter(sigs, 28, 'get size(): number {');
    expectGutter(sigs, 32, 'private bootstrap(): void {');
  });

  it('drops constructor/method/getter bodies', () => {
    expect(sigs).not.toContain('super();');
    expect(sigs).not.toContain('this.bootstrap()');
    expect(sigs).not.toContain('internalSecret');
    expect(sigs).not.toContain('this.cache.size');
    expect(sigs).not.toContain('this.cache.clear()');
  });

  it('drops all comments, including JSDoc', () => {
    expect(sigs).not.toContain('JSDoc that must vanish');
    expect(sigs).not.toContain('line comment that must vanish');
    expectNoCommentLines(sigs, 'c');
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });

  it('is materially smaller than the source', () => {
    expect(sigs.length).toBeLessThan(TS_CLASS.length);
  });
});

const TS_CONSTS = `export const VERSION = '1.2.3';
export const FLAGS = ['a', 'b'];
const internalOnly = 42;
export const BIG = {
  alpha: 1,
  beta: 2,
};
export const handler = async (event: string): Promise<void> => {
  await dispatch(event);
};
`;

describe('extractSignatures — ts export const initializers (AST)', () => {
  const sigs = extractSignatures(TS_CONSTS, 'consts.ts')!;

  it('keeps exported small literals whole', () => {
    expectGutter(sigs, 1, "export const VERSION = '1.2.3';");
    expectGutter(sigs, 2, "export const FLAGS = ['a', 'b'];");
  });

  it('drops non-exported plain literal assignments', () => {
    expect(sigs).not.toContain('internalOnly');
  });

  it('keeps the head of a large object initializer and drops its body', () => {
    expectGutter(sigs, 4, 'export const BIG = {');
    expect(sigs).not.toContain('alpha: 1');
    expect(sigs).not.toContain('beta: 2');
  });

  it('keeps the arrow-function initializer head and drops its body', () => {
    expectGutter(
      sigs,
      8,
      'export const handler = async (event: string): Promise<void> => {'
    );
    expect(sigs).not.toContain('await dispatch');
  });

  it('emits no blank gutter lines and zero comment lines', () => {
    expectNoBlankGutterLines(sigs);
    expectNoCommentLines(sigs, 'c');
  });
});

const TS_OVERLOADS = `export function parse(input: string): Node;
export function parse(input: Buffer): Node;
export function parse(input: string | Buffer): Node {
  return inner(input);
}
`;

describe('extractSignatures — ts overloads (AST)', () => {
  const sigs = extractSignatures(TS_OVERLOADS, 'overloads.ts')!;

  it('keeps all overload signatures and the implementation head', () => {
    expectGutter(sigs, 1, 'export function parse(input: string): Node;');
    expectGutter(sigs, 2, 'export function parse(input: Buffer): Node;');
    expectGutter(
      sigs,
      3,
      'export function parse(input: string | Buffer): Node {'
    );
  });

  it('drops the implementation body', () => {
    expect(sigs).not.toContain('return inner(input);');
  });
});

// ───────────────────────────── TSX (AST) ─────────────────────────────

const TSX_COMPONENT = `import React from 'react';

export interface CardProps {
  title: string;
}

export function Card({ title }: CardProps): JSX.Element {
  const [open, setOpen] = React.useState(false);
  return (
    <div className="card" onClick={() => setOpen(!open)}>
      <h1>{title}</h1>
    </div>
  );
}

export const Badge: React.FC<CardProps> = ({ title }) => (
  <span className="badge">{title}</span>
);
`;

describe('extractSignatures — tsx (AST)', () => {
  const sigs = extractSignatures(TSX_COMPONENT, 'card.tsx')!;

  it('keeps imports, interface, and component signatures', () => {
    expectGutter(sigs, 1, "import React from 'react';");
    expectGutter(sigs, 3, 'export interface CardProps {');
    expectGutter(sigs, 4, 'title: string;');
    expectGutter(
      sigs,
      7,
      'export function Card({ title }: CardProps): JSX.Element {'
    );
    expectGutter(sigs, 16, 'export const Badge: React.FC<CardProps> =');
  });

  it('drops JSX bodies', () => {
    expect(sigs).not.toContain('<div');
    expect(sigs).not.toContain('<h1>');
    expect(sigs).not.toContain('<span');
    expect(sigs).not.toContain('useState');
  });

  it('emits no blank gutter lines and zero comment lines', () => {
    expectNoBlankGutterLines(sigs);
    expectNoCommentLines(sigs, 'c');
  });
});

// ───────────────────────────── JS / CJS (AST) ─────────────────────────────

const JS_CJS = `const fs = require('fs');
const { join } = require('path');

function readConfig(dir) {
  return JSON.parse(fs.readFileSync(join(dir, 'config.json')));
}

class Loader {
  load(name) {
    return readConfig(name);
  }
}

module.exports = { readConfig, Loader };
`;

describe('extractSignatures — js/cjs (AST)', () => {
  const sigs = extractSignatures(JS_CJS, 'loader.cjs')!;

  it('keeps require lines (dependencies)', () => {
    expectGutter(sigs, 1, "const fs = require('fs');");
    expectGutter(sigs, 2, "const { join } = require('path');");
  });

  it('keeps function and class/method signatures', () => {
    expectGutter(sigs, 4, 'function readConfig(dir) {');
    expectGutter(sigs, 8, 'class Loader {');
    expectGutter(sigs, 9, 'load(name) {');
  });

  it('keeps the module.exports surface (exported symbols)', () => {
    expectGutter(sigs, 14, 'module.exports = { readConfig, Loader };');
  });

  it('drops bodies', () => {
    expect(sigs).not.toContain('JSON.parse');
    expect(sigs).not.toContain('return readConfig(name);');
  });

  it('emits no blank gutter lines and zero comment lines', () => {
    expectNoBlankGutterLines(sigs);
    expectNoCommentLines(sigs, 'c');
  });
});

const JSX_COMPONENT = `import React from 'react';

export default function Panel({ title }) {
  const hidden = useHidden();
  return (
    <div className="panel">
      <h1>{title}</h1>
    </div>
  );
}
`;

describe('extractSignatures — jsx (AST)', () => {
  const sigs = extractSignatures(JSX_COMPONENT, 'Panel.jsx')!;

  it('keeps import and default component signature with gutter lines', () => {
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 1, "import React from 'react';");
    expectGutter(sigs, 3, 'export default function Panel({ title }) {');
  });

  it('drops JSX render bodies and local implementation details', () => {
    expect(sigs).not.toContain('useHidden');
    expect(sigs).not.toContain('hidden');
    expect(sigs).not.toContain('<div');
    expect(sigs).not.toContain('<h1>');
    expectNoCommentLines(sigs, 'c');
    expectNoBlankGutterLines(sigs);
  });
});

const MJS_MODULE = `import { readFile } from 'node:fs/promises';

export const version = '1.0.0';

export default async function loadConfig(path) {
  const text = await readFile(path, 'utf8');
  return JSON.parse(text);
}

export { loadConfig as load };
`;

describe('extractSignatures — mjs (AST)', () => {
  const sigs = extractSignatures(MJS_MODULE, 'config.mjs')!;

  it('keeps ESM imports, exports, and default function heads', () => {
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 1, "import { readFile } from 'node:fs/promises';");
    expectGutter(sigs, 3, "export const version = '1.0.0';");
    expectGutter(sigs, 5, 'export default async function loadConfig(path) {');
    expectGutter(sigs, 10, 'export { loadConfig as load };');
  });

  it('drops the default function body', () => {
    expect(sigs).not.toContain("readFile(path, 'utf8')");
    expect(sigs).not.toContain('JSON.parse');
    expectNoCommentLines(sigs, 'c');
    expectNoBlankGutterLines(sigs);
  });
});

// ───────────────────────────── Python ─────────────────────────────

const PY_SOURCE = `"""Module docstring should vanish."""
import os
from typing import Optional

# top comment should vanish
__all__ = ['Client']


def helper(value: int) -> int:
    # inner comment should vanish
    result = value * 2

    def nested(x):
        return x + 1

    return nested(result)


class Client:
    """Docstring should vanish."""

    @property
    def name(self) -> str:
        return self._name

    async def fetch(
        self,
        url: str,
    ) -> Optional[str]:
        async def inner():
            return url
        return await inner()
`;

describe('extractSignatures — python', () => {
  const sigs = extractSignatures(PY_SOURCE, 'client.py')!;

  it('keeps imports and module exports', () => {
    expectGutter(sigs, 2, 'import os');
    expectGutter(sigs, 3, 'from typing import Optional');
    expectGutter(sigs, 6, "__all__ = ['Client']");
  });

  it('keeps def/class/decorator signatures with multi-line def heads', () => {
    expectGutter(sigs, 9, 'def helper(value: int) -> int:');
    expectGutter(sigs, 19, 'class Client:');
    expectGutter(sigs, 22, '@property');
    expectGutter(sigs, 23, 'def name(self) -> str:');
    expectGutter(sigs, 26, 'async def fetch(');
    expectGutter(sigs, 28, 'url: str,');
    expectGutter(sigs, 29, ') -> Optional[str]:');
  });

  it('excludes nested functions and bodies', () => {
    expect(sigs).not.toContain('def nested');
    expect(sigs).not.toContain('async def inner');
    expect(sigs).not.toContain('result = value * 2');
    expect(sigs).not.toContain('self._name');
  });

  it('drops comments and docstrings', () => {
    expect(sigs).not.toContain('docstring');
    expect(sigs).not.toContain('Docstring');
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs, 'hash');
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });
});

// ───────────────────────────── Go ─────────────────────────────

const GO_SOURCE = `package httpclient

import (
	"fmt"
	"net/http"
)

const defaultTimeout = 30

// Client wraps HTTP access (comment should vanish)
type Client struct {
	BaseURL string
	inner   *http.Client
}

type Doer interface {
	Do(req *http.Request) (*http.Response, error)
}

func New(baseURL string) *Client {
	c := &Client{BaseURL: baseURL}
	return c
}

func (c *Client) Get(path string) (*http.Response, error) {
	url := fmt.Sprintf("%s%s", c.BaseURL, path)
	return c.inner.Get(url)
}
`;

describe('extractSignatures — go', () => {
  const sigs = extractSignatures(GO_SOURCE, 'client.go')!;

  it('keeps package and the whole import block', () => {
    expectGutter(sigs, 1, 'package httpclient');
    expectGutter(sigs, 3, 'import (');
    expectGutter(sigs, 4, '"fmt"');
    expectGutter(sigs, 5, '"net/http"');
  });

  it('keeps struct and interface bodies (field/method signatures)', () => {
    expectGutter(sigs, 11, 'type Client struct {');
    expectGutter(sigs, 12, 'BaseURL string');
    expectGutter(sigs, 16, 'type Doer interface {');
    expectGutter(sigs, 17, 'Do(req *http.Request) (*http.Response, error)');
  });

  it('keeps function and receiver-method signatures, top-level const', () => {
    expectGutter(sigs, 8, 'const defaultTimeout = 30');
    expectGutter(sigs, 20, 'func New(baseURL string) *Client {');
    expectGutter(
      sigs,
      25,
      'func (c *Client) Get(path string) (*http.Response, error) {'
    );
  });

  it('drops bodies and comments', () => {
    expect(sigs).not.toContain('c := &Client');
    expect(sigs).not.toContain('fmt.Sprintf');
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs, 'c');
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });
});

// ───────────────────────────── Java ─────────────────────────────

const JAVA_SOURCE = `package com.example;

import java.util.List;

/** Javadoc that must vanish */
public class OrderService {
    private final OrderRepo repo;

    public OrderService(OrderRepo repo) {
        this.repo = repo;
    }

    public List<String> findAll(String status) {
        return repo.byStatus(status);
    }
}

interface Service {
    void run();
}
`;

describe('extractSignatures — java', () => {
  const sigs = extractSignatures(JAVA_SOURCE, 'OrderService.java')!;

  it('keeps package, imports, class/interface and member signatures', () => {
    expectGutter(sigs, 1, 'package com.example;');
    expectGutter(sigs, 3, 'import java.util.List;');
    expectGutter(sigs, 6, 'public class OrderService {');
    expectGutter(sigs, 7, 'private final OrderRepo repo;');
    expectGutter(sigs, 9, 'public OrderService(OrderRepo repo) {');
    expectGutter(sigs, 13, 'public List<String> findAll(String status) {');
    expectGutter(sigs, 18, 'interface Service {');
  });

  it('drops bodies and comments', () => {
    expect(sigs).not.toContain('this.repo = repo;');
    expect(sigs).not.toContain('repo.byStatus');
    expect(sigs).not.toContain('Javadoc');
    expectNoCommentLines(sigs, 'c');
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });
});

// ───────────────────────────── Rust ─────────────────────────────

const RUST_SOURCE = `use std::collections::HashMap;

// module comment should vanish
pub struct Cache {
    entries: HashMap<String, String>,
}

pub trait Store {
    fn get(&self, key: &str) -> Option<String>;
}

impl Store for Cache {
    fn get(&self, key: &str) -> Option<String> {
        let hit = self.entries.get(key);
        hit.cloned()
    }
}

pub fn make_cache() -> Cache {
    Cache { entries: HashMap::new() }
}
`;

describe('extractSignatures — rust', () => {
  const sigs = extractSignatures(RUST_SOURCE, 'cache.rs')!;

  it('keeps use, struct, trait, impl, and fn signatures', () => {
    expectGutter(sigs, 1, 'use std::collections::HashMap;');
    expectGutter(sigs, 4, 'pub struct Cache {');
    expectGutter(sigs, 8, 'pub trait Store {');
    expectGutter(sigs, 9, 'fn get(&self, key: &str) -> Option<String>;');
    expectGutter(sigs, 12, 'impl Store for Cache {');
    expectGutter(sigs, 13, 'fn get(&self, key: &str) -> Option<String> {');
    expectGutter(sigs, 19, 'pub fn make_cache() -> Cache {');
  });

  it('drops bodies and comments', () => {
    expect(sigs).not.toContain('let hit');
    expect(sigs).not.toContain('hit.cloned()');
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs, 'c');
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });

  it('supports the long-name .rust extension alias', () => {
    const aliasSigs = extractSignatures(RUST_SOURCE, 'cache.rust')!;

    expect(aliasSigs).not.toBeNull();
    expectGutter(aliasSigs, 4, 'pub struct Cache {');
    expectGutter(aliasSigs, 19, 'pub fn make_cache() -> Cache {');
  });
});

// ───────────────────────────── C ─────────────────────────────

const C_SOURCE = [
  '#include <stdio.h>',
  '#include "util.h"',
  '',
  '#define MAX_BUF 1024',
  '',
  '/* block comment should vanish */',
  'typedef struct Point {',
  '    int x;',
  '    int y;',
  '} Point;',
  '',
  'enum Color { RED, GREEN };',
  '',
  'static int clamp(int value, int lo, int hi);',
  '',
  'int main(int argc, char **argv) {',
  '    int total = clamp(argc, 0, MAX_BUF);',
  '    printf("%d", total);',
  '    return 0;',
  '}',
  '',
].join('\n');

describe('extractSignatures — c', () => {
  const sigs = extractSignatures(C_SOURCE, 'main.c')!;

  it('keeps #include and #define directives', () => {
    expectGutter(sigs, 1, '#include <stdio.h>');
    expectGutter(sigs, 2, '#include "util.h"');
    expectGutter(sigs, 4, '#define MAX_BUF 1024');
  });

  it('keeps struct/typedef/enum declarations with struct fields', () => {
    expectGutter(sigs, 7, 'typedef struct Point {');
    expectGutter(sigs, 8, 'int x;');
    expectGutter(sigs, 10, '} Point;');
    expectGutter(sigs, 12, 'enum Color { RED, GREEN };');
  });

  it('keeps function prototypes and definition signatures', () => {
    expectGutter(sigs, 14, 'static int clamp(int value, int lo, int hi);');
    expectGutter(sigs, 16, 'int main(int argc, char **argv) {');
  });

  it('drops bodies and comments', () => {
    expect(sigs).not.toContain('printf');
    expect(sigs).not.toContain('int total =');
    expect(sigs).not.toContain('return 0;');
    expect(sigs).not.toContain('block comment should vanish');
    expectNoCommentLines(sigs, 'c');
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });
});

const C_HEADER_SOURCE = [
  '#ifndef OCTOCODE_CLIENT_H',
  '#define OCTOCODE_CLIENT_H',
  '',
  '#ifdef __cplusplus',
  'extern "C" {',
  '#endif',
  '',
  '#include <stddef.h>',
  '',
  'typedef struct OctocodeClient OctocodeClient;',
  '',
  'OctocodeClient *octocode_client_new(const char *token);',
  'int octocode_client_search(',
  '    OctocodeClient *client,',
  '    const char *query',
  ');',
  '',
  '#ifdef __cplusplus',
  '}',
  '#endif',
  '',
  '#endif',
].join('\n');

describe('extractSignatures — h header', () => {
  const sigs = extractSignatures(C_HEADER_SOURCE, 'client.h')!;

  it('keeps header structural declarations and prototypes', () => {
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 2, '#define OCTOCODE_CLIENT_H');
    expectGutter(sigs, 5, 'extern "C" {');
    expectGutter(sigs, 8, '#include <stddef.h>');
    expectGutter(sigs, 10, 'typedef struct OctocodeClient OctocodeClient;');
    expectGutter(
      sigs,
      12,
      'OctocodeClient *octocode_client_new(const char *token);'
    );
    expectGutter(sigs, 13, 'int octocode_client_search(');
    expectGutter(sigs, 14, 'OctocodeClient *client,');
    expectGutter(sigs, 16, ');');
  });

  it('documents that include guards are mostly preprocessor noise', () => {
    expect(sigs).not.toContain('#ifndef OCTOCODE_CLIENT_H');
    expect(sigs).not.toContain('#ifdef __cplusplus');
    expect(sigs).not.toContain('#endif');
    expectNoCommentLines(sigs, 'c');
    expectNoBlankGutterLines(sigs);
  });
});

// ───────────────────────────── C++ ─────────────────────────────

const CPP_SOURCE = `#include <string>

namespace net {

class Socket {
 public:
  explicit Socket(int fd);
  int send(const std::string& payload);

 private:
  int fd_;
};

template <typename T>
T identity(T value) {
  return value;
}

int Socket::send(const std::string& payload) {
  const int written = do_send(fd_, payload);
  log_bytes(written);
  return written;
}

}
`;

describe('extractSignatures — cpp', () => {
  const sigs = extractSignatures(CPP_SOURCE, 'socket.hpp')!;

  it('keeps includes, namespace, class with member signatures, templates', () => {
    expectGutter(sigs, 1, '#include <string>');
    expectGutter(sigs, 3, 'namespace net {');
    expectGutter(sigs, 5, 'class Socket {');
    expectGutter(sigs, 7, 'explicit Socket(int fd);');
    expectGutter(sigs, 8, 'int send(const std::string& payload);');
    expectGutter(sigs, 14, 'template <typename T>');
    expectGutter(sigs, 15, 'T identity(T value) {');
    expectGutter(sigs, 19, 'int Socket::send(const std::string& payload) {');
  });

  it('drops bodies', () => {
    expect(sigs).not.toContain('return value;');
    expect(sigs).not.toContain('do_send');
    expect(sigs).not.toContain('log_bytes');
  });

  it('emits no blank gutter lines and zero comment lines', () => {
    expectNoBlankGutterLines(sigs);
    expectNoCommentLines(sigs, 'c');
  });
});

// ───────────────────────────── Ruby ─────────────────────────────

const RB_SOURCE = `require 'json'
require_relative 'helpers'

# comment should vanish
module Billing
  class Invoice
    attr_reader :total

    def initialize(total)
      @total = total
    end

    def to_json(*args)
      { total: @total }.to_json(*args)
    end
  end
end
`;

describe('extractSignatures — ruby', () => {
  const sigs = extractSignatures(RB_SOURCE, 'invoice.rb')!;

  it('keeps requires, module/class, attr_* and def signatures', () => {
    expectGutter(sigs, 1, "require 'json'");
    expectGutter(sigs, 2, "require_relative 'helpers'");
    expectGutter(sigs, 5, 'module Billing');
    expectGutter(sigs, 6, 'class Invoice');
    expectGutter(sigs, 7, 'attr_reader :total');
    expectGutter(sigs, 9, 'def initialize(total)');
    expectGutter(sigs, 13, 'def to_json(*args)');
  });

  it('drops bodies and comments', () => {
    expect(sigs).not.toContain('@total = total');
    expect(sigs).not.toContain('{ total:');
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs, 'hash');
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });
});

// ───────────────────────────── PHP ─────────────────────────────

const PHP_SOURCE = [
  '<?php',
  '',
  'namespace App\\Services;',
  '',
  'use App\\Models\\User;',
  '',
  '// comment should vanish',
  'final class UserService implements ServiceInterface',
  '{',
  "    private const ROLE = 'admin';",
  '',
  '    public function find(int $id): ?User',
  '    {',
  '        return $this->repo->find($id);',
  '    }',
  '}',
  '',
  'function helper(array $items): int',
  '{',
  '    return count($items);',
  '}',
  '',
].join('\n');

describe('extractSignatures — php', () => {
  const sigs = extractSignatures(PHP_SOURCE, 'UserService.php')!;

  it('keeps namespace, use, class/interface and function signatures', () => {
    expectGutter(sigs, 3, 'namespace App\\Services;');
    expectGutter(sigs, 5, 'use App\\Models\\User;');
    expectGutter(
      sigs,
      8,
      'final class UserService implements ServiceInterface'
    );
    expectGutter(sigs, 10, "private const ROLE = 'admin';");
    expectGutter(sigs, 12, 'public function find(int $id): ?User');
    expectGutter(sigs, 18, 'function helper(array $items): int');
  });

  it('drops bodies and comments', () => {
    expect(sigs).not.toContain('return $this->repo->find($id);');
    expect(sigs).not.toContain('return count($items);');
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs, 'php');
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });
});

// ───────────────────────────── Swift ─────────────────────────────

const SWIFT_SOURCE = `import Foundation

// comment should vanish
public protocol Greeter {
    func greet(name: String) -> String
}

public struct Person {
    public let name: String

    public init(name: String) {
        self.name = name
    }
}

public class Registry {
    @discardableResult
    public func register(
        _ person: Person,
        overwrite: Bool = false
    ) -> Bool {
        let key = person.name
        return store(key)
    }
}

extension Person {
    public var description: String {
        return "person"
    }
}
`;

describe('extractSignatures — swift', () => {
  const sigs = extractSignatures(SWIFT_SOURCE, 'person.swift')!;

  it('keeps import, protocol/struct/class/extension and member signatures', () => {
    expectGutter(sigs, 1, 'import Foundation');
    expectGutter(sigs, 4, 'public protocol Greeter {');
    expectGutter(sigs, 5, 'func greet(name: String) -> String');
    expectGutter(sigs, 8, 'public struct Person {');
    expectGutter(sigs, 9, 'public let name: String');
    expectGutter(sigs, 11, 'public init(name: String) {');
    expectGutter(sigs, 16, 'public class Registry {');
    expectGutter(sigs, 17, '@discardableResult');
    expectGutter(sigs, 18, 'public func register(');
    expectGutter(sigs, 20, 'overwrite: Bool = false');
    expectGutter(sigs, 21, ') -> Bool {');
    expectGutter(sigs, 27, 'extension Person {');
    expectGutter(sigs, 28, 'public var description: String {');
  });

  it('drops bodies and comments', () => {
    expect(sigs).not.toContain('self.name = name');
    expect(sigs).not.toContain('let key');
    expect(sigs).not.toContain('return store(key)');
    expect(sigs).not.toContain('return "person"');
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs, 'c');
  });

  it('emits no blank gutter lines', () => {
    expectNoBlankGutterLines(sigs);
  });
});

// ───────────────────────────── CSS / SCSS / LESS ─────────────────────────────

const SCSS_SOURCE = `// line comment should vanish
@use 'sass:math';
@import 'base';

$primary-color: #336699;
$spacing: 8px;

/* block comment
   should vanish */
.card,
.card--wide {
  color: $primary-color;
  padding: $spacing;
}

@mixin flex-center($gap: 0) {
  display: flex;
  gap: $gap;
}

@media (max-width: 768px) {
  .card {
    padding: math.div($spacing, 2);
  }
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
.inline-rule { margin: 0 }
`;

describe('extractSignatures — scss', () => {
  const sigs = extractSignatures(SCSS_SOURCE, 'theme.scss')!;

  it('keeps standalone at-rules and top-level variables', () => {
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 2, "@use 'sass:math';");
    expectGutter(sigs, 3, "@import 'base';");
    expectGutter(sigs, 5, '$primary-color: #336699;');
    expectGutter(sigs, 6, '$spacing: 8px;');
  });

  it('keeps selector heads including multi-selector lists', () => {
    expectGutter(sigs, 10, '.card,');
    expectGutter(sigs, 11, '.card--wide {');
    expectGutter(sigs, 22, '.card {');
  });

  it('keeps @mixin/@media/@keyframes heads', () => {
    expectGutter(sigs, 16, '@mixin flex-center($gap: 0) {');
    expectGutter(sigs, 21, '@media (max-width: 768px) {');
    expectGutter(sigs, 27, '@keyframes spin {');
  });

  it('keeps single-line rules whole (selector would be lost otherwise)', () => {
    expectGutter(sigs, 31, '.inline-rule { margin: 0 }');
  });

  it('drops declaration bodies', () => {
    expect(sigs).not.toContain('color: $primary-color;');
    expect(sigs).not.toContain('display: flex;');
    expect(sigs).not.toContain('gap: $gap;');
    expect(sigs).not.toContain('math.div');
    expect(sigs).not.toContain('rotate(0deg)');
  });

  it('drops comments and emits no blank gutter lines', () => {
    expect(sigs).not.toContain('comment should vanish');
    expect(sigs).not.toContain('should vanish');
    expectNoCommentLines(sigs, 'c');
    expectNoBlankGutterLines(sigs);
  });
});

const CSS_SOURCE = `@import url("theme.css");

body {
  margin: 0;
}
`;

describe('extractSignatures — css/less extensions', () => {
  it('handles .css files', () => {
    const sigs = extractSignatures(CSS_SOURCE, 'main.css')!;
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 1, '@import url("theme.css");');
    expectGutter(sigs, 3, 'body {');
    expect(sigs).not.toContain('margin: 0;');
  });

  it('handles .less files', () => {
    const sigs = extractSignatures(CSS_SOURCE, 'main.less')!;
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 3, 'body {');
  });
});

// ───────────────────────────── HTML ─────────────────────────────

const HTML_SOURCE = `<!DOCTYPE html>
<html lang="en">
<head>
  <!-- head comment should vanish -->
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Demo</title>
  <link href="css/main.css" rel="stylesheet">
  <script src="js/app.js" defer></script>
  <style>
    .hidden { display: none; }
  </style>
  <script>
    console.log('inline body should vanish');
  </script>
</head>
<body>
  <h1>Main Heading</h1>
  <p>Prose that should vanish.</p>
  <div id="app" class="root">
    <h2>Sub Heading</h2>
    <span>more prose to vanish</span>
    <button id="submit-btn" type="button">Go</button>
  </div>
</body>
</html>
`;

describe('extractSignatures — html', () => {
  const sigs = extractSignatures(HTML_SOURCE, 'index.html')!;

  it('keeps doctype, named meta, link href, and script src', () => {
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 1, '<!DOCTYPE html>');
    expectGutter(sigs, 6, '<meta name="viewport"');
    expectGutter(sigs, 8, '<link href="css/main.css"');
    expectGutter(sigs, 9, '<script src="js/app.js"');
  });

  it('keeps headings with their text and tags carrying id=', () => {
    expectGutter(sigs, 18, '<h1>Main Heading</h1>');
    expectGutter(sigs, 20, '<div id="app"');
    expectGutter(sigs, 21, '<h2>Sub Heading</h2>');
    expectGutter(sigs, 23, '<button id="submit-btn"');
  });

  it('drops prose, titles, and inline style/script bodies', () => {
    expect(sigs).not.toContain('<title>');
    expect(sigs).not.toContain('Prose that should vanish');
    expect(sigs).not.toContain('more prose to vanish');
    expect(sigs).not.toContain('display: none');
    expect(sigs).not.toContain('console.log');
  });

  it('drops comments and emits no blank gutter lines', () => {
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs, 'html');
    expectNoBlankGutterLines(sigs);
  });

  it('handles the .htm extension', () => {
    const htm = extractSignatures(HTML_SOURCE, 'index.htm')!;
    expect(htm).not.toBeNull();
    expectGutter(htm, 18, '<h1>Main Heading</h1>');
  });
});

// ───────────────────────────── Vue ─────────────────────────────

const VUE_SOURCE = `<template>
  <div id="root" class="wrapper">
    <!-- template comment should vanish -->
    <p>{{ greeting }} prose should vanish</p>
    <UserCard :user="user" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import UserCard from './UserCard.vue';

interface User {
  name: string;
}

const user = ref<User>({ name: 'Ada' });

function load(id: number): Promise<User> {
  return fetch('/api/users/' + id).then(r => r.json());
}

const greeting = computed(() => {
  return 'Hello ' + user.value.name;
});
</script>

<style scoped>
.wrapper {
  color: red;
}
</style>
`;

describe('extractSignatures — vue (script block via AST)', () => {
  const sigs = extractSignatures(VUE_SOURCE, 'UserPanel.vue')!;

  it('keeps the template root line and elements with id=', () => {
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 1, '<template>');
    expectGutter(sigs, 2, '<div id="root" class="wrapper">');
  });

  it('keeps the script opener and AST signatures at ORIGINAL line numbers', () => {
    expectGutter(sigs, 9, '<script setup lang="ts">');
    expectGutter(sigs, 10, "import { ref, computed } from 'vue';");
    expectGutter(sigs, 11, "import UserCard from './UserCard.vue';");
    expectGutter(sigs, 13, 'interface User {');
    expectGutter(sigs, 14, 'name: string;');
    expectGutter(sigs, 17, "const user = ref<User>({ name: 'Ada' });");
    expectGutter(sigs, 19, 'function load(id: number): Promise<User> {');
    expectGutter(sigs, 23, 'const greeting = computed(() => {');
  });

  it('drops script bodies, template prose, and style bodies', () => {
    expect(sigs).not.toContain('fetch(');
    expect(sigs).not.toContain("'Hello '");
    expect(sigs).not.toContain('{{ greeting }}');
    expect(sigs).not.toContain('<p>');
    expect(sigs).not.toContain('color: red');
  });

  it('drops comments and emits no blank gutter lines', () => {
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs, 'html');
    expectNoBlankGutterLines(sigs);
  });
});

// ───────────────────────────── Svelte ─────────────────────────────

const SVELTE_SOURCE = `<script lang="ts">
  import { onMount } from 'svelte';

  export let title: string;

  function toggle(open: boolean): boolean {
    return !open;
  }
</script>

<!-- markup comment should vanish -->
<main id="layout">
  <h1>{title}</h1>
  <p>prose should vanish</p>
</main>

<style>
  main {
    padding: 8px;
  }
</style>
`;

describe('extractSignatures — svelte (script block via AST)', () => {
  const sigs = extractSignatures(SVELTE_SOURCE, 'Layout.svelte')!;

  it('keeps the script opener and AST signatures at ORIGINAL line numbers', () => {
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 1, '<script lang="ts">');
    expectGutter(sigs, 2, "import { onMount } from 'svelte';");
    expectGutter(sigs, 4, 'export let title: string;');
    expectGutter(sigs, 6, 'function toggle(open: boolean): boolean {');
  });

  it('keeps markup elements with id=', () => {
    expectGutter(sigs, 12, '<main id="layout">');
  });

  it('drops script bodies, markup prose, and style bodies', () => {
    expect(sigs).not.toContain('return !open;');
    expect(sigs).not.toContain('<h1>');
    expect(sigs).not.toContain('<p>');
    expect(sigs).not.toContain('padding: 8px');
  });

  it('drops comments and emits no blank gutter lines', () => {
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs, 'html');
    expectNoBlankGutterLines(sigs);
  });
});

// ───────────────────────────── SQL ─────────────────────────────

const SQL_SOURCE = `-- line comment should vanish
CREATE TABLE users (
    id BIGSERIAL PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ DEFAULT now()
);

/* block comment
   should vanish */
CREATE UNIQUE INDEX idx_users_email ON users (email);

CREATE VIEW active_users AS
SELECT id, email FROM users WHERE active;

CREATE OR REPLACE FUNCTION add_user(p_email TEXT)
RETURNS BIGINT AS $$
DECLARE
    new_id BIGINT;
BEGIN
    INSERT INTO users (email) VALUES (p_email) RETURNING id INTO new_id;
    RETURN new_id;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_audit
AFTER INSERT ON users
FOR EACH ROW EXECUTE FUNCTION audit_row();
`;

describe('extractSignatures — sql', () => {
  const sigs = extractSignatures(SQL_SOURCE, 'schema.sql')!;

  it('keeps CREATE TABLE with its full column list', () => {
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 2, 'CREATE TABLE users (');
    expectGutter(sigs, 3, 'id BIGSERIAL PRIMARY KEY,');
    expectGutter(sigs, 4, 'email TEXT NOT NULL UNIQUE,');
    expectGutter(sigs, 5, 'created_at TIMESTAMPTZ DEFAULT now()');
    expectGutter(sigs, 6, ');');
  });

  it('keeps CREATE INDEX/VIEW/FUNCTION/TRIGGER heads', () => {
    expectGutter(sigs, 10, 'CREATE UNIQUE INDEX idx_users_email ON users');
    expectGutter(sigs, 12, 'CREATE VIEW active_users AS');
    expectGutter(sigs, 15, 'CREATE OR REPLACE FUNCTION add_user(p_email TEXT)');
    expectGutter(sigs, 16, 'RETURNS BIGINT AS $$');
    expectGutter(sigs, 25, 'CREATE TRIGGER trg_audit');
    expectGutter(sigs, 26, 'AFTER INSERT ON users');
    expectGutter(sigs, 27, 'FOR EACH ROW EXECUTE FUNCTION audit_row();');
  });

  it('drops view SELECT bodies and $$/BEGIN…END function bodies', () => {
    expect(sigs).not.toContain('SELECT id, email');
    expect(sigs).not.toContain('DECLARE');
    expect(sigs).not.toContain('BEGIN');
    expect(sigs).not.toContain('INSERT INTO');
    expect(sigs).not.toContain('RETURN new_id');
    expect(sigs).not.toContain('END;');
    expect(sigs).not.toContain('$$ LANGUAGE plpgsql;');
  });

  it('drops -- and /* */ comments and emits no blank gutter lines', () => {
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs, 'sql');
    expectNoBlankGutterLines(sigs);
  });
});

// ───────────────────────────── Shell ─────────────────────────────

const SH_SOURCE = [
  '#!/usr/bin/env bash',
  '# comment should vanish',
  'set -euo pipefail',
  '',
  'source ./lib/colors.sh',
  '. ./lib/utils.sh',
  '',
  'export BUILD_DIR="dist"',
  'export RELEASE_CHANNEL=stable',
  '',
  'log_info() {',
  '  echo "[info] $1"',
  '}',
  '',
  'function deploy_app {',
  '  local target="$1"',
  '  log_info "deploying ${target}"',
  '  export INNER_SECRET=should_vanish',
  '}',
  '',
  'main() {',
  '  deploy_app "$BUILD_DIR"',
  '}',
  '',
  'main "$@"',
  '',
].join('\n');

describe('extractSignatures — shell (sh/bash/zsh)', () => {
  const sigs = extractSignatures(SH_SOURCE, 'deploy.sh')!;

  it('keeps the shebang, source/. lines, and exports', () => {
    expect(sigs).not.toBeNull();
    expectGutter(sigs, 1, '#!/usr/bin/env bash');
    expectGutter(sigs, 5, 'source ./lib/colors.sh');
    expectGutter(sigs, 6, '. ./lib/utils.sh');
    expectGutter(sigs, 8, 'export BUILD_DIR="dist"');
    expectGutter(sigs, 9, 'export RELEASE_CHANNEL=stable');
  });

  it('keeps both function definition styles', () => {
    expectGutter(sigs, 11, 'log_info() {');
    expectGutter(sigs, 15, 'function deploy_app {');
    expectGutter(sigs, 21, 'main() {');
  });

  it('drops function bodies (including exports inside bodies) and calls', () => {
    expect(sigs).not.toContain('echo "[info] $1"');
    expect(sigs).not.toContain('local target');
    expect(sigs).not.toContain('INNER_SECRET');
    expect(sigs).not.toContain('deploy_app "$BUILD_DIR"');
    expect(sigs).not.toContain('main "$@"');
    expect(sigs).not.toContain('set -euo pipefail');
  });

  it('drops comments (shebang exempt) and emits no blank gutter lines', () => {
    expect(sigs).not.toContain('comment should vanish');
    expectNoCommentLines(sigs, 'hash');
    expectNoBlankGutterLines(sigs);
  });

  it('handles the .bash and .zsh extensions', () => {
    const bash = extractSignatures(SH_SOURCE, 'deploy.bash')!;
    expect(bash).not.toBeNull();
    expectGutter(bash, 11, 'log_info() {');
    const zsh = extractSignatures(SH_SOURCE, 'deploy.zsh')!;
    expect(zsh).not.toBeNull();
    expectGutter(zsh, 15, 'function deploy_app {');
  });

  it('keeps shell functions whose opening brace is on the next line', () => {
    const src = ['build_release()', '{', '  echo "building"', '}'].join('\n');
    const nextLineBrace = extractSignatures(src, 'release.sh')!;
    expect(nextLineBrace).not.toBeNull();
    expectGutter(nextLineBrace, 1, 'build_release()');
    expect(nextLineBrace).not.toContain('echo "building"');
  });
});

// ───────────────────────────── Negative fixtures ─────────────────────────────

describe('extractSignatures — negative fixtures', () => {
  it('returns null for a comments-only ts file', () => {
    const src = '// only a comment\n/* block */\n/** jsdoc */\n';
    expect(extractSignatures(src, 'comments.ts')).toBeNull();
  });

  it('returns null for a comments-only python file', () => {
    const src = '# just a comment\n# another\n';
    expect(extractSignatures(src, 'comments.py')).toBeNull();
  });

  it('returns null for an empty file', () => {
    expect(extractSignatures('', 'empty.ts')).toBeNull();
    expect(extractSignatures('\n\n', 'empty.py')).toBeNull();
  });

  it('survives a minified one-liner js file (parser robustness)', () => {
    const src =
      "export function a(){return fetch('/x')}export const b=2;export class C{m(){return 1}}";
    const sigs = extractSignatures(src, 'min.js');
    expect(sigs).not.toBeNull();
    expect(sigs).toContain('function a');
    expectNoBlankGutterLines(sigs!);
  });

  it('returns null for unsupported extensions (README)', () => {
    expect(
      extractSignatures('# Title\n\nSome prose.\n', 'README.md')
    ).toBeNull();
  });
});

// ───────────────────────── Savings summary (e) ─────────────────────────

describe('extractSignatures — savings', () => {
  const fixtures: Array<[string, string, string]> = [
    ['ts', TS_CLASS, 'service.ts'],
    ['tsx', TSX_COMPONENT, 'card.tsx'],
    ['cjs', JS_CJS, 'loader.cjs'],
    ['py', PY_SOURCE, 'client.py'],
    ['go', GO_SOURCE, 'client.go'],
    ['java', JAVA_SOURCE, 'OrderService.java'],
    ['rs', RUST_SOURCE, 'cache.rs'],
    ['c', C_SOURCE, 'main.c'],
    ['cpp', CPP_SOURCE, 'socket.hpp'],
    ['rb', RB_SOURCE, 'invoice.rb'],
    ['php', PHP_SOURCE, 'UserService.php'],
    ['swift', SWIFT_SOURCE, 'person.swift'],
    ['scss', SCSS_SOURCE, 'theme.scss'],
    ['html', HTML_SOURCE, 'index.html'],
    ['vue', VUE_SOURCE, 'UserPanel.vue'],
    ['svelte', SVELTE_SOURCE, 'Layout.svelte'],
    ['sql', SQL_SOURCE, 'schema.sql'],
    ['sh', SH_SOURCE, 'deploy.sh'],
  ];

  it.each(fixtures)(
    '%s skeleton is smaller than the source',
    (_lang, src, file) => {
      const sigs = extractSignatures(src, file)!;
      expect(sigs).not.toBeNull();
      expect(sigs.length).toBeLessThan(src.length);
      // Body-dropping must remove lines, not just characters.
      expect(sigs.split('\n').length).toBeLessThan(src.split('\n').length);
    }
  );
});
