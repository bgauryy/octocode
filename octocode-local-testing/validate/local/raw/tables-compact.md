
### localSearch vs rg

| task | expert shell command | shell chars / calls / ms / correct | octocode query | octocode chars / calls / ms / correct | verdict |
|---|---|---|---|---|---|
| ts-decl-regex | `rg -n -t ts '(const\|function) newElement\b' tsx` | 1710 / 1 / 23 / P1 R1 (17/17) | `{"searchText":"(const\|function) newElement\\b","langType":"t…` | 1817 / 1 / 265 / P1 R1 (17/17) | tie |
| ts-imports-l-F-glob | `rg -l -t ts -F 'from "@excalidraw/common"' -g '!**/*.test.*' …` | 11761 / 1 / 22 / P1 R1 (244/244) | `{"searchText":"from \"@excalidraw/common\"","regex":"literal"…` | 14591 / 1 / 406 / P1 R1 (244/244) | tie on correctness; shell reads 1.2× less |
| ts-context-C2 | `rg -n -C2 -F 'throw new Error(' tsx/packages/element/src/tran…` | 839 / 1 / 8 / P1 R1 (4/4) | `{"searchText":"throw new Error(","regex":"literal","contextLi…` | 1034 / 1 / 227 / P1 R1 (4/4) | tie on correctness; shell reads 1.2× less |
| rs-count-w | `rg -c -w unsafe -t rust rust/tokio/src` | 5467 / 1 / 19 / exact (1079 over 138 files) | `{"searchText":"unsafe","wholeWord":true,"langType":"rust","re…` | 7801 / 1 / 256 / exact (1079 over 138 files) | tie on correctness; shell reads 1.4× less |
| rs-multiline-U | `rg -U -n '#\[track_caller\]\s*\n\s*pub fn spawn' -t rust rust` | 6620 / 1 / 24 / P1 R1 (40/40) | `{"searchText":"#\\[track_caller\\]\\s*\\n\\s*pub fn spawn","m…` | 6247 / 1 / 391 / P1 R1 (40/40) | tie |
| rs-smartcase-S | `rg -S -c 'semaphore' rust/tokio/src/sync` | 670 / 1 / 13 / exact (578 over 17 files) | `{"searchText":"semaphore","caseMode":"smart","resultView":"co…` | 988 / 1 / 224 / exact (578 over 17 files) | tie on correctness; shell reads 1.5× less |
| go-l-F-notest | `rg -l -F 'errors.New("' -g '!*_test.go' go/tsdb` | 824 / 1 / 14 / P1 R1 (31/31) | `{"searchText":"errors.New(\"","regex":"literal","exclude":["*…` | 2094 / 1 / 417 / P1 R1 (31/31) | tie on correctness; shell reads 2.5× less |
| go-max-count | `rg -n -m1 'func \(h \*Head\) ' go/tsdb/head.go` | 48 / 1 / 8 / P1 R1 (1/1) | `{"searchText":"func \\(h \\*Head\\) ","maxMatchesPerFile":1}` | 1011 / 1 / 269 / P1 R1 (1/1) | tie on correctness; shell reads 21.1× less |
| go-w-t-all | `rg -n -w NewHead -t go go` | 8506 / 1 / 24 / P1 R1 (101/101) | `{"searchText":"NewHead","wholeWord":true,"langType":"go","max…` | 8955 / 1 / 287 / P1 R1 (101/101) | tie |
| py-w-scope | `rg -n -w get_object_or_404 -t py python/django` | 669 / 1 / 59 / P1 R1 (7/7) | `{"searchText":"get_object_or_404","wholeWord":true,"langType"…` | 790 / 1 / 286 / P1 R1 (7/7) | tie on correctness; shell reads 1.2× less |
| py-multiline-property | `rg -U -n '@property\s*\n\s*def \w+' python/django/db/models/f…` | 299 / 1 / 10 / P1 R1 (6/6) | `{"searchText":"@property\\s*\\n\\s*def \\w+","multiline":"on"…` | 569 / 1 / 230 / P1 R1 (6/6) | tie on correctness; shell reads 1.9× less |
| java-count-total | `rg -c -F '@CanIgnoreReturnValue' -t java java/guava/src` | 11944 / 1 / 23 / exact (842 over 186 files) | `{"searchText":"@CanIgnoreReturnValue","regex":"literal","lang…` | 15046 / 1 / 285 / exact (842 over 186 files) | tie on correctness; shell reads 1.3× less |
| java-l-F-meta | `rg -l -F 'checkNotNull(' java/guava/src/com/google/common/base` | 1336 / 1 / 12 / P1 R1 (25/25) | `{"searchText":"checkNotNull(","regex":"literal","resultView":…` | 1852 / 1 / 354 / P1 R1 (25/25) | tie on correctness; shell reads 1.4× less |
| c-def-anchor | `rg -n '^void \*zmalloc\(' c/src` | 49 / 1 / 21 / P1 R1 (1/1) | `{"searchText":"^void \\*zmalloc\\("}` | 202 / 1 / 246 / P1 R1 (1/1) | tie on correctness; shell reads 4.1× less |
| c-count-w | `rg -c -w zfree -g '*.c' c/src` | 1432 / 1 / 15 / exact (764 over 77 files) | `{"searchText":"zfree","wholeWord":true,"include":["*.c"],"res…` | 3474 / 1 / 263 / exact (764 over 77 files) | tie on correctness; shell reads 2.4× less |
| cpp-hot-file-all | `rg -n -F JSON_HEDLEY_ cpp/include` | 207573 / 1 / 13 / P1 R1 (1864/1864) | `{"searchText":"JSON_HEDLEY_","regex":"literal","maxMatchesPer…` | 164961 / 9 / 5033 / P1 R1 (1864/1864) | tie on correctness; octocode reads 1.3× less |
| cpp-count-only | `rg -c -F JSON_HEDLEY_ cpp/include` | 1028 / 1 / 13 / exact (1864 over 20 files) | `{"searchText":"JSON_HEDLEY_","regex":"literal","resultView":"…` | 1531 / 1 / 227 / exact (1864 over 20 files) | tie on correctness; shell reads 1.5× less |
| **total** | | 260775 / 17 / 321 | | 232963 / 25 / 9666 | |

