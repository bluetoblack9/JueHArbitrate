# CLI Source Coverage Audit — Task 11 (M0.2)

Upstream reference: `/Users/deep/Documents/workspaceB/openC/seek`, pinned commit
`cb27b90d2979f5f840526100b28ade586d034df9` (READ-ONLY).

Method: **static source reading only**. No Go code was compiled, run, or tested; no Go
binary was executed; no program output was collected. Every expected value in
`tests/fixtures/cli-contract.json` and `tests/fixtures/cli-subcommands.json` was derived
by reading source text at the pinned commit. The upstream repo was never written to.

Contractual counts verified against source: **45 canonical commands, 11 groups, 32
top-level flags, 12 pre-parse verbs, 3 positional lanes, 42 `flag.NewFlagSet` sites**.
All match the task contract; no count disagreement.

Companion fixtures: `tests/fixtures/cli-contract.json` (top-level index),
`tests/fixtures/cli-subcommands.json` (per-entry inventory). Structural gate:
`tests/cli_contract.rs`.

## 1. Top-level surface — cmd/seek/main.go

| Concern | Symbol | Lines | Behavior |
|---|---|---|---|
| exit mapping | `main` | 774-791 | `run()` nil → 0; `errToolsUsage` → 2 (text pre-printed stderr); other err → `seek: <err>` stderr exit 1 |
| help pre-scan | `helpRequested` | 801-811 | flat scan of `os.Args[1:]`; `help` only at index 0; `-h`/`--help` any index; stops at `--` without revoking an earlier match |
| usage text | `printUsage` | 818-849 | stdout when help requested |
| pre-parse dispatch | `run` | 858-946 | 12 verbs before flag registration: skill 858, memory 861, agents 870, session 879, checkpoint 885, undo 888, redo 891, hooks 898, worktree 906, cron 915, keys 924 (`keyscli.ErrUsage` → `os.Exit(2)` 926-927), serve 941 (`controlplane.ExitCode(err)`, no re-print, 942-944) |
| flag registration | `run` | 948-988 | 32 flags on `flag.CommandLine` (`flag.ExitOnError`) |
| help branch | `run` | 999-1002 | requested help → `printUsage(os.Stdout)`, exit 0 — runs AFTER pre-parse dispatch, BEFORE `flag.Parse` |
| parse | `run` | 1008-1009 | `flag.Usage` set 1008; `flag.Parse` 1009; parse error → usage stderr exit 2; unregistered `-help`/`--h` → `flag.ErrHelp` → usage stderr exit 0 (ExitOnError semantics) |
| mutual exclusion | `run` | 1012 | `-yolo` + `-plan` rejected before short-circuits |
| short-circuits | `run` | 1018-1032 | order: version 1018 → upgrade/upgrade-dry-run/upgrade-force 1022 → upgrade-check 1025 → install 1030; all before provider/session init, no API key required |
| theme validation | `run` | 1041-1045 | invalid `-theme` rejected |
| acp lane | `run` | 1065-1084, 1219-1223, 2286-2288 | marker 1065; provider/model pins flag > `SEEK_ACP_PROVIDER`/`SEEK_ACP_MODEL` env > auto (1074-1083); `acpYolo` via `SEEK_ACP_YOLO` (1219; `internal/acp/headless.go:28,33-43`, accepts 1/true/yes/on) |
| goal lane | `run` | 1459, 2293-2305 | marker 1459; cond = `TrimSpace(Join(os.Args[3:]))` iff `os.Args[2]=="run"`; `policy.SetPref(PrefYolo)` 2303 |
| autopilot lane | `run` | 1825-1833, 1952-1958 | marker 1825; goal joined iff `os.Args[2]=="run"` (1827-1829); `IsRemoteMutating` guard 1830-1833; model fallback 1953-1957 |
| provider init | `buildProvider` | 3780-3860, called 1114-1119 | no DeepSeek key → error exit 1 BEFORE lane dispatch; non-DeepSeek providers return nil `dsClient` with nil error (lanes reach their own client-nil checks) |
| tools whitelist | `applyToolsWhitelist` | 4177-4200 | unknown tool → stderr `-tools: unknown tool(s): ...` + `available tools: ...` + `errToolsUsage` (4138) → exit 2 |
| install | `runInstall` | pathprompt.go:72 | non-Windows → `install: adding seek to PATH is only supported on Windows` → exit 1 |
| lane runners | `runAutopilot` 2999-3027, `runGoal` 3556-3594, `runACP` 3603-3634 | — | usage checks precede client checks in both goal (3557-3562) and autopilot (3000-3008); ACP: stdout protocol / stderr logs, `SEEK_ACP_LOG` trace 3608-3618 |
| cron auto-tick | `cronAutoTickEnabled` | 1988, 4129-4130 | top-level `-no-cron-tick` (M4) gates the in-process tick, not `cron tick` |

