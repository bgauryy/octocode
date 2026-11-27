import { test } from 'node:test';
import assert from 'node:assert/strict';
import { headingSlug } from './lib/heading-slug.mjs';

test('heading slugs drop links, tags, code marks and punctuation, and turn spaces into hyphens', () => {
  for (const [heading, slug] of [
    ['Install the CLI', 'install-the-cli'],
    ['`octocode schema` and **bold** __x__', 'octocode-schema-and-bold-x'],
    ['See [docs](docs/README.md) now', 'see-docs-now'],
    ['<a id="x"></a>Anchored heading', 'anchored-heading'],
    ['Use Vec<Option<T>> here', 'use-vec-here'],
    ['a <<b>script> b', 'a-script-b'],
    ['x <> y', 'x--y'],
    ['Trailing hashes ##', 'trailing-hashes'],
    ['Émoji 🎉 test', 'émoji--test'],
    ['snake_case-and-dash', 'snake_case-and-dash'],
  ]) assert.equal(headingSlug(heading), slug, heading);
});