### localFetch vs sed / rg -C / head / tail

| task | expert shell command | shell chars / calls / ms / correct | octocode query | octocode chars / calls / ms / correct | verdict |
|---|---|---|---|---|---|
| range-ts | `sed -n '174,230p' tsx/packages/element/src/newElement.ts` | 1875 / 1 / 3 / OK | `{"startLine":174,"endLine":230}` | 2201 / 1 / 202 / OK | tie on correctness; shell reads 1.2× less |
| range-rust | `sed -n '429,500p' rust/tokio/src/runtime/blocking/pool.rs` | 2933 / 1 / 3 / OK | `{"startLine":429,"endLine":500}` | 3230 / 1 / 202 / OK | tie on correctness; shell reads 1.1× less |
| range-go | `sed -n '1250,1300p' go/tsdb/head.go` | 1269 / 1 / 4 / OK | `{"startLine":1250,"endLine":1300}` | 1588 / 1 / 205 / OK | tie on correctness; shell reads 1.3× less |
| range-python | `sed -n '861,960p' python/django/db/models/query.py` | 4776 / 1 / 4 / OK | `{"startLine":861,"endLine":960}` | 5107 / 1 / 208 / OK | tie |
| range-java | `sed -n '731,745p' java/guava/src/com/google/common/collect/Li…` | 510 / 1 / 4 / OK | `{"startLine":731,"endLine":745}` | 763 / 1 / 203 / OK | tie on correctness; shell reads 1.5× less |
| range-c | `sed -n '4151,4250p' c/src/server.c` | 5046 / 1 / 4 / OK | `{"startLine":4151,"endLine":4250}` | 5370 / 1 / 220 / OK | tie |
| range-cpp | `sed -n '29863,29900p' cpp/single_include/nlohmann/json.hpp` | 2181 / 1 / 7 / OK | `{"startLine":29863,"endLine":29900}` | 2469 / 1 / 263 / OK | tie on correctness; shell reads 1.1× less |
| range-huge-ts | `sed -n '49064,49120p' typescript/tsc/testdata/fixtures/compil…` | 2610 / 1 / 10 / OK | `{"startLine":49064,"endLine":49120}` | 2990 / 1 / 352 / OK | tie on correctness; shell reads 1.1× less |
| match-ts | `rg -n -C5 -F "export const newTextElement" tsx/packages/eleme…` | 265 / 1 / 8 / OK | `{"matchString":"export const newTextElement","matchStringCase…` | 497 / 1 / 207 / OK | tie on correctness; shell reads 1.9× less |
| match-rust | `rg -n -C5 -F "fn spawn_thread" rust/tokio/src/runtime/blockin…` | 251 / 1 / 8 / OK | `{"matchString":"fn spawn_thread","matchStringCaseSensitive":t…` | 464 / 1 / 317 / OK | tie on correctness; shell reads 1.8× less |
| match-go | `rg -n -C5 -F "func (h *Head) gc()" go/tsdb/head.go` | 653 / 1 / 8 / OK | `{"matchString":"func (h *Head) gc()","matchStringCaseSensitiv…` | 861 / 1 / 230 / OK | tie on correctness; shell reads 1.3× less |
| match-python | `rg -n -C5 -F "def get_or_create" python/django/db/models/quer…` | 414 / 1 / 8 / OK | `{"matchString":"def get_or_create","matchStringCaseSensitive"…` | 619 / 1 / 231 / OK | tie on correctness; shell reads 1.5× less |
| match-java | `rg -n -C5 -F "public static <E extends @Nullable Object> Arra…` | 496 / 1 / 8 / OK | `{"matchString":"public static <E extends @Nullable Object> Ar…` | 736 / 1 / 213 / OK | tie on correctness; shell reads 1.5× less |
| match-c | `rg -n -C5 -F "int processCommand(client *c)" c/src/server.c` | 584 / 1 / 8 / OK | `{"matchString":"int processCommand(client *c)","matchStringCa…` | 786 / 1 / 298 / OK | tie on correctness; shell reads 1.3× less |
| match-cpp | `rg -n -C5 -F "class lexer : public lexer_base" cpp/single_inc…` | 529 / 1 / 8 / OK | `{"matchString":"class lexer : public lexer_base","matchString…` | 755 / 1 / 537 / OK | tie on correctness; shell reads 1.4× less |
| match-huge-ts | `rg -n -C5 -F "function checkSourceElementWorker" typescript/t…` | 400 / 1 / 9 / OK | `{"matchString":"function checkSourceElementWorker","matchStri…` | 645 / 1 / 1112 / OK | tie on correctness; shell reads 1.6× less |
| head-ts | `head -n 40 tsx/packages/element/src/newElement.ts` | 1010 / 1 / 3 / OK | `{"startLine":1,"endLine":40}` | 1311 / 1 / 204 / OK | tie on correctness; shell reads 1.3× less |
| head-go | `head -n 40 go/tsdb/head.go` | 1117 / 1 / 3 / OK | `{"startLine":1,"endLine":40}` | 1447 / 1 / 206 / OK | tie on correctness; shell reads 1.3× less |
| head-c | `head -n 40 c/src/server.c` | 1016 / 1 / 3 / OK | `{"startLine":1,"endLine":40}` | 1317 / 1 / 218 / OK | tie on correctness; shell reads 1.3× less |
| tail-rust | `tail -n 30 rust/tokio/src/runtime/blocking/pool.rs` | 728 / 1 / 3 / OK | `{"startLine":749,"endLine":778}` | 1247 / 2 / 505 / OK | tie on correctness; shell reads 1.7× less |
| tail-python | `tail -n 30 python/django/db/models/query.py` | 1086 / 1 / 3 / OK | `{"startLine":3108,"endLine":3137}` | 1562 / 2 / 408 / OK | tie on correctness; shell reads 1.4× less |
| tail-java | `tail -n 30 java/guava/src/com/google/common/collect/Lists.java` | 712 / 1 / 3 / OK | `{"startLine":1216,"endLine":1245}` | 1220 / 2 / 403 / OK | tie on correctness; shell reads 1.7× less |
| outline-ts | `rg -n '^export (const\|function\|type\|interface) \w+' tsx/pa…` | 799 / 1 / 10 / OK | `{"minify":"symbols"}` | 8325 / 1 / 217 / OK | tie on correctness; shell reads 10.4× less |
| outline-go | `rg -n '^func ' go/tsdb/head.go` | 10365 / 1 / 8 / OK | `{"minify":"symbols"}` | 17560 / 1 / 259 / OK | tie on correctness; shell reads 1.7× less |
| outline-python | `rg -n '^\s*(class\|def) \w+' python/django/db/models/query.py` | 6956 / 1 / 10 / OK | `{"minify":"symbols"}` | 15505 / 1 / 230 / OK | tie on correctness; shell reads 2.2× less |
| **total** | | 48581 / 25 / 150 | | 78575 / 28 / 7650 | |

