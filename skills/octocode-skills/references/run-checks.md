Load when running the structural checker on a skill or collection, or exercising its commands, standalone bundle, activation or size. These are steps 3 and 4 of [check skills](check-skills.md); finish with its [decision](check-skills.md#5-decide).

# Run checks and exercise behavior

## 3. Run structural checks

`<this-skill>` is the installed skill's absolute path; targets resolve from the working directory. Node is required.

```bash
node <this-skill>/scripts/skill-review.mjs <target> --json
node <this-skill>/scripts/skill-review.mjs --collections <collection> --json
node <this-skill>/scripts/skill-review.mjs <target> --commands --json   # also run each script's --help and check documented flags
node <this-skill>/scripts/skill-review.mjs <target> --tells --json      # also MUST/NEVER per file and sentences repeated across files
node <this-skill>/scripts/skill-review.mjs --self-test
```

Inspect `errorCount`, `warnCount` and each skill's `findings` (`level`, `code`, `message`; `link-escapes-skill` and the tell findings add `file` and `line`): errors exit 1; warnings alone exit 0. Resolve warnings or explain them. Explicit collections may be empty; missing roots and incomplete child skills still fail. Run `--self-test` after checker changes.

The checker covers common frontmatter, names, description limits, local paths/anchors, resource reachability, `SKILL.md`, `output.md` or a reference over 50 lines (`skill-length`, `doc-length`), a scratch path outside `<workspace>/.octocode/tmp/<skill-name>/` (`scratch-outside-skill-folder`), scripts without a documented command (`script-usage-missing`) or `--help` (`script-help-missing`), Markdown links that resolve outside the skill folder (`link-escapes-skill`), with `--commands` documented flags a script's `--help` does not list (`command-flag-unknown`), with `--tells` the two tells in [walk the route](check-skills.md#2-walk-the-route), and selected shared drift. It does not prove trigger quality, instruction correctness, script behavior, external URLs, or all YAML, imports and shared copies. Inline result contracts, conditional two-column tables and generic output destinations are valid; review their meaning against the actual task and consumer.

In the Octocode repo run `node skills/common/build.mjs --check`, `node skills-dev/octocode-dev/scripts/dev.mjs skills:check`, and `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify`. Repair drift at its owner, regenerate with `node skills/common/build.mjs`, then inspect the generated diff. Preserve concurrent work. Use the specification's [skills-ref validator](https://github.com/agentskills/agentskills/tree/main/skills-ref) for publication/advanced YAML when available; report if not run.

## 4. Exercise behavior and portability

- After the `--commands` run, exercise each changed command's normal and failure case in a disposable workspace within existing authorization. Links alone do not validate commands.
- Resolve each `link-escapes-skill` warning: bundle the file, link a public URL, or name the other skill in plain text. Then copy the skill without siblings/repository dependencies; check required imports and relevant helper help/self-tests, then walk the changed task. Do not mutate real provider installations to test packaging.
- For a substantial template change, have a fresh agent use it on a different task without the author's explanation. Check its first action, missing-input and failure behavior, resource choices and completion. A procedure, reference guide and branching tool task may need different shapes; do not grade them by matching headings or file counts.
- For activation, use fresh target-host sessions on the same requests before/after, holding model, tool policy, permissions and project constant. Record correct routes, misses, wrong routes and false triggers; repeat before claiming small gains. A description-only probe is not host activation. Use the `octocode-agentic` activation procedure if needed; respect existing cost/side-effect authorization.
- Measure discovery text, activated entry and selected-branch reads separately with `node <this-skill>/scripts/skill-review.mjs <target> --sizes --json` (UTF-8 bytes per stage), or a named tokenizer. Preserve safety and completion checks. Smaller text is not measured token, latency, comprehension or cost savings.
