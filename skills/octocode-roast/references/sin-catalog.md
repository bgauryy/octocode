# The Sin Catalog

Severity-tiered code sins with detection patterns and roast lines. For language-specific sins and ready-to-run search queries, see language-sins.md.

## Severity Levels

| Level | Icon | Meaning |
|-------|------|---------|
| CAPITAL OFFENSES | 💀 | Security, data, or correctness risk; fix now |
| FELONIES | ⚖️ | High-impact maintainability, performance, or safety risk |
| CRIMES | 🚨 | Real defects or friction worth scheduling |
| SLOP | 🤖 | AI hallucinations & filler |
| MISDEMEANORS | 📝 | Judge silently |
| PARKING TICKETS | 🅿️ | Mention if bored |

---

## 💀 CAPITAL OFFENSES (Fix Now)

### Security Sins

| Sin | Pattern | Roast |
|-----|---------|-------|
| Hardcoded secrets | `password=`, `api_key=`, `secret=`, `token=` with a real value | "Credential-shaped literal in code. Rotate if real, remove either way." |
| `eval()` usage | `eval(`, `new Function(` | "Running `eval()`? Let me know when you start accepting TCP connections from strangers too." |
| SQL injection | String concat in queries | "Bobby Tables sends his regards." |
| XSS vectors | `innerHTML =`, `dangerouslySetInnerHTML` without sanitization | "XSS delivery mechanism deployed. Hackers can now run a casino in your DOM." |
| No input validation | Direct user input to DB/shell/file without validation | "User input goes straight into the blast zone. Very generous threat model." |
| Path traversal | User input in file paths without sanitization | "`../../../etc/passwd` has entered the chat." |
| Insecure deserialization | `pickle.loads()`, unsafe YAML/object loaders, untrusted serialized payloads | "Deserializing untrusted data. Congratulations, the parser is now part of your attack surface." |
| Disabled security | `verify=False`, `rejectUnauthorized: false` | "SSL verification disabled. Man-in-the-middle attackers thank you for your hospitality." |

---

## ⚖️ FELONIES (Fix Today)

### Type & Safety Sins

| Sin | Pattern | Roast |
|-----|---------|-------|
| `any` abuse (5+ instances) | `: any`, `as any` | "TypeScript saw this and asked to be called JavaScript again." |
| Force unwrap spam | `!.`, `!!` | "Using `!` like you've never been null-referenced before. Spoiler: you will be." |
| Empty catch blocks | `catch { }` | "Swallowing exceptions like you're being paid per suppressed error." |
| `var` declarations | `var ` | "Time traveler detected. Welcome to the future, we have `const` now." |

### Performance Sins

| Sin | Pattern | Roast |
|-----|---------|-------|
| N+1 queries | Loop containing DB/API calls | "N+1 query in a loop. Your database is crying. I can hear it from here." |
| Sync I/O in async context | `readFileSync` in async, blocking event loop | "Blocking the event loop like it owes you money." |
| Memory leak patterns | Unbounded arrays, listeners not cleaned | "Memory leak detected. Your app is a hoarder." |
| Missing pagination | Fetching all records | "`SELECT * FROM users` — Bold choice for a table with 10 million rows." |
| Unbounded loops | No limit on iterations | "Infinite loop potential. Enjoy your frozen browser tab." |

### Structure Sins

| Sin | Pattern | Roast |
|-----|---------|-------|
| Callback hell (4+ levels) | Nested `.then(` or callbacks | "This indentation is legally classified as a geological formation." |
| God function (200+ lines) | Manual count plus mixed responsibilities | "This function has more responsibilities than a startup CEO during a funding round." |
| God class (1000+ lines) | Class line count plus mixed responsibilities | "This class does everything. It's not a class, it's a company." |
| 500+ line files | Line count | "This file needs a table of contents and possibly a bibliography." |
| Circular dependencies | A imports B imports A | "Circular dependency detected. Your code is having an existential crisis." |
| Global state mutation | `window.`, mutable globals | "Globals everywhere. Bold choice for someone who clearly hates debugging." |
| Tight coupling | Direct instantiation, no DI | "These classes are so tightly coupled they need couples therapy." |

---

## 🚨 CRIMES (Fix This Week)

### Code Quality Sins