Lane consequence (Go flag positional termination): `flag.Parse` stops at the lane verb,
so **trailing top-level flags are unreachable** — ignored for `acp`, joined into the
condition/goal text for `goal`/`autopilot` (comment at 3626-3628 confirms for ACP).
`seek acp -h` / `goal -h` / `autopilot -h` hit the top-level help pre-scan → stdout/0.

## 2. flag.NewFlagSet sites — all 42

### internal/skillcli (11)
| Site | Entry | Notes |
|---|---|---|
| skillcli.go:95 | skill install | SetOutput stderr 96; custom Usage 97-109; flags 111-116; NArg==1 121-124; `--type` validation 134-145 |
| skillcli.go:158 | skill uninstall | `keep-stats` bool default **true**, stub no-op 169; NArg==1 173-176 |
| skillcli.go:188 | skill update | `--all` 198; conflicts 204-206; partial-failure summary 224-226; NArg==1 230-233 |
| query.go:161 | skill list | source/json 169-170; **no NArg check** |
| query.go:240 | skill status | json 248; NArg==1 252-255; not-loaded 263-265 |
| query.go:429 | skill stats | top=10/since=720h/distill/idle=0s/json 437-441; **fs.Visit explicit-set detection 451-460**; idle conflicts 455, idle<=0 458; no NArg check |
| create.go:18 | skill create | custom Usage 20-40; description/project/into 42-44; **name-peel 53-57**; NArg==0 61-64; description required 70-73 |
| drafts.go:72 | skill drafts (list) | json 78; dispatch 61-70 has **no default branch**; no NArg check |
| drafts.go:161 | skill drafts show | no flags; NArg==1 174-177 |
| drafts.go:233 | skill drafts approve | project/user/force 245-247; NArg==1 251-254; mutual exclusion 255-257; scope sidecar logic 268-278 |
| drafts.go:302 | skill drafts reject | no flags; NArg==1 315-318 |

### internal/memorycli (6)
| Site | Entry | Notes |
|---|---|---|
| memorycli.go:95 | memory list | all/archived 97-98; load error → stderr + **return nil → exit 0** 103-107 |
| memorycli.go:156 | memory show | no flags; empty name → usage stderr **exit 0** 162-166; not-found stderr exit 0 175-178 |
| memorycli.go:207 | memory search | no flags; query=join(fs.Args()); empty → usage stderr exit 0 213-217 |
| memorycli.go:259 | memory archive branch 1 | reason default "manual archive" 261; `err==nil && NArg>0` + non-dash name → doArchive 264-269 |
| memorycli.go:274 | memory archive branch 2 (`fs2`) | second parse branch, reason2 same default 276; Parse(args[1:]) 272-280 — see judgment call J2 |
| memorycli.go:318 | memory add | **name-peel 312-316**; tagline/content/tags 320-322; Parse error returned → exit 1 323-325; missing fields → usage stderr exit 0 326-329 |