### structureSearch vs git ls-files / find / wc -l

| task | expert shell command | shell chars / calls / ms / correct | octocode query | octocode chars / calls / ms / correct | verdict |
|---|---|---|---|---|---|
| tree-ts | `cd tsx && git -c core.quotepath=off ls-files \| awk -F/ '{pri…` | 2801 / 1 / 10 / P1 R1 (/129) | `{"operation":"tree","maxDepth":1,"hidden":true,"pageSize":100}` | 4600 / 2 / 488 / P1 R0.969 (/129) | **shell** (correctness) |
| tree-rust | `cd rust && git -c core.quotepath=off ls-files \| awk -F/ '{pr…` | 2324 / 1 / 9 / P1 R1 (/118) | `{"operation":"tree","maxDepth":1,"hidden":true,"pageSize":100}` | 4127 / 2 / 509 / P0.992 R1 (/118) | **shell** (correctness) |
| tree-go | `cd go && git -c core.quotepath=off ls-files \| awk -F/ '{prin…` | 6960 / 1 / 10 / P1 R1 (/371) | `{"operation":"tree","maxDepth":1,"hidden":true,"pageSize":100}` | 11821 / 4 / 1057 / P1 R1 (/371) | tie on correctness; shell reads 1.7× less |
| tree-python | `cd python && git -c core.quotepath=off ls-files \| awk -F/ '{…` | 5807 / 1 / 19 / P1 R1 (/311) | `{"operation":"tree","maxDepth":1,"hidden":true,"pageSize":100}` | 9147 / 4 / 1031 / P1 R1 (/311) | tie on correctness; shell reads 1.6× less |
| tree-java | `cd java && git -c core.quotepath=off ls-files \| awk -F/ '{pr…` | 1116 / 1 / 14 / P1 R1 (/65) | `{"operation":"tree","maxDepth":1,"hidden":true,"pageSize":100}` | 1707 / 1 / 226 / P1 R1 (/65) | tie on correctness; shell reads 1.5× less |
| tree-c | `cd c && git -c core.quotepath=off ls-files \| awk -F/ '{print…` | 5163 / 1 / 10 / P1 R1 (/329) | `{"operation":"tree","maxDepth":1,"hidden":true,"pageSize":100}` | 10274 / 4 / 1070 / P1 R1 (/329) | tie on correctness; shell reads 2.0× less |
| names-ts | `find tsx/packages -name '*.test.tsx' -type f` | 4073 / 1 / 8 / P1 R1 (/80) | `{"operation":"files","names":["*.test.tsx"],"entryType":"f","…` | 6726 / 1 / 276 / P1 R1 (/80) | tie on correctness; shell reads 1.7× less |
| names-rust | `find rust/tokio/tests -name 'sync_*.rs' -type f` | 660 / 1 / 4 / P1 R1 (/19) | `{"operation":"files","names":["sync_*.rs"],"entryType":"f","p…` | 1257 / 1 / 235 / P1 R1 (/19) | tie on correctness; shell reads 1.9× less |
| names-go | `find go/tsdb -name '*_test.go' -type f` | 1757 / 1 / 4 / P1 R1 (/57) | `{"operation":"files","names":["*_test.go"],"entryType":"f","p…` | 3744 / 1 / 240 / P1 R1 (/57) | tie on correctness; shell reads 2.1× less |
| names-python | `find python/django/contrib -name 'models.py' -type f` | 506 / 1 / 101 / P1 R1 (/11) | `{"operation":"files","names":["models.py"],"entryType":"f","p…` | 880 / 1 / 430 / P1 R1 (/11) | tie on correctness; shell reads 1.7× less |
| names-java | `find java/guava/src -name '*Builder.java' -type f` | 501 / 1 / 5 / P1 R1 (/8) | `{"operation":"files","names":["*Builder.java"],"entryType":"f…` | 830 / 1 / 250 / P1 R1 (/8) | tie on correctness; shell reads 1.7× less |
| names-cpp | `find cpp/tests/src -name 'unit-*.cpp' -type f` | 2850 / 1 / 4 / P1 R1 (/80) | `{"operation":"files","names":["unit-*.cpp"],"entryType":"f","…` | 5035 / 1 / 255 / P1 R1 (/80) | tie on correctness; shell reads 1.8× less |
| largest-ts | `cd tsx && git ls-files -z -- 'packages/**/*.ts' 'packages/*.t…` | 484 / 1 / 19 / exact order+counts | `{"operation":"files","extensions":["ts"],"entryType":"f","det…` | 1679 / 1 / 367 / exact order+counts | tie on correctness; shell reads 3.5× less |
| largest-rust | `cd rust && git ls-files -z -- 'tokio/src/**/*.rs' 'tokio/src/…` | 376 / 1 / 19 / exact order+counts | `{"operation":"files","extensions":["rs"],"entryType":"f","det…` | 1514 / 1 / 332 / exact order+counts | tie on correctness; shell reads 4.0× less |
| largest-go | `cd go && git ls-files -z -- 'tsdb/**/*.go' 'tsdb/*.go' \| xar…` | 281 / 1 / 15 / exact order+counts | `{"operation":"files","extensions":["go"],"entryType":"f","det…` | 1474 / 1 / 314 / exact order+counts | tie on correctness; shell reads 5.2× less |
| largest-python | `cd python && git ls-files -z -- 'django/**/*.py' 'django/*.py…` | 410 / 1 / 30 / exact order+counts | `{"operation":"files","extensions":["py"],"entryType":"f","det…` | 1611 / 1 / 613 / exact order+counts | tie on correctness; shell reads 3.9× less |
| largest-java | `cd java && git ls-files -z -- 'guava/src/**/*.java' 'guava/sr…` | 629 / 1 / 26 / exact order+counts | `{"operation":"files","extensions":["java"],"entryType":"f","d…` | 1772 / 1 / 388 / exact order+counts | tie on correctness; shell reads 2.8× less |
| largest-c | `cd c && git ls-files -z -- 'src/**/*.c' 'src/*.c' \| xargs -0…` | 241 / 1 / 19 / exact order+counts | `{"operation":"files","extensions":["c"],"entryType":"f","deta…` | 1432 / 1 / 350 / exact order+counts | tie on correctness; shell reads 5.9× less |
| **total** | | 36939 / 18 / 326 | | 69630 / 29 / 8431 | |

