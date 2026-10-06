# Communication group review

All four installed hosts reviewed the communication skill and code, sent findings to each other, then cross-examined the integrated fixes. The complete group has **28 messages, 28 acknowledged deliveries, eight correlated request/reply conversations, all 12 directed peer pairs, and zero pending mail**. The final snapshot contains **694 unified records**, cursors 1–694 without gaps. No reviewer controller authored or acknowledged its model's mail.

Run: 2026-10-05. Scope: every conversation in this dedicated review group, including full message bodies, delivery receipts, native host streams and tool actions. Unrelated existing user sessions are excluded. The coordinator created the bindings, maintained presence, verified findings, edited sources and acknowledged replies. Reviewers were read-only. No commits, stashes, user settings changes or live shared-DB migration occurred.

## Participants and reasoning evidence

| Host | Installed version / model evidence | Requested effort and evidence | Rounds |
| --- | --- | --- | --- |
| Codex | 0.159.2; configured gpt-6.1-sol; server-selected model unproved | xhigh accepted by CLI configuration; no independent server attestation | Two actual ephemeral CLI sessions using the same maintained DB binding; exit 0 both |
| Claude Code | 2.1.289; emitted claude-opus-5-5 | max accepted; per-turn effort active in native result | Same native session resumed; exit 0 both |
| Grok Build | 1.0.46; emitted Grok 4.7 | ACP advertised xhigh, set_config_option selected it and session state attests it | First prompt timed out at 420s; same session resumed and completed; round two end_turn |
| Pi | 0.99.2; emitted guy-provider-anthropic-x / claude-opus-5-5 | --thinking max accepted with thinking blocks; provider's exact applied effort unproved | Two actual CLI invocations with the shipped communication extension; exit 0 both |

These are four independent host reviews. Pi and Claude used the same underlying model family, so this is not four independent model vendors. Every reviewer received/read the communication and brainstorming skills and applied alternatives, evidence, architecture/product/value objections and concessions. Public reports give conclusions and evidence; they do not expose private reasoning traces. Mode flags are observations, not proof of model quality.

## Complete communication audit

| From / to | Codex | Claude | Grok | Pi |
| --- | --- | --- | --- | --- |
| Codex | — | 5 | 6 | 7 |
| Claude | 10 | — | 8 | 9 |
| Grok | 17 | 18 | — | 19 |
| Pi | 12 | 11 | 13 | — |

Cells are actual informational message IDs, all read and acknowledged by their recipient models. Late Grok messages were handled in round two. FYIs received no prohibited replies.

| Reviewer | First request → final reply | Cross-examination request → reply |
| --- | --- | --- |
| Codex | 1 → 16 | 21 → 28 |
| Claude | 2 → 15 | 22 → 26 |
| Grok | 3 → 20 | 23 → 25 |
| Pi | 4 → 14 | 24 → 27 |

Every request has exactly one final answer with the correct sender, recipient and conversationId. Every message has its fixed delivery receipt; all retry keys are unique per sender. Every exported record retains id/path/from/to/type/timestamp/data and optional branch. Message bodies resolve from their immutable operational rows; no body was clipped from the export. The snapshot is read-only audit evidence, not a participant visibility bypass in fetch.

- [Audit checks and exact record/type counts](.octocode/tmp/communication-group-20261005-012249/conversation-audit.json)
- [All conversations, full bodies and receipts](.octocode/tmp/communication-group-20261005-012249/conversations.md)
- [Every unified record](.octocode/tmp/communication-group-20261005-012249/records-full.jsonl), [messages](.octocode/tmp/communication-group-20261005-012249/messages-full.jsonl), [deliveries](.octocode/tmp/communication-group-20261005-012249/deliveries-full.jsonl)
- [Verified SQLite snapshot](.octocode/tmp/communication-group-20261005-012249/group-final.sqlite), [export integrity/hash receipt](.octocode/tmp/communication-group-20261005-012249/db-export-final.json)
- [Artifact sizes and SHA256 hashes](.octocode/tmp/communication-group-20261005-012249/artifact-manifest.json)