| Sin | Pattern | Roast |
|-----|---------|-------|
| Magic numbers | Unexplained numeric literals | "42? Is this the answer to life or just the first number you thought of?" |
| Copy-paste code | Duplicate blocks | "Ctrl+C, Ctrl+V — the WET design pattern. Write Everything Twice." |
| 10+ function args | Argument count | "This function signature reads like a legal contract." |
| Nested ternaries | `? : ? :` | "Ternary inception. We need to go deeper... said no one ever." |
| Boolean trap | `fn(true, false, true)` | "`process(true, false, true, false)` — Is this code or Morse code?" |
| Switch 20+ cases | Case count | "This switch statement is longer than my will to live." |
| Sleep-based sync | `sleep(`, `setTimeout` as sync | "`await sleep(1000)` — Ah yes, hope-driven development." |

### Concurrency Sins

| Sin | Pattern | Roast |
|-----|---------|-------|
| Race condition | Shared state without locks | "Race condition detected. May the fastest thread win. Or crash. Dealer's choice." |
| Missing error handling in async | Unhandled promise rejection | "`async` without `catch`. Living dangerously." |
| Deadlock patterns | Nested locks, await in locks | "Deadlock waiting to happen. Your app will freeze like it saw a ghost." |

### Frontend Sins

| Sin | Pattern | Roast |
|-----|---------|-------|
| `!important` spam | Multiple `!important` | "CSS so unhinged it's screaming at itself." |
| z-index: 999999 | High z-index values | "z-index arms race. Next PR: z-index: Infinity." |
| Prop drilling (5+ levels) | Props passed through many components | "Props passed down more generations than family trauma." |
| useEffect abuse | Missing deps, infinite loops, stale captured values | "`useEffect` is now a tiny state machine with no manual." |
| No error boundaries | Missing React error boundaries | "No error boundaries. One bad render and the whole app goes white screen of death." |

### Testing Sins

| Sin | Pattern | Roast |
|-----|---------|-------|
| No tests | Missing test files | "No tests. Bold strategy. Let's see if it pays off." |
| Test naming | `test1`, `test2`, `it works` | "Test named 'it works'. Descriptive. Very helpful when it fails." |
| Testing implementation | Mocking everything | "You're testing your mocks, not your code. Congratulations, the mocks work." |

---

## 🤖 SLOP (AI Hallucinations & Filler)

| Sin | Pattern | Roast |
|-----|---------|-------|
| AI Intro | "In today's digital landscape..." | "This prose walked out of a content mill and into your codebase." |
| Formulaic filler | Stock phrases that obscure the actual behavior | "The comment has vibes, but the code needed facts." |
| Verbosity | 10 lines to say `i++` | "This comment is longer than the function. Brevity is the soul of wit, and this is witless." |
| Em-Dash Abuse | Multiple `—` in comments | "The em-dash abuse is real. We get it, you know grammar. Stop lecturing the compiler." |

## 📝 MISDEMEANORS (Judge Silently)

| Sin | Pattern | Roast |
|-----|---------|-------|
| WHAT comments | `// increment`, `// loop` | "`i++ // increment i` — Thanks, I was worried it might do something else." |
| Console archaeology | `console.log('here')` | "`console.log('here 2')` — A debugging strategy as old as time." |
| TODO fossils | `TODO` + old date | "TODO from 2019. The task outlived two jobs and a pandemic." |
| Single letter vars | `x = y + z` | "Variable naming by someone who peaked in algebra class." |
| Inconsistent naming | Mixed conventions | "`getData`, `fetch_info`, `retrieveSTUFF` — Pick a personality." |
| Dead code commented | Large comment blocks | "200 lines commented 'just in case'. The case: never." |
| `eslint-disable` | `eslint-disable` comments | "Disabling the linter is like removing the smoke detector to cook." |
| Git conflict markers | `<<<<<<<` | "You committed a git conflict. The code equivalent of a crime scene photo." |

## 🅿️ PARKING TICKETS (Mention If Bored)

| Sin | Pattern | Roast |
|-----|---------|-------|
| Trailing whitespace | Whitespace at EOL | "Trailing whitespace. Your code has dandruff." |
| Missing semicolons | ASI reliance | "Letting JavaScript guess where statements end. Brave." |
| == instead of === | `==` comparison | "Type coercion roulette. Sometimes `'1' == 1`. Sometimes your app crashes." |
| Utils dumping ground | Giant utils file | "`utils.ts` — Where functions go when you can't be bothered to organize." |
| Manager classes | `*Manager`, `*Handler` | "`UserDataManagerHandler` — Buzzword bingo winner." |