Plain `find` baseline for the same trees (no gitignore awareness):

| task | find chars | find correct |
|---|---|---|
| tree-find-ts | 2801 | P1 R1 (/129) |
| tree-find-rust | 2399 | P0.959 R1 (/118) |
| tree-find-go | 6960 | P1 R1 (/371) |
| tree-find-python | 5807 | P1 R1 (/311) |
| tree-find-java | 1116 | P1 R1 (/65) |
| tree-find-c | 5163 | P1 R1 (/329) |

### astSearch match vs ast-grep CLI

| task | expert shell command | shell chars / calls / ms / correct | octocode query | octocode chars / calls / ms / correct | verdict |
|---|---|---|---|---|---|
| match-rust | `ast-grep run -p '$X.unwrap()' -l rust rust/tokio/src/sync` | 10213 / 1 / 19 / reference (truth source); plain output prints every line of… | `{"operation":"match","langType":"rust","pattern":"$X.unwrap()…` | 18403 / 1 / 439 / P1 R1 (95/95) | tie on correctness; shell reads 1.8× less |
| match-ts | `ast-grep run -p 'new Error($MSG)' -l typescript tsx/packages/…` | 2783 / 1 / 28 / reference (truth source); plain output prints every line of… | `{"operation":"match","langType":"typescript","pattern":"new E…` | 3515 / 1 / 326 / P1 R0.684 (13/19) | **shell** (octocode misses matches) |
| match-go | `ast-grep run -p 'errors.New($S)' -l go go/tsdb  ⟶ 0 hits, the…` | 10092 / 2 / 233 / reference (truth source); first bare pattern misparsed → 0 … | `{"operation":"match","langType":"go","pattern":"errors.New($S…` | 26937 / 2 / 1108 / P1 R1 (107/107) | octocode: right on first try (CLI needed a rule) |
| match-python | `ast-grep run -p '$M.objects.filter($$$A)' -l python python/dj…` | 1562 / 1 / 43 / reference (truth source); plain output prints every line of… | `{"operation":"match","langType":"python","pattern":"$M.object…` | 3869 / 1 / 584 / P1 R1 (10/10) | tie on correctness; shell reads 2.5× less |
| match-java | `ast-grep run -p 'checkNotNull($X)' -l java java/guava/src/com…` | 9059 / 1 / 14 / reference (truth source); plain output prints every line of… | `{"operation":"match","langType":"java","pattern":"checkNotNul…` | 16493 / 1 / 342 / P1 R1 (94/94) | tie on correctness; shell reads 1.8× less |
| match-c | `ast-grep run -p 'zfree($X)' -l c c/src  ⟶ 0 hits, then: ast-g…` | 36568 / 2 / 275 / reference (truth source); first bare pattern misparsed → 0 … | `{"operation":"match","langType":"c","pattern":"zfree($X)","pa…` | 122307 / 7 / 5186 / P1 R1 (745/745) | octocode: right on first try (CLI needed a rule) |
| match-cpp | `ast-grep run -p 'JSON_THROW($E)' -l cpp cpp/include` | 25381 / 1 / 46 / reference (truth source); plain output prints every line of… | `{"operation":"match","langType":"cpp","pattern":"JSON_THROW($…` | 54831 / 4 / 1949 / P1 R1 (155/155) | tie on correctness; shell reads 2.2× less |
| **total** | | 95658 / 9 / 658 | | 246355 / 17 / 9934 | |