`doArchive` 286-306: ALL errors (load 289-291, not-found 294-297, filesystem 299-302) →
stderr + return nil → **exit 0**. No `os.Exit`/`ErrUsage` anywhere in memorycli.go.

### internal/agentscli (4)
| Site | Entry | Notes |
|---|---|---|
| agentscli.go:118 | agents list | no flags; NArg>0 error 127-130; stats.Errors stderr footer but exit 0 151-156 |
| agentscli.go:192 | agents check | no flags; NArg>0 error 201-204; error lines to **STDOUT** 231-233 + exit 1 234 |
| agentscli.go:240 | agents create | 9 flags 252-260; **name-peel 266-270**; NArg==0 274-277; agentsmgr.Create requires Description (agentsmgr.go:81-83), refuses existing target, no --force (116-117) |
| agentscli.go:308 | agents remove | no flags; NArg==1 323-326; builtins refused 327-333 |

### internal/sessioncli (1)
| Site | Entry | Notes |
|---|---|---|
| sessioncli.go:66 | session export | `o` default "" 67; SetOutput stderr 68; **flagsFirst reorder 123-143**; NArg<=1 72-74; id > Latest 75-96; -o "" → markdown stdout 99-103, else file + stderr confirmation 105-113 |

### internal/checkpointcli (5)
| Site | Entry | Notes |
|---|---|---|
| checkpointcli.go:148 | checkpoint list | session/json; **no SetOutput (default os.Stderr), no custom Usage, no NArg check**; JSONL 162-169 |
| checkpointcli.go:202 | checkpoint restore | session/dry-run/force; `<turn|last>` required 210-212; **parseTurnArg 244-254** ("last"/"latest"/"-1"→0; "-1" needs `--`) |
| checkpointcli.go:259 | checkpoint prune | session/before; **parseBefore 132-143**; no NArg check |
| checkpointcli.go:284 | undo | session/n=1/force; optional [path] absolutized 292-298; partial results before error 304-311 |
| checkpointcli.go:321 | redo | mirror of undo 320-353 |

Shared: `resolveSession` 94-125 (`--session` > Latest; no-session error 109).

### internal/hookscli (4)
| Site | Entry | Notes |
|---|---|---|
| hookscli.go:104 | hooks list | by-name 106; SetOutput stderr 105; no NArg check; gateForCWD 82-99 excludes untrusted project (stderr warning, no prompt) |
| hookscli.go:160 | hooks check | event/tool 162-163; missing event → usage stderr **exit 0** 167-170; unknown event stderr exit 0 199-202 |
| hookscli.go:236 | hooks trust | reset string 238; **fs.Visit 251-256**; all/cwd/path branches 261-279; reset errors stderr exit 0; bare trailing `--reset` is a parse error (J11) |
| hookscli.go:304 | hooks audit | since=0s/tool/denied/limit=0 306-309; read error stderr exit 0 314-318; limit keeps LAST N 344-346; newest-first 353 |

### internal/worktreecli (2)
| Site | Entry | Notes |
|---|---|---|
| worktreecli.go:82 | worktree list | flagless FlagSet; SetOutput stderr 83; errors wrapped `worktree list:` 88-101; 10s timeout 96 |
| worktreecli.go:123 | worktree gc | older-than=48h 125, orphan-after=DefaultOrphanAge=168h (worktree.go:634) 127; orphan pass skipped when <=0 154-156; 30s timeout 141 |

### internal/keyscli (2)
| Site | Entry | Notes |
|---|---|---|
| keyscli.go:74 | keys list | json 76 "emit JSONL on stdout"; Parse error incl. ErrHelp → **ErrUsage → exit 2** 77-79 (main.go:926-927) |
| keyscli.go:133 | keys actions | json 135 "emit JSON array on stdout"; same ErrUsage posture |

