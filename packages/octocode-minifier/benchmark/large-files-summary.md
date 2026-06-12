# Large-File Minification Benchmark

> Generated 2026-06-12T11:57:06.974Z
>
> Each sample is a realistic large file or real-source-derived stress fixture.
> All four minification modes are measured.

## Summary

| Metric | Value |
| --- | --- |
| Languages measured | 26 |
| Average content-view cut | **30.6%** |
| Average apply-minify cut | **35.5%** |
| Average async cut | **36.6%** |
| Average agent rating | **7.2/10** |
| excellent (≥9.0) | 6 |
| strong (≥8.0) | 3 |
| good (≥7.0) | 7 |
| fair (<7.0) | 10 |

## Per-language results

| Ext | Language | Lines | Bytes | Content-view | Apply | Async | Symbols | Rating |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `.js` | JavaScript | 419 | 10,670 | −54.1% | −61.9% | −61.9% | −91.3% | **10/10** excellent |
| `.ts` | TypeScript | 322 | 10,126 | −25.9% | −61.4% | −61.4% | −71.3% | **9.5/10** excellent |
| `.java` | Java | 394 | 14,819 | −54.7% | −54.7% | −54.7% | −83.3% | **9/10** excellent |
| `.rb` | Ruby | 201 | 6,781 | −55.4% | −55.4% | −55.4% | −92.8% | **9/10** excellent |
| `.yml` | YAML | 297 | 10,310 | −67.6% | −67.6% | −67.6% | n/a | **9/10** excellent |
| `.php` | PHP | 255 | 7,159 | −47.5% | −47.5% | −47.5% | −82.7% | **9/10** excellent |
| `.html` | HTML | 185 | 7,795 | −26.8% | −42.3% | −42.2% | −74.9% | **8.5/10** strong |
| `.scss` | SCSS | 291 | 7,034 | −28.2% | −40.6% | −70.4% | −55.7% | **8.5/10** strong |
| `.proto` | Protobuf | 198 | 6,808 | −56.6% | −56.6% | −56.6% | n/a | **8/10** strong |
| `.go` | Go | 323 | 8,428 | −34.5% | −34.5% | −34.5% | −77.7% | **7.5/10** good |
| `.rs` | Rust | 325 | 9,961 | −33.2% | −33.2% | −33.2% | −80.8% | **7.5/10** good |
| `.kt` | Kotlin | 206 | 7,281 | −38.6% | −38.6% | −38.6% | −75.3% | **7.5/10** good |
| `.cs` | C# | 226 | 8,485 | −33% | −33% | −33% | −81.9% | **7.5/10** good |
| `.bash` | Bash | 235 | 7,306 | −39.9% | −39.9% | −39.9% | −94.1% | **7.5/10** good |
| `.xml` | XML | 142 | 7,256 | −35.1% | −44.3% | −44.3% | n/a | **7.5/10** good |
| `.css` | CSS | 363 | 10,231 | −18.8% | −30.8% | −31.6% | −66.2% | **7/10** good |
| `.md` | Markdown | 243 | 6,143 | −27.4% | −27.4% | −27.4% | n/a | **6.5/10** fair |
| `.tsx` | TSX | 422 | 10,509 | −0% | −28.5% | −28.5% | −95.5% | **6/10** fair |
| `.py` | Python | 341 | 10,724 | −16.7% | −16.7% | −16.7% | −60.2% | **6/10** fair |
| `.c` | C | 852 | 29,047 | −15.1% | −15.1% | −15.1% | −86% | **6/10** fair |
| `.sql` | SQL | 261 | 8,970 | −18.6% | −18.6% | −18.6% | −42.2% | **6/10** fair |
| `.sh` | Shell | 294 | 9,146 | −21.5% | −21.5% | −21.5% | −91.9% | **6/10** fair |
| `.jsx` | JSX | 330 | 11,996 | −12% | −17.4% | −17.4% | −98.6% | **6/10** fair |
| `.graphql` | GraphQL | 249 | 6,652 | −18.1% | −18.1% | −18.1% | n/a | **5/10** fair |
| `.cpp` | C++ | 545 | 17,663 | −7.9% | −7.9% | −7.9% | −69.7% | **4.5/10** fair |
| `.vb` | Visual Basic | 539 | 21,461 | −8.7% | −8.7% | −8.7% | n/a | **3.5/10** fair |

## Analysis

### Best performers (≥9.0)

- **`.js`** JavaScript: content-view −54.1%, apply −61.9%, symbols −91.3%
- **`.ts`** TypeScript: content-view −25.9%, apply −61.4%, symbols −71.3%
- **`.java`** Java: content-view −54.7%, apply −54.7%, symbols −83.3%
- **`.rb`** Ruby: content-view −55.4%, apply −55.4%, symbols −92.8%
- **`.yml`** YAML: content-view −67.6%, apply −67.6%
- **`.php`** PHP: content-view −47.5%, apply −47.5%, symbols −82.7%

### Weakest performers (<7.5)

- **`.css`** CSS: content-view −18.8%, apply −30.8% — _good_
- **`.md`** Markdown: content-view −27.4%, apply −27.4% — _fair_
- **`.tsx`** TSX: content-view −0%, apply −28.5% — _fair_
- **`.py`** Python: content-view −16.7%, apply −16.7% — _fair_
- **`.c`** C: content-view −15.1%, apply −15.1% — _fair_
- **`.sql`** SQL: content-view −18.6%, apply −18.6% — _fair_
- **`.sh`** Shell: content-view −21.5%, apply −21.5% — _fair_
- **`.jsx`** JSX: content-view −12%, apply −17.4% — _fair_
- **`.graphql`** GraphQL: content-view −18.1%, apply −18.1% — _fair_
- **`.cpp`** C++: content-view −7.9%, apply −7.9% — _fair_
- **`.vb`** Visual Basic: content-view −8.7%, apply −8.7% — _fair_

### Comment-density drivers

Languages where comment stripping contributes the most value tend to be
heavily-documented (JavaDoc, Rustdoc, Python docstrings). Formats like YAML
and shell provide less savings because real-world files carry fewer comments
relative to payload data.