rg text approximation of the same structural queries:

| task | rg command | chars | correct vs structural truth |
|---|---|---|---|
| match-rust | `rg -n -F '.unwrap()' -t rust rust/tokio/src/sync` | 29349 | P0.258 R0.958 (91/95) |
| match-ts | `rg -n 'new Error\(' -g '*.ts' tsx/packages/element/src` | 1859 | P0.905 R1 (19/19) |
| match-go | `rg -n -F 'errors.New(' -t go go/tsdb` | 9985 | P1 R1 (107/107) |
| match-python | `rg -n '\w+\.objects\.filter\(' -t py python/django/contrib` | 905 | P1 R0.8 (8/10) |
| match-java | `rg -n -w 'checkNotNull\([^,()]*\)' -t java java/guava/src/com/google/common/base` | 9854 | P0.91 R0.968 (91/94) |
| match-c | `rg -n '\bzfree\(' -g '*.c' c/src` | 35706 | P0.995 R1 (745/745) |
| match-cpp | `rg -n -F 'JSON_THROW(' cpp/include` | 25042 | P0.975 R1 (155/155) |

### astSearch symbols vs rg declaration regex / ctags

| task | expert shell command | shell chars / calls / ms / correct | octocode query | octocode chars / calls / ms / correct | verdict |
|---|---|---|---|---|---|
| symbols-go | `rg -n '^func ' go/tsdb/head.go` | 10365 / 1 / 9 / P1 R1 (139/139) | `{"operation":"symbols","pageSize":500}` | 14881 / 1 / 399 / P1 R1 (139/139) | tie on correctness; shell reads 1.4× less |
| symbols-rust | `rg -n '^\s*(pub(\([\w:]+\))? )?(const )?(async )?(unsafe )?fn…` | 2264 / 1 / 10 / P1 R1 (41/41) | `{"operation":"symbols","pageSize":500}` | 6485 / 1 / 389 / P1 R1 (41/41) | tie on correctness; shell reads 2.9× less |
| symbols-python | `rg -n '^\s*(async )?(def\|class) \w+' python/django/db/models…` | 8069 / 1 / 10 / P1 R1 (188/188) | `{"operation":"symbols","pageSize":500}` | 17188 / 1 / 407 / P1 R1 (188/188) | tie on correctness; shell reads 2.1× less |
| symbols-java | `rg -n '^\s*(public\|protected\|private\|static\|final\|abstra…` | 7547 / 1 / 11 / P0.806 R1 (108/108) | `{"operation":"symbols","pageSize":500}` | 14057 / 1 / 310 / P1 R1 (108/108) | **octocode** (correctness) |
| symbols-ts | `rg -n '^\s*(export )?(async )?(function \w+\|const \w+ = (asy…` | 1041 / 1 / 10 / P0.826 R0.95 (19/20) | `{"operation":"symbols","pageSize":500}` | 2105 / 1 / 248 / P1 R1 (20/20) | **octocode** (correctness) |
| symbols-c | `ctags -x c/src/zmalloc.c` | 7967 / 1 / 9 / P0.981 R0.815 (53/65) | `{"operation":"symbols","pageSize":500}` | 9472 / 1 / 309 / P1 R1 (65/65) | **octocode** (correctness) |
| **total** | | 37253 / 6 / 59 | | 64188 / 6 / 2062 | |

### lspSearch vs rg -w

| task | expert shell command | shell chars / calls / ms / correct | octocode query | octocode chars / calls / ms / correct | verdict |
|---|---|---|---|---|---|
| refs-rust | `rg -n -w add_permits -t rust rust` | 2826 / 1 / 24 / P0.621 R1 (18/18) | `{"operation":"references","symbolName":"add_permits","lineHin…` | 3146 / 1 / 6586 / P1 R1 (18/18) | **octocode** (correctness) |
| refs-ts | `rg -n -w getNonDeletedElements tsx/packages/element/src` | 2773 / 1 / 14 / P0.259 R1 (7/7) | `{"operation":"references","symbolName":"getNonDeletedElements…` | 12226 / 1 / 5101 / P1 R1 (7/7) | **octocode** (correctness) |
| refs-python | `rg -n -w slugify -t py python` | 1369 / 1 / 84 / P0.308 R0.8 (4/5) | `{"operation":"references","symbolName":"slugify","lineHint":4…` | 1477 / 1 / 3202 / P1 R1 (5/5) | **octocode** (correctness) |
| refs-c | `rg -n -w zmalloc_used_memory -g '*.c' -g '*.h' c/src` | 2552 / 1 / 15 / P0.9 R1 (27/27) | `{"operation":"references","symbolName":"zmalloc_used_memory",…` | 596 / 1 / 2791 / P1 R0.074 (2/27) | **shell** (correctness) |
| refs-go | `rg -n -w NewHead -t go go` | 8506 / 1 / 23 / 101 hits (no truth) | `{"operation":"references","symbolName":"NewHead","lineHint":2…` | 729 / 1 / 324 / error: lsp.serverUnavailable | tie on correctness; octocode reads 11.7× less |
| refs-java | `rg -n -w 'partition' -t java java/guava/src` | 2936 / 1 / 23 / 21 hits (no truth) | `{"operation":"references","symbolName":"partition","lineHint"…` | 803 / 1 / 277 / error: lsp.serverUnavailable | tie on correctness; octocode reads 3.7× less |
| **total** | | 20962 / 6 / 183 | | 18977 / 6 / 18281 | |

