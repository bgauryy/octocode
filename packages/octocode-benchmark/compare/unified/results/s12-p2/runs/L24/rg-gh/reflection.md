1. **Helped:** The `grep -nE '\bgo (func|[a-zA-Z_.]+\()' *.go | grep -v _test.go` call found all `go` statements at once. The follow-up call with `sed -n` ranges plus `grep '\.Go('|errgroup` showed the surrounding code and caught the `errgroup` spawn in `Manager.Stop`, which a plain `go` search would miss.

2. **Did not help:**
   - `rg` was not installed, so my first search failed and I had to repeat it with grep.
   - Every Bash call printed `/dev/null: Operation not permitted`, and `git` failed, so I could not confirm the checkout was at `ea954809ce`. I said that in the answer.
   - I never found the name of the function containing `manager.go:434`, and I never read `scrapeLoop.run`. I gave my best guess (`ApplyConfig`) but should have run one grep for `^func` to settle it.
   - My answer's opening was sloppy. It said "five functions", then "six", and listed seven function entries in all. The count should have been checked before writing.
   - I inferred which function contains `scrape.go:399` and `scrape.go:554` from line ranges instead of checking.

3. **Next time:** Start with `grep -n '^func'` and the `go` search in the same call, so each hit maps to a function name immediately. Count the functions before writing the opening sentence.

4. **Confidence:** Medium-high on the list of goroutine sites, since the grep was exhaustive for `go` statements and `errgroup`. Medium on the function attributions for `manager.go:434`, `scrape.go:399` and `scrape.go:554`, and on the pinned commit.