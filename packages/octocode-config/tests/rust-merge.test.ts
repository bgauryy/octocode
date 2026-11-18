import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { NATIVE_CRATES, withDispatchRoots } from '../scripts/tool-contract/rust.ts';
import { queryBranches, toolNames } from '../scripts/tool-contract/rust-dispatch.ts';
import { mergeDuplicateTypes } from '../scripts/tool-contract/rust-merge.ts';
import { nativeIdentifiers } from '../scripts/tool-contract/rust-reach.ts';
import { contractIr } from './contract-ir.ts';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** A one-value string enum as typify emits it, with its conversions. */
const literal = (name: string, value = 'continue'): string => `#[doc = "\`${name}\`"]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ${name} {
    #[serde(rename = "${value}")]
    Continue,
}
impl ::std::fmt::Display for ${name} {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Self::Continue => f.write_str("${value}"),
        }
    }
}`;

const call = (name: string, kind: string, defaulted = 'call_kind'): string => `#[doc = "\`${name}\`"]
#[derive(Clone, Debug, PartialEq)]
pub struct ${name} {
    #[serde(default = "defaults::${defaulted}")]
    pub kind: ${kind},
    pub cursor: ::std::string::String,
}`;

const declared = (rust: string): string[] =>
  [...rust.matchAll(/^pub (?:struct|enum) (\w+)/gm)].flatMap(([, name]) => (name ? [name] : []));

const defaults = (body: string): string => `pub mod defaults {
${body}
}
pub mod error {
    pub struct ConversionError(::std::borrow::Cow<'static, str>);
}`;

describe('merging generated Rust types', () => {
  it('emits one type per shape, nested shapes included, under the name the copies share', () => {
    const generated = [
      literal('AlphaContinueCallKind'),
      literal('BetaContinueCallKind'),
      call('AlphaContinueCall', 'AlphaContinueCallKind', 'alpha_continue_call_kind'),
      call('BetaContinueCall', 'BetaContinueCallKind', 'beta_continue_call_kind'),
      `pub struct Root {
    pub alpha: AlphaContinueCall,
    pub beta: ::std::vec::Vec<BetaContinueCall>,
}`,
      defaults(`    pub(super) fn alpha_continue_call_kind() -> super::AlphaContinueCallKind {
        super::AlphaContinueCallKind::Continue
    }
    pub(super) fn beta_continue_call_kind() -> super::BetaContinueCallKind {
        super::BetaContinueCallKind::Continue
    }`),
    ].join('\n');
    const { rust: merged, renamed } = mergeDuplicateTypes(generated, new Set(['Root']));
    expect(declared(merged)).toEqual(['ContinueCallKind', 'ContinueCall', 'Root']);
    // Each copy reports the name it now goes by.
    expect(Object.fromEntries(renamed)).toEqual({
      AlphaContinueCallKind: 'ContinueCallKind',
      BetaContinueCallKind: 'ContinueCallKind',
      AlphaContinueCall: 'ContinueCall',
      BetaContinueCall: 'ContinueCall',
    });
    expect(merged).toContain('#[doc = "`ContinueCallKind`"]');
    expect(merged).toContain('pub alpha: ContinueCall,');
    expect(merged).toContain('pub beta: ::std::vec::Vec<ContinueCall>,');
    expect(merged).toContain('impl ::std::fmt::Display for ContinueCallKind');
    // The kept copy's default stays; the one only a dropped copy called goes.
    expect(merged).toContain('#[serde(default = "defaults::alpha_continue_call_kind")]');
    expect(merged).toContain('fn alpha_continue_call_kind() -> super::ContinueCallKind');
    expect(merged).not.toContain('beta_continue_call_kind');
    expect(merged).not.toMatch(/\bAlpha|\bBeta/);
    expect(mergeDuplicateTypes(generated, new Set(['Root'])).rust).toBe(merged);
  });

  it('keeps types apart when their wire behavior differs', () => {
    const generated = [
      literal('AlphaKind'),
      literal('BetaKind', 'stop'),
      call('AlphaCall', 'AlphaKind', 'one'),
      call('BetaCall', 'AlphaKind', 'two'),
      defaults(`    pub(super) fn one() -> super::AlphaKind {
        super::AlphaKind::Continue
    }
    pub(super) fn two() -> i64 {
        2
    }`),
    ].join('\n');
    expect(mergeDuplicateTypes(generated, new Set())).toEqual({ rust: generated, renamed: new Map() });
  });

  it('keeps every name native uses: one names its set, two stay two types', () => {
    const generated = [
      literal('AlphaKind'),
      literal('BetaKind'),
      literal('GammaKind'),
      literal('DeltaKind'),
      `pub struct AlphaCall {
    pub kind: AlphaKind,
    pub cursor: ::std::string::String,
}`,
      `pub struct BetaCall {
    pub kind: BetaKind,
    pub cursor: ::std::string::String,
}`,
    ].join('\n');
    // One kept name: every copy takes it. A set whose names share one word
    // and whose type has more than one member takes its shortest name.
    expect(declared(mergeDuplicateTypes(generated, new Set(['GammaKind'])).rust)).toEqual(['GammaKind', 'BetaCall']);
    // Two kept names: both stay, the other copies merge apart, and so do the
    // structs that hold them.
    const split = mergeDuplicateTypes(generated, new Set(['AlphaKind', 'BetaKind'])).rust;
    // A one-member type is named by that member and the role its names share.
    expect(declared(split)).toEqual(['AlphaKind', 'BetaKind', 'ContinueKind', 'AlphaCall', 'BetaCall']);
    expect(split).toContain('pub kind: BetaKind,');
  });

  it('drops the From impls two variants would share once their types merge', () => {
    const generated = [
      literal('AlphaKind'),
      literal('BetaKind'),
      `pub enum Either {
    Alpha(AlphaKind),
    Beta(BetaKind),
}
impl ::std::convert::From<AlphaKind> for Either {
    fn from(value: AlphaKind) -> Self {
        Self::Alpha(value)
    }
}
impl ::std::convert::From<BetaKind> for Either {
    fn from(value: BetaKind) -> Self {
        Self::Beta(value)
    }
}`,
    ].join('\n');
    const merged = mergeDuplicateTypes(generated, new Set(['Either'])).rust;
    expect(declared(merged)).toEqual(['ContinueKind', 'Either']);
    expect(merged).toContain('Alpha(ContinueKind),');
    expect(merged).toContain('Beta(ContinueKind),');
    expect(merged).not.toContain('for Either');
  });

  it('leaves nothing to merge in the generated file', () => {
    const file = readFileSync(resolve(packageRoot, 'contract/tool_types.rs'), 'utf8');
    // typify's output, without the pinned header and the dispatch after it.
    const rust = file
      .slice(0, file.indexOf('/// How a continuation moves'))
      .split('\n')
      .filter(line => !line.startsWith('//') && !line.startsWith('pub const ') && !line.startsWith('pub use '))
      .join('\n');
    const tools = toolNames(contractIr().tools);
    const named = withDispatchRoots(nativeIdentifiers(NATIVE_CRATES), tools);
    const kept = new Set([...named, ...queryBranches(rust, tools).map(({ type }) => type)]);
    expect(mergeDuplicateTypes(rust, kept).rust).toBe(rust);
  });
});
