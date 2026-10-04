1. **Helped:** The second Bash call did the work. `grep -n "self\._chain()\|^    def \|^class " query.py | grep -B1 "self\._chain()"` gave each call site with its nearest `def` and line number in one pass.

2. **Did not help:** The first call assumed `rg` and `git rev-parse` would work. `rg` isn't installed, and `/dev/null` writes were blocked, so that call produced only errors and the pinned commit stayed unverified. The `-B1` filter also showed only one line of context per hit, so my claim that no `class` line falls between the hits wasn't actually checked. I never opened a method body.

3. **Next time:** I'd check for `rg` first and use `grep` directly. I'd verify the commit through a path that doesn't need `/dev/null`, such as reading `.git/HEAD`. I'd also use `awk` to track the enclosing `def` and `class` for every hit, and spot-check a few bodies.

4. **Confidence:** Medium-high on the list of call sites and line numbers, which come straight from grep output. My final answer has an error: it said "Thirty methods", but the table lists 23 methods (24 call sites, since `__getitem__` has two). The table is right and the number was wrong. The commit pin also remains unverified.