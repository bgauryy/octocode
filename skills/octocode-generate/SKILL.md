---
name: octocode-generate
description: "Load this skill first when asked to scaffold, bootstrap or generate a new app, service, library or project. Interviews the user on what and how, writes a plan document for approval, then scaffolds with the framework CLI, applies tested lint/format/test/CI templates and proves every quality gate green."
---

# Octocode Generate

Interview, plan, approve, then build. Every choice that is costly to reverse cites evidence, and the result passes format, lint, typecheck, test and build. Scripts ([detect](scripts/detect-workspace.mjs), [versions](scripts/check-versions.mjs), [templates](scripts/apply-template.mjs), [verify](scripts/verify-project.mjs)) run with Node 20+ and print JSON; `--help` lists options.

1. **Detect.** `node <skill-folder>/scripts/detect-workspace.mjs <target>` reports existing languages, package manager, monorepo, tooling and installed toolchains. Existing conventions win unless the user overrides them.
2. **Interview.** Ask what and how in short batches with recommended defaults ([intake](references/intake.md)). Skip what the request or detection settles. Do not choose a stack before the what-batch is answered.
3. **Decide.** Choose the framework ([frameworks](references/frameworks.md)), package manager ([package managers](references/package-managers.md)), lint/test lane ([toolchain](references/toolchain.md)) and feature libraries ([libraries](references/libraries.md)). Keep two candidates for each costly choice, compare them on the deciding constraint, and research unsettled ones ([research](references/research.md)). For a contested choice, test the assumption it rests on ([mental models](references/mental-models.md)).
4. **Resolve versions.** `node <skill-folder>/scripts/check-versions.mjs <every planned package>` (`crates:` and `pypi:` prefixes for Rust and Python). Fix each conflict with its `fixPeer` or `fixPackage`, add `missingPeers`, and replace deprecated packages. Exit 0 means the set fits.
5. **Plan.** Write `<workspace>/.octocode/octocode-generate/<slug>/plan.md` from the [plan template](assets/plan.md): requirements with sources, decisions with rejected options, versions, risks, steps, gates. Show it and wait for approval. Framework CLIs and installs run third-party code and write files.
6. **Scaffold.** After approval: run the framework CLI non-interactively in the approved folder. Then `node <skill-folder>/scripts/apply-template.mjs <target> base <stack> <lint-lane> --pm <pm>` as a dry run, then with `--write`. It never overwrites; `--list` shows templates. Run its `install` commands with the resolved versions and implement each feature with its first test.
7. **Verify.** Run the format script once, then `node <skill-folder>/scripts/verify-project.mjs <target>`. Exit 0 means every gate passed. Fix `fail` and `missing` gates, then rerun; after two failed fixes of the same gate, ask the user. Start the dev server once for apps.

| Decision | Evidence required |
|---|---|
| Framework, auth, data store | 2+ sources or the user's explicit choice |
| Config pattern | 1 quality source, noted in the plan |
| Anything less certain | Ask the user |

Never commit, push or deploy unasked. Keep secrets in `.env` (ignored) and document them in `.env.example`.

Return the plan path, what was created, a gates table from `verify-project.mjs`, the evidence behind each decision, next steps (dev command, env to fill) and anything not run.
Done when the user approved the plan and `verify-project.mjs` exits 0, or the failing gate and its blocker are reported.