Five identities left, ten edit leases released, and five coordinate.out records retained. The parent heartbeat worker and owned Grok ACP driver stopped normally. No review process remains running.

## Improvements and deciding evidence

| Finding | Decision / change | Verification |
| --- | --- | --- |
| Leave retained claimedBy/claimUntil despite promising release | Clear pending claims in the existing leave transaction; preserve acknowledgement state and uncertain dispatches | Reproduced before fix; Python test checks (None,0,None) and no ACK record |
| Codex advertised 5000 but renderer permitted up to 9000 bytes | One 5000 constant now feeds config, inline rendering and batch deferral; other hosts retain their budgets | Previously failing 6500-byte fixture now returns intact envelope/fetch references; exact payload survives |
| Large batches could overrun the configured host budget | Retain deferred rows; offer them later with exact IDs and executable full-data references | Both Codex/Grok fixtures cover all 16 rows exactly once, byte bounds and no stranded staged state |
| Pi ambiguous errors hid its automatic retry key and repeated argv | Show unchanged generated/explicit key, full CLI stderr and captured stdout; omit duplicate command echoes | Real CLI commits, fixture stalls process past 10s, new tool call reuses key and inbox remains exactly one |
| Record cursor confused with mail ID | Move concrete id-versus-data.messageId example before completion; extend cross-vendor typed-fetch/reply test to Pi | Real standalone handbook lifecycle and four-vendor regression use differing IDs and atomic replies |
| Handbook required too much secondary discovery | Add supplied-binding quick path, small profiles and single-command/type discovery; document raw wait ownership, operator audit and Pi timing | All 50 JSON command examples validate; selected standalone lifecycle executes; all 24 type/data definitions remain |
| Stale SERVICE_PROTOCOL prose described flattened context/null removal | Describe the unified envelope and distinguish absent optional metadata from required fields/generic explicit nulls | Source and schema regressions preserve mandatory envelope and arbitrary JSON |

The first metadata-pressure hook can defer all mail. A real deep-path fixture reproduced a 4239-byte first banner with one pending message; the next hook offers its full fetch reference without any peer change. The added regression passes. No production replay behavior was added. Constantly changing peer/context generations can still consume budget; raw pending fetch remains the recovery path.

## Cross-examination and reflection

Codex and Claude initially favored changing inbox wait behavior. Both conceded that read-only waits have a coherent ownership boundary: raw agents must maintain presence or wait less than its remaining lifetime; managed/listen paths own heartbeat. The skill now makes this explicit. Expiry remains a loud error, not a silent renewal.

Pi proposed removing the peer snapshot recheck. It withdrew that recommendation after reading the transaction: the first check avoids idle writes, the second prevents concurrent hooks emitting the same delta. No speedup was measured. Pi also withdrew its fixed-15s heartbeat claim; timing is a 15s target, approximately 15–25s under idle polling plus CLI latency, not a hard guarantee.

Grok withdrew null omission. Its suggestion came from stale flattened-context prose rather than a storage defect. Mandatory to:null and arbitrary JSON nulls remain. Claude corrected its claims that every handbook command executes and that no data-loss issue exists: all 50 examples validate, but only a selected lifecycle executes; unexamined native transports cannot be certified by this review.

Pi's suggestion to suppress retry guidance whenever stderr is present was held. Arbitrary process errors/stderr do not establish transaction outcome; this adapter has no structured committed-versus-rejected discriminator. The conservative hint can cost a recovery lookup on deterministic rejection, but avoids inventing certainty. Already staged/uncertain deliveries deliberately require inspection and explicit recovery after leave/resume; clearing claims does not authorize replay.

