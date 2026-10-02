**1. Helped:** The single `rg -n "ResolveJsonModule" --glob '*.go'` call found the getter, every caller, the parser and the showConfig entry at once. The second call, a batch of `sed -n` reads at those line numbers, confirmed the default logic and each call site's behavior. Two calls were enough.

**2. Did not help:** Every Bash call printed `/dev/null: Operation not permitted`. The `git log -1 --format=%H` in my first call failed outright, so I never confirmed the checkout is actually at 4f5ddae. I assumed it matched the pinned commit, and my answer didn't say that. I also never opened the test files, so my comments about which baselines change are inferred from file names.

**3. Next time:** I'd confirm the commit with `cat .git/HEAD` or the packed refs, since the git command failed. I'd skim the two test files before naming them as affected. I'd also run one `rg` for `\.ResolveJsonModule\b` to back up my claim that nothing reads the raw field.

**4. Confidence:** High for the source-level claims, because I read each cited line directly. Medium for the test impact, which is inferred from file names, and for the commit pin, which is unverified.