### lspSearch definition vs rg candidate hunt

| task | expert shell command | shell chars / calls / ms / correct | octocode query | octocode chars / calls / ms / correct | verdict |
|---|---|---|---|---|---|
| def-ts | `rg -n '(const\|function) getNonDeletedElements\b' -t ts tsx/p…` | 330 / 2 / 22 / shell yields candidate list; agent must disambiguate by rea… | `{"operation":"definition","symbolName":"getNonDeletedElements…` | 542 / 1 / 3423 / OK | octocode resolves exactly; shell returns candidates |
| def-python | `rg -n -w '_slugify' python/django/template/defaultfilters.py …` | 129 / 2 / 16 / shell yields candidate list; agent must disambiguate by rea… | `{"operation":"definition","symbolName":"_slugify","lineHint":…` | 418 / 1 / 2810 / OK | octocode resolves exactly; shell returns candidates |
| def-rust | `rg -n 'fn add_permits' -t rust rust/tokio/src  ;  rg -n 'use …` | 1136 / 4 / 43 / shell yields candidate list; agent must disambiguate by rea… | `{"operation":"definition","symbolName":"add_permits","lineHin…` | 450 / 1 / 5878 / OK | octocode resolves exactly; shell returns candidates |
| **total** | | 1595 / 8 / 81 | | 1410 / 3 / 12111 | |

### astTopology dependents vs rg import grep

| task | expert shell command | shell chars / calls / ms / correct | octocode query | octocode chars / calls / ms / correct | verdict |
|---|---|---|---|---|---|
| topo-ts | `rg -l -t ts "(from\|import)\s*\(?\s*['\"](\./\|\.\./)+mutateE…` | 605 / 1 / 13 / P1 R1 (15/15) | `{"analysis":"dependents","pageSize":100}` | 2810 / 1 / 335 / P1 R1 (15/15) | tie on correctness; shell reads 4.6× less |
| topo-python | `rg -l -t py -e 'from django\.utils\.text import' -e 'import d…` | 966 / 1 / 98 / P0.958 R1 (23/23) | `{"analysis":"dependents","pageSize":100}` | 5737 / 1 / 3509 / P1 R1 (23/23) | **octocode** (correctness) |
| topo-python-subroot | `rg -l -t py -e 'from django\.utils\.text import' -e 'import d…` | 852 / 1 / 58 / P0.952 R1 (20/20) | `{"analysis":"dependents","pageSize":100}` | 985 / 1 / 1388 / P0 R0 (0/20) | **shell** (correctness) |
| topo-go | `rg -l -F '"github.com/prometheus/prometheus/tsdb/chunkenc"' -…` | 1766 / 1 / 25 / P1 R1 (67/67) | `{"analysis":"dependents","pageSize":100}` | 13482 / 1 / 1983 / P1 R1 (67/67) | tie on correctness; shell reads 7.6× less |
| topo-go-subroot | `rg -l -F '"github.com/prometheus/prometheus/tsdb/chunkenc"' -…` | 822 / 1 / 15 / P1 R1 (33/33) | `{"analysis":"dependents","pageSize":100}` | 591 / 1 / 990 / P0 R0 (0/33) | **shell** (correctness) |
| topo-c | `rg -l '^\s*#\s*include\s+"zmalloc\.h"' c/src` | 401 / 1 / 21 / P1 R1 (24/24) | `{"analysis":"dependents","pageSize":100}` | 7658 / 1 / 912 / P1 R1 (24/24) | tie on correctness; shell reads 19.1× less |
| topo-java | `rg -l 'import (static )?com\.google\.common\.collect\.Lists[.…` | 2382 / 2 / 38 / P1 R1 (23/23) | `{"analysis":"dependents","pageSize":100}` | 5244 / 1 / 757 / P1 R0.957 (22/23) | **shell** (correctness) |
| **total** | | 7794 / 8 / 268 | | 36507 / 7 / 9874 | |

### astRewrite vs ast-grep --rewrite vs sed

| task | ast-grep (preview+apply) chars / calls / ms / changed lines | sed chars / calls / changed lines / diff vs ast-grep / parse errors after | octocode (preview pages + apply) chars / calls / ms / matches / changed lines / diff vs ast-grep | verdict |
|---|---|---|---|---|
| Rewrite $X.unwrap() → $X.expect("checked") in a copy of rust/tokio/sr… | 25770 / 2 / 49 / 97 | 0 / 1 / 353 / 18 files, 216 hunks / 0 (before 0) | 142249 / 3 / 2333 / 95 / 97 / 0 files, 0 hunks | octocode = ast-grep result; sed wrong |
| Rewrite new Error($MSG) → new AppError($MSG) in a copy of tsx/package… | 8395 / 2 / 66 / 19 | 0 / 1 / 21 / 4 files, 8 hunks / 0 (before 0) | 37147 / 2 / 1394 / 19 / 19 / 0 files, 0 hunks | octocode = ast-grep result; sed wrong |
| Rewrite new Error($$$A) → new AppError($$$A) in a copy of tsx/package… | 9006 / 2 / 60 / 21 | 0 / 1 / 21 / 4 files, 6 hunks / 0 (before 0) | 39399 / 2 / 1492 / 21 / 21 / 0 files, 0 hunks | octocode = ast-grep result; sed wrong |
| Rewrite require.NoError(t, $E) → require.NoError(t, $E, "chunkenc") i… | 69205 / 2 / 76 / 233 | 0 / 1 / 233 / 0 files, 0 hunks / 0 (before 0) | 335990 / 3 / 2197 / 233 / 233 / 0 files, 0 hunks | octocode = ast-grep result; sed same |
| Rewrite self.assertEqual(len($A), 0) → self.assertLen($A, 0) in a cop… | 13787 / 2 / 145 / 35 | 0 / 1 / 17 / 1 files, 13 hunks / 0 (before 0) | 53062 / 2 / 2087 / 27 / 35 / 0 files, 0 hunks | octocode = ast-grep result; sed wrong |

