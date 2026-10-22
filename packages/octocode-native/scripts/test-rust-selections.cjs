'use strict';

// The `test:rust` script in package.json is the one source of the Rust test
// selections (packages and features). Tools that must run the same selections
// read them here instead of keeping a copy.
//
// Runner decision (audit 2026-10-21, F2): `test:rust` and CI stay on nextest,
// and the local loop runs libtest through `cargo dev-test` (.cargo/config.toml),
// which carries the first selection below, so both share one build.
// - libtest runs a test binary in one process: about 65 s for the selection
//   against 266 s under nextest, whose 3,600 processes each repeat the
//   per-process start-up (the first GitHub client, contract parse, regex init).
// - nextest stays the gate: a process per test isolates process-global state
//   (environment, `GitHubBudget::global`, thread-locals) and kills a hung test
//   (`.config/nextest.toml` slow-timeout). In one shared process two
//   timing-sensitive tests failed in 1 of 2 libtest runs, so a libtest gate
//   would be flaky until those are fixed and the suite re-measured.

const RUN = 'cargo nextest run ';

/** The argument list of every `cargo nextest run` command in the `test:rust` script. */
function testRustSelections(pkg = require('../package.json')) {
  return pkg.scripts['test:rust'].split('&&').map(command => {
    const trimmed = command.trim();
    if (!trimmed.startsWith(RUN)) throw Error(`test:rust step is not "${RUN}…": ${trimmed}`);
    return trimmed.slice(RUN.length).split(/\s+/);
  });
}

module.exports = { testRustSelections };
