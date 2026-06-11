# Large-File Minification Benchmark

> Generated 2026-06-11T17:22:55.186Z
>
> Each sample is a realistic ~400-line file taken from real open-source
> projects. All four minification modes are measured.

## Summary

| Metric | Value |
| --- | --- |
| Languages measured | 12 |
| Average content-view cut | **32.8%** |
| Average apply-minify cut | **37.4%** |
| Average async cut | **37.5%** |
| Average agent rating | **7.5/10** |
| excellent (≥9.0) | 4 |
| strong (≥8.0) | 0 |
| good (≥7.0) | 4 |
| fair (<7.0) | 4 |

## Per-language results

| Ext | Language | Lines | Bytes | Content-view | Apply | Async | Symbols | Rating |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `.js` | JavaScript | 419 | 10,668 | −54.1% | −61.9% | −61.9% | −91.3% | **10/10** excellent |
| `.ts` | TypeScript | 322 | 10,126 | −25.9% | −61.4% | −61.4% | −71.3% | **9.5/10** excellent |
| `.java` | Java | 394 | 14,819 | −54.7% | −54.7% | −54.7% | −83.3% | **9/10** excellent |
| `.rb` | Ruby | 201 | 6,781 | −55.4% | −55.4% | −55.4% | −92.8% | **9/10** excellent |
| `.go` | Go | 323 | 8,428 | −34.5% | −34.5% | −34.5% | −77.7% | **7.5/10** good |
| `.rs` | Rust | 325 | 9,961 | −33.2% | −33.2% | −33.2% | −80.8% | **7.5/10** good |
| `.kt` | Kotlin | 206 | 7,281 | −38.6% | −38.6% | −38.6% | −75.3% | **7.5/10** good |
| `.css` | CSS | 363 | 10,231 | −18.8% | −30.8% | −31.6% | −66.2% | **7/10** good |
| `.py` | Python | 341 | 10,724 | −16.7% | −16.7% | −16.7% | −60.2% | **6/10** fair |
| `.sql` | SQL | 261 | 8,970 | −18.6% | −18.6% | −18.6% | −42.2% | **6/10** fair |
| `.sh` | Shell | 294 | 9,146 | −21.5% | −21.5% | −21.5% | −91.9% | **6/10** fair |
| `.yml` | YAML | 312 | 8,974 | −21.9% | −21.9% | −21.9% | n/a | **5/10** fair |

## Analysis

### Best performers (≥9.0)

- **`.js`** JavaScript: content-view −54.1%, apply −61.9%, symbols −91.3%
- **`.ts`** TypeScript: content-view −25.9%, apply −61.4%, symbols −71.3%
- **`.java`** Java: content-view −54.7%, apply −54.7%, symbols −83.3%
- **`.rb`** Ruby: content-view −55.4%, apply −55.4%, symbols −92.8%

### Weakest performers (<7.5)

- **`.css`** CSS: content-view −18.8%, apply −30.8% — _good_
- **`.py`** Python: content-view −16.7%, apply −16.7% — _fair_
- **`.sql`** SQL: content-view −18.6%, apply −18.6% — _fair_
- **`.sh`** Shell: content-view −21.5%, apply −21.5% — _fair_
- **`.yml`** YAML: content-view −21.9%, apply −21.9% — _fair_

### Comment-density drivers

Languages where comment stripping contributes the most value tend to be
heavily-documented (JavaDoc, Rustdoc, Python docstrings). Formats like YAML
and shell provide less savings because real-world files carry fewer comments
relative to payload data.
