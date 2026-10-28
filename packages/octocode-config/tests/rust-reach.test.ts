import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { NATIVE_CRATES } from '../scripts/tool-contract/rust.ts';
import {
  blankLiterals,
  classify,
  itemRanges,
  keepReachable,
  nativeIdentifiers,
} from '../scripts/tool-contract/rust-reach.ts';

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
  [...rust.matchAll(/^pub (?:struct|enum|type) (\w+)/gm)].map(([, name]) => name as string);

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
    // The pinned header (comments and constants) is not typify output.
    const rust = readFileSync(resolve(packageRoot, 'contract/tool_types.rs'), 'utf8')
      .split('\n')
      .filter(line => !line.startsWith('//') && !line.startsWith('pub const '))
      .join('\n');
    const roots = nativeIdentifiers(NATIVE_CRATES);
    expect(roots.has('LocalFetchQuery')).toBe(true);
    // Nothing is left to drop: every emitted type is reachable from native.
    expect(declared(keepReachable(rust, roots))).toEqual(declared(rust));
  }, 30_000);
});
