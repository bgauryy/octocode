# ghGetHistoryItem: independently checkable questions

Use the [shared protocol](README.md) and [rating rubric](../../RATINGS.md).

| ID | Case | Question |
|---|---|---|
| GHI-01 | Normal | For the newest path commit discovered in GHH-01, read the complete patch only for internal/config/config.go and retain parent/commit identity. Use the discovered immutable commit, never a guessed one. |
| GHI-02 | Advanced | For a merged pull request discovered in GHH-02, read its body, every selected file and review using small response windows. Walk response pages before section pages. Compare complete selected collections with the provider and disclose unavailable patches. |
| GHI-03 | Issue | Read the body and complete comments of one issue discovered in GHH-02, keeping source identities and sequence. Compare all selected comments with a paginated provider read. |
| GHI-04 | Compare | Compare cli/cli b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0 with its first parent for internal/config/config.go only, keeping complete selected patches or an honest unchanged-file result. |
| GHI-05 | Invalid | Send a pullRequest without number, a compare without headRef, and sections body together with include. Preserve the actionable input responses. |
| GHI-06 | Summary/sections | Read a discovered merged pull request summary, then separately its files, reviews and one path patch. Confirm section reads retain number/state/commit identity and that independent section continuations cover complete requested evidence. |