Architect judgment: one type catalog and one query envelope simplify discovery; short operational transactions still enforce presence, leases, replies and receipts. Product judgment: early exact IDs, installation/hooks/all-command examples and visible retry keys reduce avoidable confusion. Value judgment: actual interoperability is proved in this group; universal performance, automatic skill selection and all OS/native transport behavior are not proved.

| Reviewer | First judgment | Final judgment | Boundary |
| --- | --- | --- | --- |
| Codex | Skill 7; runtime 8 | Skill 8; runtime 8.5 | No native clipping or live duplicate established |
| Claude | Correctness 8; usability 6.5; efficiency 7.5; portability 8 | Correctness 8; usability 7.5; efficiency 7.5; portability 8 | Scores apply to reviewed paths; not a transport certification |
| Grok | Overall 8 | Overall 9 | Corrected stale-contract recommendation; no benchmark |
| Pi | Overall 7.5 | Overall 8; durability 8.5; retry safety 8 | Controlled timeout recovery; conservative validation hint remains |
| Coordinator | — | Overall 8.5 | Subjective engineering judgment; all four host paths tested locally |

Scores are subjective, not calibrated measurements or an average of independent model vendors.

## Efficiency and conversation quality

[Exact discovery byte measurements](.octocode/tmp/communication-group-20261005-012249/catalog-efficiency.json): full schema 80,192 bytes; one send_message schema 1,021; full type definitions 18,364; compact complete field inventory 3,757; one message type schema 2,066. Compact type discovery is about 80% smaller while full constraints/examples remain available. Use supplied bound tools and a small profile; request one command/type when needed.

The handbook is intentionally about 29 KB because the user requires every step in SKILL.md. Its 30 KB allowance is explicit; tool/hook/page budgets remain unchanged. Typed lookup reduces subsequent discovery load, not the initial handbook size.

Round-two calls decreased for Codex 36→29, Claude 27→19 and Pi 36→18, but tasks/context differed. Codex input counters increased 991,790→1,473,838 with substantial cached input. Grok still reported large cumulative/request counters. Fewer commands did not establish fewer provider tokens or a speedup. Original scopes/counters remain in host artifacts; overlapping/cumulative counters are not summed as unique context. The 570 lease-renewal records are operational evidence from the coordinator's maintenance loop, not 570 model messages, and were retained.

Audit quality issues were preserved:

- Codex initially confused the record cursor and mail ID, corrected before completing. Caller/schema mistakes and an obsolete source continuation chain are retained; the abandoned chain is not called complete coverage.
- Claude's first pass used ten clipping-shaped shell commands; round two used narrow dogfood reads. Two localSearch contract errors were corrected. One native tool response clipped 1549 characters; later deciding reads recovered hook/Pi evidence. Partial-read status 6 and an empty search are distinguished from protocol failure.
- Grok's first 420s timeout and all partial source/tool frames remain. Same-session recovery completed all sends/replies. Round two has 14 tool histories, no failed tools or permission requests. Installed memory context was not used as deciding evidence.
- Pi used raw read/grep/sed and nine/six clipping-shaped commands in its two passes, bypassing repo dogfood practice. Its first pass has six explicit partial-file continuations; neither pass has a confirmed host truncation marker. One second-pass grep looked for peer_context in the wrong file and was corrected. It reported about 12 calls; native logs show 18, which this audit uses. No source-completeness claim is inferred from those partial reads.
- Coordinator made corrected schema/path/compact-form mistakes, initially named the extracted review folder incorrectly, and displayed oversized metadata/protocol output through the outer response window. Full saved protocol and audit files remain. The new pressure fixture initially hit Node SQLite's long-path open limit; it now checks dispatch state through the actual Python CLI and unified fetch. No production path restriction or test deadline was changed.

## Full host conversations and audits

