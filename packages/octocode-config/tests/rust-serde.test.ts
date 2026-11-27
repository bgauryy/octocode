import { describe, expect, it } from 'vitest';
import { decodedTypes, encodeOnly } from '../scripts/tool-contract/rust-serde.ts';

const struct = (name: string, ...fields: string[]): string => `#[doc = "\`${name}\`"]
#[derive(:: serde :: Deserialize, :: serde :: Serialize, Clone, Debug, PartialEq)]
pub struct ${name} {
${fields.map((type, index) => `    pub f${index}: ${type},`).join('\n')}
}`;

/** A validated string newtype with typify's hand-written `Deserialize`. */
const text = (name: string): string => `#[derive(
    :: serde :: Deserialize,
    :: serde :: Serialize,
    Clone,
    Debug,
)]
#[serde(transparent)]
pub struct ${name}(::std::string::String);
impl<'de> ::serde::Deserialize<'de> for ${name} {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        ::std::string::String::deserialize(deserializer).map(Self)
    }
}`;

const generated = [
  struct('ToolQuery', 'Shared', 'QueryOnly'),
  struct('ToolInput', '::std::vec::Vec<ToolQuery>'),
  struct('ToolOutput', 'Row', 'Shared'),
  struct('Row', 'RowText', '::std::option::Option<Detail>'),
  struct('Detail', 'i64'),
  struct('Shared', 'i64'),
  struct('QueryOnly', 'i64'),
  struct('Standalone', 'Detail'),
  text('RowText'),
].join('\n');

const roles = { inputs: ['ToolQuery', 'ToolInput'], outputs: ['ToolOutput'] };
const derivesDeserialize = (rust: string, name: string): boolean =>
  new RegExp(`Deserialize,[^\\]]*\\)\\]\\s*(?:#\\[serde[^\\]]*\\]\\s*)?pub struct ${name}\\b`).test(rust);

describe('serde derives by schema role', () => {
  it('decodes what a call carries, and what no tool schema reaches, with every type they use', () => {
    expect([...decodedTypes(generated, roles)].sort()).toEqual(['Detail', 'QueryOnly', 'Shared', 'Standalone', 'ToolInput', 'ToolQuery']);
    // Without roles nothing narrows.
    expect(decodedTypes(generated, { inputs: [], outputs: [] }).size).toBe(9);
  });

  it('decodes an output type native names, and what it holds', () => {
    // Native saves rows and checks rows it builds against their generated types.
    expect([...decodedTypes(generated, roles, ['Row', 'NotGenerated'])].sort()).toEqual(
      ['Detail', 'QueryOnly', 'Row', 'RowText', 'Shared', 'Standalone', 'ToolInput', 'ToolQuery']
    );
  });

  it('keeps Serialize everywhere and drops Deserialize, derived or hand-written, from encode-only types', () => {
    const rust = encodeOnly(generated, decodedTypes(generated, roles));
    for (const name of ['ToolQuery', 'ToolInput', 'Shared', 'QueryOnly', 'Detail', 'Standalone'])
      expect(derivesDeserialize(rust, name), name).toBe(true);
    for (const name of ['ToolOutput', 'Row', 'RowText']) expect(derivesDeserialize(rust, name), name).toBe(false);
    expect(rust).not.toContain("impl<'de> ::serde::Deserialize<'de> for RowText");
    expect(rust.match(/:: serde :: Serialize/g)).toHaveLength(9);
    expect(rust).toContain('pub struct RowText(::std::string::String);');
    // Decoding everything changes nothing.
    expect(encodeOnly(generated, decodedTypes(generated, { inputs: [], outputs: [] }))).toBe(generated);
  });
});