### internal/servecli (1)
| Site | Entry | Notes |
|---|---|---|
| servecli.go:123 | serve | SetOutput stderr 124; `frontend` fs.Var 125 (frontendList 90-97, Set appends → repeatable, zero value empty list); `listen` "" 126; fs.Usage=printHelp(stderr) 127; ErrHelp branch 129-136 unreachable belt-and-braces; badConfig 286-294 wraps `controlplane.ErrNoFrontends` → ExitCode 2 |

### internal/routinescli (6)
| Site | Entry | Notes |
|---|---|---|
| routinescli.go:165 | cron create | 9 flags 167-175 (**--autopilot/--goal absent from printHelp**); mutual exclusion 179-181; prompt=join(fs.Args()) 185-188; auto name 197-199 + autoGenName 398-404; cwd→$HOME 204-209 |
| routinescli.go:237 | cron list | json 239; JSON mode encodes empty list too 253-258; no NArg check |
| routinescli.go:288 | cron delete | flagless FlagSet; NArg==1 293-295 |
| routinescli.go:314 | cron run | timeout=DefaultRunTimeout=30m (tick.go:30) 316; NArg==1 320-322; failed job run is stdout status, exit 0 345-351; self-delete note 338-343 |
| routinescli.go:360 | cron tick | verbose 362; quiet idle/skipped 378-389; no NArg check |
| routinescli.go:442 | cron config check | probe 444; ✗/✓ lines to **STDOUT**; failures → exit 1 510-511; no NArg check |

## 3. Hand-written argument parsers (no FlagSet)

| Parser | Location | Behavior |
|---|---|---|
| top-level help pre-scan | main.go:801-811 | flat scan; `help` index-0 only; `-h`/`--help` anywhere before `--`; break at `--` keeps earlier match |
| pre-parse verb dispatch | main.go:858-946 | 12 raw `os.Args[1]` comparisons before flag registration |
| lane arg joining | main.go:2294-2298 (goal), 1827-1829 (autopilot) | raw `os.Args[3:]` join iff `os.Args[2]=="run"`; trailing flags become text |
| serve help pre-scan | servecli.go:111-119 | `help` first-position only; `-h`/`--help` any position before `--`; printHelp(stdout) → exit 0 |
| session flagsFirst | sessioncli.go:123-143 | `-o`/`--o` consume next value; `-o=`/`--o=` prefixes; other dash tokens (incl `--`) to flag bucket; positionals appended after; bare trailing `-o` left for Parse to report |
| skill create name-peel | create.go:53-57 | args[0] missing or dash-prefixed → Usage + error exit 1; flags parsed from args[1:] |
| agents create name-peel | agentscli.go:266-270 | same pattern |
| memory add name-peel | memorycli.go:312-316 | missing/dash-first → usage stderr **exit 0** |
| memory archive two-branch | memorycli.go:259-284 | branch 1 fs.Parse(args); branch 2 fs2.Parse(args[1:]) when args[0] non-dash; fallback usage stderr exit 0 282-283 |
| cron config sub-dispatch | routinescli.go:423-434 | sub defaults "check"; args[0] consumed only when non-dash; unknown → error exit 1 |
| keys check arg handling | keyscli.go:109-130 | args[0] non-dash → path, else default `~/.seek/keybindings.toml`; all other tokens (incl `-h`) ignored; validation errors stderr + ErrUsage exit 2 |
| checkpoint parseTurnArg | checkpointcli.go:244-254 | "last"/"latest"/"-1" → 0; positive int; else error |
| checkpoint parseBefore | checkpointcli.go:132-143 | required; RFC3339 or YYYY-MM-DD |
| checkpoint resolveSession | checkpointcli.go:94-125 | --session > Latest |
| fs.Visit explicit-set | query.go:451-460 (skill stats), hookscli.go:251-256 (hooks trust) | distinguishes explicitly-set flags from defaults |
| group verb switches | skillcli.go:35-63, drafts.go:61-70, memorycli.go:27-52, agentscli.go:37-60, sessioncli.go:27-41, checkpointcli.go:38-56, hookscli.go:32-55, worktreecli.go:41-57, routinescli.go:71-95, keyscli.go:51-71 | per-group no-args/help/unknown behavior recorded in fixture `groups` |