### Edge cases

| id | scenario | shell (cmd → chars, exit) | octocode (chars, exit) | check |
|---|---|---|---|---|
| huge-search-one | Find one declaration in 3.2MB checker.ts | `rg -n 'function checkSourceElementWorker' /Users/bgaryy/code/octocode…` → 65c, exit 0 | 270c, exit 0 | {"bothLine49064":true} |
| huge-search-many | Frequent identifier getTypeOfSymbol in checker.ts (default output) | `rg -n -w getTypeOfSymbol /Users/bgaryy/code/octocode/octocode-local-t…` → 16530c, exit 0 | 2741c, exit 6 | {"rgLines":166,"ocReportsTotal":"166"} |
| huge-fetch-full | Read whole 3.2MB checker.ts (cat vs fullContent) | `cat /Users/bgaryy/code/octocode/octocode-local-testing/repos/typescri…` → 3151772c, exit 0 | 18355c, exit 6 | {"catChars":3151772,"ocMentionsClip":true} |
| huge-symbols | Declarations in checker.ts (astSearch symbols vs rg decl regex) | `rg -c '^\s*function \w+' /Users/bgaryy/code/octocode/octocode-local-t…` → 5c, exit 0 | 1374c, exit 0 | {"rgFunctionLines":2446,"ocTotal":"12"} |
| huge-structure | Find files >1MB under typescript/tsc/testdata (find -size vs structur… | `find typescript/tsc/testdata -type f -size +1024k` → 721c, exit 0 | 1306c, exit 0 | {"findCount":8,"ocFiles":8} |
| minified-rg-naive | Search in 2MB single-line minified JS (naive rg prints the whole line) | `rg -n 'function f12345\b' src/vendor.min.js` → 2077782c, exit 0 | 710c, exit 0 | {"rgChars":2077782,"ocChars":710,"ocHasHit":true} |
| minified-rg-expert | Same, expert rg -o with a ±60 char window | `rg -n -o '.{0,60}function f12345\b.{0,60}' src/vendor.min.js` → 138c, exit 0 | 361c, exit 0 | {"rgChars":138,"ocChars":361} |
| minified-fetch | Read around a match in minified JS (grep -o window vs localFetch cont… | `grep -o '.\{0,80\}function f12345(.\{0,80\}' src/vendor.min.js` → 177c, exit 0 | 426c, exit 0 | {"ocChars":426,"ocHasHit":true} |
| binary-search | Text search hitting a binary file | `rg -n NEEDLE_BIN src` → 0c, exit 1 | 223c, exit 1 | {"rgSays":"","ocMentionsBinary":false} |
| binary-fetch | Read a binary file | `cat -v src/blob.bin` → 30c, exit 0 | 434c, exit 5 | {"ocRefusesOrFlags":true} |
| latin1-search | Search a latin-1 (non-UTF8) file | `rg -n NEEDLE_LATIN1 src/latin1.txt` → 27c, exit 0 | 229c, exit 0 | {"ocHit":true,"ocFlagsEncoding":true} |
| latin1-search-accent | Search "café" (UTF-8 query) in a latin-1 file | `rg -n 'café' src/latin1.txt; rg -n -E latin1 'café' src/latin1.txt` → 46c, exit 0 | 223c, exit 1 | {"rgEncodingFlagFinds":true,"ocFinds":false} |
| latin1-fetch | Read a latin-1 file | `cat src/latin1.txt` → 42c, exit 0 | 438c, exit 5 | {"ocHasReplacementChar":false,"ocFlagsEncoding":true} |
| symlink-search | Search through symlinks pointing outside root (/etc/hosts, ../lib.mjs) | `rg -n 'localhost\|ocAll' src; echo '--- with -L:'; rg -L -l 'localhos…` → 102c, exit 2 | 223c, exit 1 | {"ocLeaksHosts":false,"ocLeaksOutside":false} |
| symlink-fetch | Read a symlink that points outside the root | `cat src/link_hosts \| head -3` → 21c, exit 0 | 531c, exit 5 | {"ocRefused":true} |
| symlink-fetch-rel | Read a relative symlink escaping the root | `head -3 src/link_outside` → 168c, exit 0 | 595c, exit 5 | {"ocRefused":true} |
| symlink-tree | Tree listing containing symlinks | `find src -maxdepth 1 \| sort` → 147c, exit 0 | 349c, exit 0 | {"ocListsLinks":false,"ocDescendsLinkDir":false} |
| sandbox-abs | Absolute path outside the workspace root | `head -2 /etc/hosts` → 19c, exit 0 | 431c, exit 5 | {"ocRefused":true} |
| sandbox-dotdot | Path traversal with .. | `head -2 src/../../../../lib.mjs` → 120c, exit 0 | 569c, exit 5 | {"ocRefused":true} |
| ignore-default | Default search: gitignored dir, node_modules, target, dist, .env, *.l… | `rg -l NEEDLE_ .` → 144c, exit 0 | 267c, exit 0 | {"rg":["./dist/bundle.js","./node_modules/pkg/index.js","./src/config.ts","./src/crlf.txt","./src/emoji.ts","./src/latin1.txt","./src/main.ts","./target/debug/… |
| ignore-defaultExcludes-false | defaultExcludes:false vs rg (dependency/build dirs) | `rg -l NEEDLE_ .` → 144c, exit 0 | 361c, exit 0 | {"oc":["dist/bundle.js","node_modules/pkg/index.js","src/config.ts","src/crlf.txt","src/emoji.ts","src/latin1.txt","src/main.ts","target/debug/out.rs"]} |
| ignore-noIgnore-hidden | noIgnore+hidden+defaultExcludes:false vs rg --no-ignore --hidden | `rg -l --no-ignore --hidden NEEDLE_ .` → 181c, exit 0 | 409c, exit 0 | {"rg":["./.env","./app.log","./dist/bundle.js","./ignored/notes.txt","./node_modules/pkg/index.js","./src/config.ts","./src/crlf.txt","./src/emoji.ts","./src/l… |
| ignore-real-target | Real repo: rust/target (81MB, gitignored) — files mentioning tokio | `rg -l 'tokio' rust \| wc -l; rg -l --no-ignore 'tokio' rust \| wc -l` → 18c, exit 0 | 771c, exit 6 | {"rgCounts":["595","     597"],"ocTotalFiles":"595"} |
| ignore-real-target-off | Real repo: same with defaultExcludes:false + noIgnore | `rg -l --no-ignore 'tokio' rust \| wc -l` → 9c, exit 0 | 941c, exit 6 | {"rg":"597","ocTotalFiles":"596"} |
| zero-results | No matches | `rg -n ZZZ_DEFINITELY_ABSENT_42 src` → 0c, exit 1 | 223c, exit 1 | {"rgExit":1,"ocHints":true} |
| zero-results-ast | No structural matches | `ast-grep run -p 'nonexistentFn($A)' -l typescript src` → 0c, exit 1 | 332c, exit 1 | {"ocMsg":"{\"results\":[{\"index\":0,\"data\":{\"hints\":[\"Write the pattern as a complete node (keep terminators like `;`), check a tree view, then broaden p… |
| regex-error | Invalid regex | `rg -n 'foo(bar' src` → 67c, exit 2 | 509c, exit 2 | {"rgExit":2,"ocError":true} |
| regex-error-ast | Invalid ast-grep pattern | `ast-grep run -p 'foo(' -l typescript src` → 242c, exit 0 | 332c, exit 1 | {"ocMsg":"{\"results\":[{\"index\":0,\"data\":{\"hints\":[\"Write the pattern as a complete node (keep terminators like `;`), check a tree view, then broaden p… |
| unicode-col-search | Column of a hit after emoji (rg --column is bytes) | `rg -n --column NEEDLE_EMOJI src/emoji.ts` → 114c, exit 0 | 273c, exit 0 | {"truth":{"utf16_0based":26,"byte_1based":31,"codepoint_0based":24},"rgColumn":31,"ocColumns":[26,44]} |
| unicode-col-ast | astSearch column after emoji (documented UTF-16 0-based) | `ast-grep run -p 'const NEEDLE_EMOJI = $V;' -l typescript src/emoji.ts…` → 23c, exit 0 | 383c, exit 0 | {"truthConstKeyword_utf16":20,"sgStart":"{\"line\":0,\"column\":18}","ocColumns":[20,41]} |
| unicode-col-symbols | astSearch symbols line for emojiFn after emoji comment | `rg -n 'function emojiFn' src/emoji.ts` → 62c, exit 0 | 475c, exit 0 | {"ocHasEmojiFnLine2":true} |
| secret-search | Search a file holding fake credentials | `rg -n 'AWS_SECRET\|GITHUB_TOKEN\|DB_URL\|PRIVATE' src/config.ts` → 332c, exit 0 | 894c, exit 0 | {"rgLeaks":["wJalrXUtnFEMI","ghp_aBcDe","hunter2secret"],"ocLeaks":[]} |
| secret-fetch | Read the credentials file | `cat src/config.ts` → 602c, exit 0 | 718c, exit 0 | {"ocLeaks":[]} |
| secret-ast | Structural match over the credentials file | `ast-grep run -p 'export const $N = $V' -l typescript src/config.ts` → 694c, exit 0 | 336c, exit 1 | {"ocLeaks":[]} |
| secret-ast-terminated | Structural match with terminated pattern over the credentials file | `ast-grep run -p 'export const $N = $V;' -l typescript src/config.ts` → 694c, exit 0 | 1064c, exit 0 | {"sgLeaks":["wJalrXUtnFEMI","ghp_aBcDe","hunter2secret"],"ocLeaks":[]} |
| ast-strictness | Unterminated pattern: ast-grep CLI matches, astSearch does not | `ast-grep run -p 'const NEEDLE_EMOJI = $V' -l typescript src/emoji.ts …` → 23c, exit 0 | 336c, exit 1 | {"sgHits":1,"ocHits":0} |
| secret-dotenv | Read .env | `cat .env` → 24c, exit 0 | 372c, exit 5 | {"ocRefusedOrRedacted":true} |
| crlf-fetch | CRLF file line fidelity | `sed -n '1,2p' src/crlf.txt \| od -c \| head -3` → 154c, exit 0 | 262c, exit 0 | {"ocKeepsCR":true} |
| missing-path | Nonexistent path | `rg -n foo src/nope.ts` → 95c, exit 2 | 227c, exit 3 | {"ocErr":"pathNotFound"} |
| rewrite-stale-guard | astRewrite apply after the file changed since preview | `ast-grep -U has no preview/apply guard (applies to whatever is on dis…` → 0c, exit  | 2549c, exit  | {"hadApply":true,"fileUnchangedByApply":true,"applyRejected":true} |