| Host | First review / full stream | Cross-examination / full stream | Action/error/usage audit |
| --- | --- | --- | --- |
| Codex | [Report](.octocode/tmp/communication-group-20261005-012249/codex/report-first.md), [native stream](.octocode/tmp/communication-group-20261005-012249/codex/stdout-first.jsonl) | [Report](.octocode/tmp/communication-group-20261005-012249/codex/report-second.md), [native stream](.octocode/tmp/communication-group-20261005-012249/codex/stdout-second.jsonl) | [Both rounds](.octocode/tmp/communication-group-20261005-012249/codex/controller-audit-all.json), [round one](.octocode/tmp/communication-group-20261005-012249/codex/controller-audit-first.json), [round two](.octocode/tmp/communication-group-20261005-012249/codex/controller-audit-second.json) |
| Claude | [Conversation](.octocode/tmp/communication-group-20261005-012249/claude/conversation.md), [native stream](.octocode/tmp/communication-group-20261005-012249/claude/stdout.jsonl) | [Conversation](.octocode/tmp/communication-group-20261005-012249/claude/conversation-second.md), [native stream](.octocode/tmp/communication-group-20261005-012249/claude/stdout-second.jsonl) | [Round one](.octocode/tmp/communication-group-20261005-012249/claude/audit.json), [round two](.octocode/tmp/communication-group-20261005-012249/claude/audit-second.json) |
| Grok | [Review](.octocode/tmp/communication-group-20261005-012249/grok/review.md), [initial frames](.octocode/tmp/communication-group-20261005-012249/grok/frames.jsonl), resumed frames listed in manifest | [Conversation](.octocode/tmp/communication-group-20261005-012249/grok/round2-review.md), [native frames](.octocode/tmp/communication-group-20261005-012249/grok/round2-frames.jsonl) | [All rounds](.octocode/tmp/communication-group-20261005-012249/grok/audit.json), [round two](.octocode/tmp/communication-group-20261005-012249/grok/round2-audit.json) |
| Pi | [Conversation](.octocode/tmp/communication-group-20261005-012249/pi/conversation.md), [native stream](.octocode/tmp/communication-group-20261005-012249/pi/stdout.jsonl) | [Conversation](.octocode/tmp/communication-group-20261005-012249/pi-round2/conversation.md), [native stream](.octocode/tmp/communication-group-20261005-012249/pi-round2/stdout.jsonl) | [Round one](.octocode/tmp/communication-group-20261005-012249/pi/audit.json), [round two](.octocode/tmp/communication-group-20261005-012249/pi-round2/audit.json), full tool ledgers/usage in each folder |

## Verification and limits

- Full canonical Node suite: **281/281 passed**, zero failed/skipped/cancelled, 413.716s. [Full log](.octocode/tmp/communication-group-20261005-012249/package-tests.log).
- Python: **7/7 passed**; build, Python/JS syntax and 60 relative links passed.
- Final full host-hook suite, including the added deep-path progress regression: **26/26 passed**. [Log](.octocode/tmp/communication-group-20261005-012249/host-hooks-final.log). The new test was added after that file ran in the full suite; it is verified separately rather than claimed in the 281 total.
- Final standalone handbook: **2/2 passed**. All 50 examples validate against the catalog; selected messaging/record/lease lifecycle executes from a copied skill.
- Extracted release smoke passed actual CLI/MCP reply correlation, atomic acknowledgement, deduplication, unified records, leases and export/restore.
- Extracted skill review: **0 errors, 2 optional warnings** (intentional unshipped README; shell CLI variable misclassified as a route). Repository docs and whitespace checks passed.
- Conversation audit: all checks true; all message/record evidence exported; SQLite integrity ok; identities and leases cleaned up.

Earlier seven managed-host fixture failures remain as historical receipts. They ran successfully in this recheck without deadline changes or a production launcher workaround; the macOS pre-entry stall's cause remains unconfirmed. Automatic skill-trigger accuracy, Windows/Linux, live native socket/app-server edge cases and an oversized native-host spill are not certified. Previous fresh Claude/Codex/Grok hook nonce checks plus this four-host tool exchange are separate evidence, not blanket validation of every adapter.