## 4. Judgment calls and discrepancies

- **J1 — memory archive exit codes (controller anchor corrected).** The task anchor
  claimed "filesystem errors exit 2 (memorycli.go ~274)". Static verification: line 274
  is the `fs2` FlagSet; `doArchive` (286-306) prints every error to stderr and returns
  nil → **exit 0**. memorycli.go contains no `os.Exit` and no `ErrUsage`. The fixture
  records the true source behavior (exit 0) with citations.
- **J2 — memory archive branch 2 is statically unreachable.** Branch 1 fires whenever
  `fs.Parse` succeeds with NArg>0 and a non-dash name; any non-dash args[0] makes Parse
  stop immediately with NArg>0, so branch 2's precondition (args[0] non-dash AND branch 1
  not taken) cannot occur. Consequence: name-first `archive <name> -reason "x"` takes
  branch 1, parse stops at the name, and **-reason is silently ignored** (reason stays
  "manual archive"). Upstream test memorycli_test.go:205-243 uses that form but asserts
  only the "archived" substring, consistent with this reading. Both branches are still
  inventoried (contract requires the second parse branch covered).
- **J3 — leaf help posture.** ContinueOnError leaf FlagSets return `flag.ErrHelp`
  verbatim → main prints `seek: flag: help requested` → **exit 1, usage on stderr**.
  Exceptions: keys list/actions (ErrUsage → 2), serve (pre-scan stdout/0), memory
  show/search/archive/add usage errors (stderr/0), memory add leading `-h` (name-peel
  usage stderr/0), skill/agents create leading `-h` (name-peel error exit 1). Group-level
  `help` verbs are stdout/0 except keys no-args (stderr/2) and skill drafts (no help verb).
- **J4 — skill drafts has no help verb and no unknown-subcommand error.** Dispatch
  (drafts.go:61-70) has no default branch; unknown tokens (including "help") fall to the
  list FlagSet as ignored positionals → inbox listing exit 0.
- **J5 — agents check prints definition errors to STDOUT** (231-233) then returns an
  error → exit 1 (234). Split from the usual stderr posture.
- **J6 — memory/hooks UX errors are stderr + exit 0** (memory list/show/search/archive/add
  load & not-found paths; hooks check missing/unknown event, trust reset failures, audit
  read failures). Flag parse errors in the same commands still exit 1.
- **J7 — checkpoint FlagSets use default output/usage** (no SetOutput, no custom Usage):
  errors and usage land on os.Stderr by flag-package default.
- **J8 — cron create --autopilot/--goal are registered but undocumented** in printHelp;
  mutually exclusive (179-181). Fixture records them from the registration site.
- **J9 — top-level `-help`/`--h`** are not caught by the pre-scan (only `-h`/`--help`);
  they fall through to `flag.Parse` (ExitOnError) → ErrHelp → usage **stderr exit 0**.
- **J10 — lanes cannot receive trailing top-level flags** (flag.Parse positional
  termination at 1009). goal/autopilot join them into condition/goal text; acp ignores
  them. `seek acp -h` etc. are answered by the top-level pre-scan (stdout/0), not a lane.
