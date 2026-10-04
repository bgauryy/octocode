1. **Helped:** The second call, `gh api ... contents/prometheus/histogram.go?ref=v1.20.0 | base64 -d | grep -n`, found the key lines at the pinned tag (265, 393, 564). The third call, `sed -n '258,266p;384,398p;555,568p'`, then gave the surrounding doc comment and the `if len(h.upperBounds) == 0 && opts.NativeHistogramBucketFactor <= 1` condition.

2. **Did not help:** The first call was wasted. It used `rg`, which isn't installed, and a pointless `cd /tmp`. Every call also printed `/dev/null: Operation not permitted` noise from the sandbox. I never ran a call to confirm the repo name or that the `v1.20.0` tag exists. The repo name came from memory, and the tag was only implicitly confirmed because the content fetch succeeded.

3. **Next time:** I'd check for tools first, or just use `grep`. I'd print the file with `grep -n` or `nl` so every cited line has a number I actually saw.

4. **Confidence:** High on the substance, because it is read directly from the tagged source. Medium on the line ranges. Only 265, 393 and 564 were shown with numbers. I inferred 261-264, 389-397 and 563-565 from the `sed` offsets, so they may be off by a line or two.