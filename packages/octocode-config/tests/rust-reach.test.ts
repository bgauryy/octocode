import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { BASE_CRATE, NATIVE_CRATES, withDispatchRoots } from '../scripts/tool-contract/rust.ts';
import { toolNames } from '../scripts/tool-contract/rust-dispatch.ts';
import {
  blankLiterals,
  classify,
  itemRanges,
  keepReachable,
  namesThrough,
  nativeIdentifiers,
  withoutTypesOf,
} from '../scripts/tool-contract/rust-reach.ts';
import { contractIr } from './contract-ir.ts';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const generated = `#[doc = "\`Root\` names Unused only in this doc: \\"Unused\\""]
#[derive(Clone, Debug)]
pub struct Root {
    #[serde(default = "defaults::root_mode")]
    pub mode: RootMode,
    pub path: RootPath,
}
#[derive(Clone, Copy, Debug)]
pub enum RootMode {
    On,
}
#[derive(Clone, Debug)]
pub struct RootPath(::std::string::String);
impl ::std::str::FromStr for RootPath {
    type Err = self::error::ConversionError;
    fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
        static PATTERN: ::std::sync::LazyLock<::regress::Regex> =
            ::std::sync::LazyLock::new(|| ::regress::Regex::new("^[{(\\"]$").unwrap());
        let _ = &*PATTERN;
        Ok(Self(value.to_string()))
    }
}
impl ::std::convert::From<RootPath> for ::std::string::String {
    fn from(value: RootPath) -> Self {
        value.0
    }
}
impl :: std :: convert :: From < Unused > for Root {
    fn from(_: Unused) -> Self {
        unimplemented!()
    }
}
#[derive(Clone, Debug)]
pub struct Unused(pub Deeper);
impl<'de> ::serde::Deserialize<'de> for Unused {
    fn deserialize<D>(_: D) -> Result<Self, D::Error> {
        let _c = '{';
        unimplemented!()
    }
}
#[derive(Clone, Debug)]
pub struct Deeper(pub i64);
pub mod defaults {
    pub(super) fn default_bool<const V: bool>() -> bool {
        V
    }
    pub(super) fn root_mode() -> super::RootMode {
        super::RootMode::On
    }
    pub(super) fn unused() -> super::Unused {
        super::Unused(super::Deeper(0))
    }
}
pub mod error {
    pub struct ConversionError(::std::borrow::Cow<'static, str>);
}
`;

const declared = (rust: string): string[] =>
  [...rust.matchAll(/^pub (?:struct|enum|type) (\w+)/gm)].flatMap(([, name]) => (name ? [name] : []));

describe('generated Rust reachability', () => {
  it('keeps the root types, what they name and their impls; drops the rest', () => {
    const kept = keepReachable(generated, new Set(['Root', 'unrelated']));
    expect(declared(kept)).toEqual(['Root', 'RootMode', 'RootPath']);
    // An impl naming a dropped type goes with it and never keeps it.
    expect(kept).not.toMatch(/From\s*<\s*Unused/);
    expect(kept).toContain('impl ::std::convert::From<RootPath> for ::std::string::String');
    expect(kept).toContain('impl ::std::str::FromStr for RootPath');
    // Default functions follow their types; generic ones and `error` stay.
    expect(kept).toContain('fn default_bool');
    expect(kept).toContain('fn root_mode');
    expect(kept).not.toContain('fn unused');
    expect(kept).toContain('pub mod error');
  });

  it('follows a type only through code, never through docs or literals', () => {
    expect(declared(keepReachable(generated, new Set(['Unused'])))).toEqual(['Unused', 'Deeper']);
    expect(declared(keepReachable(generated, new Set()))).toEqual([]);
  });

  it('splits items around literals that hold brackets, and blanks literals in place', () => {
    const source = `let a = "}{"; let b = r#"]"#; let c = '['; let d: &'static str = "x";`;
    const blanked = blankLiterals(source);
    expect(blanked).toHaveLength(source.length);
    expect(blanked).not.toMatch(/[}{\][]/);
    expect(blanked).toContain("&'static str");
    expect(itemRanges(blankLiterals(generated)).length).toBeGreaterThan(0);
    expect(() => classify('const X: i64 = 1;')).toThrow(/unexpected generated Rust item/);
  });

  it('emits exactly the types native names or reaches', () => {
    // The pinned header (comments and constants) and the generated dispatch
    // after typify's output (a trait, a constant table) are not typify output.
    const file = readFileSync(resolve(packageRoot, 'contract/tool_types.rs'), 'utf8');
    const rust = file
      .slice(0, file.indexOf('/// How a continuation moves'))
      .split('\n')
      .filter(line => !line.startsWith('//') && !line.startsWith('pub const ') && !line.startsWith('pub use '))
      .join('\n');
    const roots = withDispatchRoots(
      nativeIdentifiers(NATIVE_CRATES),
      toolNames(contractIr().tools)
    );
    expect(roots.has('LocalFetchQuery')).toBe(true);
    // Nothing is left to drop: every emitted type is reachable from native.
    expect(declared(keepReachable(rust, roots))).toEqual(declared(rust));
    // The base file holds exactly what native names through the base crate,
    // and the types file declares none of it again.
    const base = readFileSync(resolve(packageRoot, 'contract/tool_types_base.rs'), 'utf8')
      .split('\n')
      .filter(line => !line.startsWith('//'))
      .join('\n');
    const baseRoots = namesThrough(NATIVE_CRATES, BASE_CRATE);
    expect(baseRoots.has('ErrorCode')).toBe(true);
    expect(declared(keepReachable(base, baseRoots))).toEqual(declared(base));
    expect(declared(rust).filter(name => declared(base).includes(name))).toEqual([]);
    expect(file).toContain(`pub use ::${BASE_CRATE}::{error, ${declared(base).join(', ')}};`);
  });
});

describe('the base crate split', () => {
  it('reads the names native reaches through a crate path, use trees included', () => {
    const dir = mkdtempSync(join(tmpdir(), 'names-through-'));
    try {
      mkdirSync(join(dir, 'github', 'src'), { recursive: true });
      mkdirSync(join(dir, 'target'));
      writeFileSync(join(dir, 'github', 'src', 'lib.rs'), `use base::{Wait, Kind::Spacing, nested::{Deep}};
// base::InComment
fn f() -> base :: Code { let _ = "base::InString"; base::Other::new() }`);
      writeFileSync(join(dir, 'target', 'skipped.rs'), 'use base::Built;');
      expect([...namesThrough(dir, 'base')].sort()).toEqual(['Code', 'Kind', 'Other', 'Wait', 'nested']);
      writeFileSync(join(dir, 'github', 'src', 'glob.rs'), 'use base::{Wait, *};');
      expect(() => namesThrough(dir, 'base')).toThrow(/instead of importing \*/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('leaves out the types the other crate declares, the impls and defaults only they use, and the error module', () => {
    const base = keepReachable(generated, new Set(['RootPath']));
    expect(declared(base)).toEqual(['RootPath']);
    const rest = withoutTypesOf(generated, base);
    expect(declared(rest)).toEqual(['Root', 'RootMode', 'Unused', 'Deeper']);
    expect(rest).not.toContain('impl ::std::str::FromStr for RootPath');
    expect(rest).not.toContain('From<RootPath> for ::std::string::String');
    // A type still here keeps naming the moved one.
    expect(rest).toContain('pub path: RootPath,');
    expect(rest).toContain('fn root_mode');
    expect(rest).not.toContain('pub mod error');
    expect(base).toContain('pub mod error');
  });
});