- **J11 — hooks trust bare trailing `--reset`** is a Go flag parse error ("flag needs an
  argument: -reset") → exit 1, despite the comment at 248-250 suggesting a bare form.
  `--reset=` / `--reset ""` give the explicit-empty cwd case via fs.Visit.
- **J12 — checkpoint restore `-1`** means "last" in parseTurnArg but the flag parser sees
  a flag token; it requires `-- -1`.
- **J13 — serve no-args exits 2** ("no frontends selected", buildFrontends 197-199 →
  badConfig → ErrNoFrontends → ExitCode 2). All serve config errors map to 2; runtime
  maps nil→0 / ErrDrainIncomplete→1 / ErrFrontendsDown|ErrNoFrontends→2 / other→1
  (controlplane/serve.go:294-305, sentinels 103-105); main never re-prints serve errors
  (941-946).
- **J14 — goal/autopilot check order.** runGoal/runAutopilot test the empty
  condition/goal BEFORE the nil client (3557-3562, 3000-3008), but `buildProvider`
  (1114-1119) aborts earlier when no key exists at all; the client-nil messages fire only
  when a non-DeepSeek provider was configured (nil dsClient, nil error).
- **J15 — keys check has no help path**: `-h` is ignored and a real check runs on the
  default file. Help exists only at group level.
- **J16 — cron config `-h` vs `help`**: `-h` keeps sub="check" → cmdConfigCheck ErrHelp →
  usage stderr exit 1; the token `help` is an unknown subcommand → exit 1.
- **J17 — install/positional flags-first rule**: after the first positional, flag.Parse
  stops; trailing flags become extra positionals → error where NArg is checked, ignored
  where not (per-entry count_rule records which).

## 5. Fixture conventions

- `cases[].argv`: tokens AFTER the binary, INCLUDING command words
  (e.g. `["hooks","list","-h"]`). Exception: `serve.help.help_scan_cases[].argv` excludes
  binary AND `serve` (per brief), matching the scan input at servecli.go:111.
- `sources[]`: `file:symbol:lines` (or `file:lines`) at the pinned commit.
- `flags[].declared_default`: the literal registration value (Go duration strings like
  "48h"/"168h"/"30m"/"720h"/"0s"; empty list `[]` for serve frontend). Effective defaults
  resolved from config/env are documented in `positionals.notes` / `parsing.notes`, never
  guessed as runtime values.
- `flags[].aliases`: `[]` everywhere — Go registers no alias names; single/double dash
  equivalence is a parser property recorded in `parsing.dash_equivalence`. Verb aliases
  (ls/rm/forget/...) are recorded per command/group in `aliases`.
- `repeatable`: true only for serve `--frontend` (flag.Value append semantics); all scalar
  flags are last-wins, not repeatable-accumulating.
- `errors` keys vary per entry to name the actual failure categories; categories that do
  not apply are explained in `positionals.count_rule` / `help.notes` with source lines
  (e.g. "no NArg check"), never silently omitted.
- `help.exit_code: null` / `stream: "n/a"` appears only for keys check, which genuinely
  has no leaf help path (keyscli.go:109-130).

## 6. Manual verification record

- Read in full (static): cmd/seek/main.go (dispatch/help/flags/lanes/runners/buildProvider/
  applyToolsWhitelist), pathprompt.go runInstall, and the complete CLI files of skillcli
  (skillcli/create/query/drafts), memorycli, agentscli, sessioncli, checkpointcli, hookscli,
  worktreecli, routinescli, keyscli, servecli; controlplane/serve.go exit-code contract;
  agentsmgr.Create validation; worktree.DefaultOrphanAge; routines.DefaultRunTimeout;
  acp/headless.go YoloFromEnv; memorycli_test.go archive test (assertion scope only).
- Enumerated every `flag.NewFlagSet` occurrence repo-wide (42 sites, section 2) and
  cross-checked each against its fixture entry; no site unmapped.
- Verified contractual counts against source: 45 canonical entries (aliases not counted
  as entries), 11 groups, 32 top-level flags, 12 pre-parse verbs, 3 lanes.
- RED evidence: `cargo test --workspace --locked --test cli_contract` failed with
  NotFound on the missing fixtures before Step 3 (tests compiled; failure was missing
  data, not compile errors). GREEN: both tests pass after fixtures + this audit doc.
- No TODO/placeholder values remain in either fixture.
