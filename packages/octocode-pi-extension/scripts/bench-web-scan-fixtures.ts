export const adversarialPages = (): [string, string][] => [
  ['unclosed links', '<a href="x">t'.repeat(80_000)],
  ['an unclosed nav', `<nav>${'word '.repeat(200_000)}`],
  ['unclosed navs', '<nav>w'.repeat(170_000)],
  ['lone <', '<'.repeat(1_000_000)],
  ['one tag of opening links', `${'<a '.repeat(330_000)}>`],
  ['unclosed hidden elements', '<div hidden>'.repeat(85_000)],
  ['unclosed headings, list items and scripts', '<h1<li<script'.repeat(75_000)],
  ['unclosed quotes', '<a x="'.repeat(170_000)],
  ['unclosed comments and empty links', '<!--[]('.repeat(140_000)],
  ['tags searched for role=main', '<a'.repeat(500_000)],
];
