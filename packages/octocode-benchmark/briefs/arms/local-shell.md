## Arm brief: local shell, lean

Your only tool is Bash, limited to read-only commands on the clones: `rg`, `grep`, `find`, `fd`,
`ls`, `cat`, `sed -n`, `head`, `tail`, `wc`, `sort`, `uniq`, `cut`, read-only `git` (`log`, `show`,
`grep`, `ls-files`, `ls-tree`, `blame`, `rev-parse`, `cat-file`, `diff`) and `ast-grep` (also
`sg`) for structural search. Make every byte you read back count:

- **Search with line numbers.** `rg -n PATTERN /abs/clone/sub/dir`; `-F` for literals, `-w` for
  whole words, `-g '*.rs'` or `-g '!*_test.go'` to filter, `-l` or `-c` to list or count before
  reading, `-m N` to cap. A matching line that answers ends the question.
- **Read regions, not files.** `sed -n '120,170p' FILE` or `rg -n -A8 PATTERN FILE`; check `wc -l`
  before anything large and never `cat` a big file.
- **Structure.** `ast-grep run -l rust -p 'pub async fn $N($$$A) -> $R { $$$B }' DIR` for
  declarations and call shapes; `--json=compact` with `head` keeps output small.
- **Trees.** `ls DIR`, `git -C CLONE ls-files 'glob'`, `find DIR -maxdepth 1 -name '*.go'`.

Use absolute paths. One command per call, optionally piped into `grep`, `head`, `tail`, `wc`,
`sort`, `uniq`, `cut` or `sed -n`; shell variables, `$(…)`, `;` or `&&` chains and redirection
into files are not allowed, and a call that uses them counts as failed. Read only: never run a
command that writes.
