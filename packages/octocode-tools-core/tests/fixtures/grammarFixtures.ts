// Each advertised extension must map to a real fixture. Adding a native grammar
// without extending this acceptance corpus is a failing release check.
export const grammarFixtures = [
  {
    extensions: ['ts', 'mts', 'cts'],
    source:
      'export function target(value: number): number {\n  const body_marker = value + 1;\n  return body_marker;\n}\n',
  },
  {
    extensions: ['tsx'],
    source:
      'export function target(value: number) {\n  const body_marker = value + 1;\n  return <div>{body_marker}</div>;\n}\n',
  },
  {
    extensions: ['js', 'jsx', 'mjs', 'cjs'],
    source:
      'export function target(value) {\n  const body_marker = value + 1;\n  return body_marker;\n}\n',
  },
  {
    extensions: ['py', 'pyi'],
    source:
      'def target(value):\n    body_marker = value + 1\n    return body_marker\n',
  },
  {
    extensions: ['go'],
    source:
      'package fixture\nfunc target(value int) int {\n  body_marker := value + 1\n  return body_marker\n}\n',
  },
  {
    extensions: ['rs'],
    source:
      'fn target(value: i32) -> i32 {\n  let body_marker = value + 1;\n  body_marker\n}\n',
  },
  {
    extensions: ['java'],
    source:
      'class Fixture {\n  int target(int value) {\n    int body_marker = value + 1;\n    return body_marker;\n  }\n}\n',
  },
  {
    extensions: ['c', 'h'],
    source:
      'int target(int value) {\n  int body_marker = value + 1;\n  return body_marker;\n}\n',
  },
  {
    extensions: ['cpp', 'hpp', 'cc', 'cxx', 'hh', 'hxx'],
    source:
      'class Fixture {\npublic:\n  int target(int value) {\n    int body_marker = value + 1;\n    return body_marker;\n  }\n};\n',
  },
  {
    extensions: ['cs'],
    source:
      'class Fixture {\n  public int target(int value) {\n    int body_marker = value + 1;\n    return body_marker;\n  }\n}\n',
  },
  {
    extensions: ['rb', 'rake', 'gemspec', 'ru'],
    source:
      'def target(value)\n  body_marker = value + 1\n  body_marker\nend\n',
  },
  {
    extensions: ['php'],
    source:
      '<?php\nfunction target($value) {\n  $body_marker = $value + 1;\n  return $body_marker;\n}\n',
  },
  {
    extensions: ['kt', 'kts'],
    source:
      'fun target(value: Int): Int {\n  val body_marker = value + 1\n  return body_marker\n}\n',
  },
  {
    extensions: ['lua'],
    source:
      'function target(value)\n  local body_marker = value + 1\n  return body_marker\nend\n',
  },
  {
    extensions: ['sql'],
    source: 'SELECT target FROM users WHERE active = true;\n',
  },
  {
    extensions: ['zig'],
    source:
      'fn target(value: i32) i32 {\n  const body_marker = value + 1;\n  return body_marker;\n}\n',
  },
  {
    extensions: ['html', 'htm'],
    source: '<div id="target"><span>value</span></div>\n',
  },
  { extensions: ['css'], source: '.target { color: red; }\n' },
  {
    extensions: ['scss'],
    source: '$color: red;\n.target { color: $color; }\n',
  },
  {
    extensions: ['scala', 'sc', 'sbt'],
    source:
      'object Fixture {\n  def target(value: Int): Int = {\n    val body_marker = value + 1\n    body_marker\n  }\n}\n',
  },
  { extensions: ['json', 'jsonc'], source: '{"target": true}\n' },
  { extensions: ['yaml', 'yml'], source: 'target: true\n' },
  { extensions: ['toml'], source: 'target = true\n' },
  {
    extensions: ['swift'],
    source:
      'func target(value: Int) -> Int {\n  let body_marker = value + 1\n  return body_marker\n}\n',
  },
];
