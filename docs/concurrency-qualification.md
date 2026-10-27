# Concurrent-user qualification

Status: **RC.10 release qualification passed; publication authorized**
(27 October 2026). The user authorized publication
after reporting a recurring production save conflict. That exact generic message
was also reproduced locally with no unique fields: a legacy global control-number
mirror rejected a repeated value in an independent module. The release adds a
targeted fix; its final versioned executable passed a fresh 100-user run below.
This is not a claim that published RC.9 supports 100 simultaneous active users.
Do not publish RC.10 on the strength of earlier eight-user or large-read-fixture
tests. Those cover different failure modes.

## RC.10 targeted release checks

The exact RC.10 executable is SHA-256
`07a464ca03edba933bcac58fbe4054850e179bee41f52d9401ccd58060b4fee4`.
Its targeted save regression passed after the original error was reproduced
against the preceding build. Ordinary `control_no` values no longer inherit the
old global uniqueness constraint. A repeated value allocates hidden compatibility
metadata without modifying the submitted field or any previous record. Hidden
sequence allocation reconciles against stored year-zero identifiers, including
retained/deleted records. No uniqueness constraint is dropped or data renumbered.

The regression includes same-module and independent-module duplicates, twenty
concurrent matching values, payload-bound replay, missing/regressed sequence
state, restart, and explicit uniqueness on create/PATCH/PUT. Genuine unique-field
failures identify the field without exposing another record's private values.
Unexpected constraint failures retain private extended SQLite diagnostics.

Native **237 passed / two ignored** and frontend **385 passed**. The matching
frontend production build passed. Published RC.9 database preservation, real
RC.9-to-RC.10 signed activation/backup/paired rollback/killed-worker recovery,
CLI and four-engine updater UI/admin-draft checks passed. Private evidence:
`/mnt/PROJECT-DISK-1TB/tests/mx-rc10-release-Vz0xbz/`.

### Final versioned 100-user result: PASS

`/mnt/PROJECT-DISK-1TB/tests/mx-concurrency-UhIw1S/results.json` records
**33,123 requests in 492.20 seconds**, zero unexpected failures and the exact
RC.10 executable above. The real N1 binary remained
`c7b1e4ded7194073ad85b90c84966244854022c8f98da8a00fb04163129fcd5f`,
source `90f436ff54a6b67fe892235cdbd6424503eca353`, with its default admission/rate
policy. No N1 source or production deployment was changed.

- 100 distinct authenticated accounts and 100 live connections; 10,000 existing
  synthetic records, two mixed create/edit/read rounds and replay checks passed.
- 1,000 imported rows, zero failed, while 100 accounts read/edited: 193.64 s.
  100 workflow actions, 100 balanced journals and 50 reservations plus 50 expected
  insufficient-stock denials passed, with zero overdraw.
- Current permission revocation, creator/field isolation and a grant revoked
  while a create waited passed. No private live-record leak was observed.
- A one-minute sustained workload completed 5,109 requests and 533 edits.
- 100 simultaneous random 256 KiB files passed exact download and range checks
  in 64.23 s. A same-name collision preserved the winning bytes.
- Acknowledged records/files survived forced MX/N1 process restarts, integrity
  and foreign keys were clean, and 100 live clients reconnected. A separate
  interrupted-create/delivery fault reconciled 100 operations exactly once and
  recovered 10,000 durable notifications. No unexpected server error was logged.

Slowest ordinary read **2.61 s**, ordinary write **9.61 s**, below the actual
15/30-second browser deadlines without hidden retries. Peak measured RSS was
**701.79 MiB MX / 54.47 MiB N1**. This remains a bounded API/WebSocket qualification
on one shared HDD, not every production server, physical power loss or 1,000
active users. The two earlier passes below predate the version/control fix and
are supporting history, not substitutes for this final executable's result.

## Pre-versioning qualification

Both pre-versioning target runs used executable SHA-256
`197200af86a16d84c6dbf8acb32d23b0d36217d2c043dbc4486f2aa550aaee41`,
the latest selected N1 commit/binary identified below, **N1's default admission
and rate settings**, and MX's new default eight-transfer budget. Ordinary
requests had actual browser deadlines: 15 seconds for reads, 30 seconds for
writes, with no hidden automatic retries. Uploads had a 180-second test deadline
(the production browser allows 30 minutes).

| Run | Workload and outcome |
| --- | --- |
| Full target | 100 accounts/live connections, 10,000 existing records, three create/edit rounds, 1,000-row import with active editors, two-minute sustained workload, workflows/books/stock/privacy/files/crash recovery: **PASS**, 41,952 requests, 818.75 s |
| Independent repeat | 100 accounts/live connections, 10,000 existing records, two create/edit rounds, one-minute sustained workload, same business/privacy/recovery phases and random 256 KiB file payloads: **PASS**, 11,124 requests, 376.26 s; no import in this repeat |

Both runs had **zero unexpected request failures**, clean SQLite integrity and
foreign keys, correct formulas/sequence/replay/ledger/stock results, current
permission enforcement, ordered private/public live events and recovered
acknowledged file/record bytes. Explicit stale-field/stock/access denials and
connections deliberately severed during fault injection are expected outcomes,
not failures hidden from the report. Retried interrupted writes use their
original operation IDs and reconcile to one record each. Each run recovered
10,000 queued creation notifications after the injected delivery failure.
Neither MX nor N1 logged an unexpected error.

Across these two runs, the slowest read was **2.29 s** and the slowest ordinary
write **22.20 s**. The 100-file bursts took **122.60 s** and **83.19 s**, respectively.
This is not a low-latency SLA: the shared HDD can still make saves and notification
delivery slow during bursts. Peak measured MX RSS was about **764 / 640 MiB**;
N1 about **53 / 55 MiB**. Post-run database inspection found zero pending
notification jobs in both runs; recovered-notification counts passed in both.

Hardware: Ryzen 5 PRO 4650U, 12 logical CPUs, approximately 15 GiB physical RAM,
one project HDD, Linux. The load generator ran on the same host using real
loopback HTTPS/WebSocket traffic, not 100 graphical browsers. This is not
qualification of a different production server, 1,000 active users, 100 Office
renderers, physical power loss or actual Windows desktop banners.

Private evidence directories:

- `/mnt/PROJECT-DISK-1TB/tests/mx-concurrency-MuHtoy/`
- `/mnt/PROJECT-DISK-1TB/tests/mx-concurrency-cIz1V2/`

Earlier passes/failures below are diagnostic history, not substitutes for these
final-binary runs. The 1,000-user overload attempt failed browser deadlines;
**do not advertise 1,000-user capacity**. It used the preceding candidate before
the storage-transfer budget; the final build has no successful 1,000-user result.
Passing a bounded test is not proof of universal absence of bugs. The user has
since authorized RC.10 publication, subject to the final release checks above.

### Pre-versioning regression results

- Native: **235 passed, zero failed, two explicitly ignored**.
- Frontend: **385 passed in 62 files**, rerun after the frontend correction.
- Production frontend build: release metadata, bundled Office checksums and
  built assets verified; **zero Svelte errors/warnings**. Vite retains its
  existing large-chunk advisory; this is not a warning-free bundler claim.
- Chromium, Brave, Firefox and Linux WebKit: notification focus/dedup/reconnect,
  workflow/configuration, journals/stock and administrative draft-preservation
  checks passed. Advanced relationships/privacy/import checks passed too.
- Collaboration and record attachment Office tests passed on the corrected
  frontend. Actual DOCX/XLSX rendering passed in Chromium, Brave and Firefox;
  Linux WebKit's known missing worker-WebGL support was verified as an explicit
  fallback with unchanged original downloads, not a rendering pass.

Browser testing exposed another real reliability defect: background resync
recreated the Administration panel and discarded the selected module and its
unsaved configuration. A forced reconnect reproduced it in Chromium, independent
of the original WebKit failure. The frontend now separates explicit Refresh
from passive reconnect/automatic refresh. Editor panels retain their state;
read-only audit/trash lists and update status can still refresh in the background.
Both an actual ten-second refresh tick and a forced live reconnect preserved the
draft and its confirmation in all four engines; saving through normal server
revision checks and explicit Refresh also passed. Source changes are in the
original repository; the rebuilt frontend is on the project HDD only.

Corrected frontend build directory:
`/mnt/PROJECT-DISK-1TB/build/mx-concurrency-frontend.m4Nxff/frontend/dist`.
Its `index.html` SHA-256 is
`4c57658a51aac89a91e49264751a7b7f9de205c644fa06dfadb50b8918856668`;
`assets/main-yhr_iZm1.js` SHA-256 is
`385a8cb17fe6be1a944609d780c5a3580ac7fa46aae8aff40acc7971ea4aa18d`.
The API load runs do not render the frontend; these subsequent browser tests
qualify the corrected UI against the same tested server binary.

Final private browser evidence (`/mnt/PROJECT-DISK-1TB/tests/`):

| Directory | Suite |
| --- | --- |
| `mx-eight-user-1hl33j` | Notification focus, duplicate prevention and reconnect |
| `mx-eight-user-JPAgxL` | CRM/ERP/accounting process UI and decision history |
| `mx-eight-user-uDgz9M` | Exact journals, stock and lost-acknowledgement recovery |
| `mx-eight-user-WLWssV` | Automatic refresh/reconnect preserve admin draft; save/explicit Refresh |
| `mx-eight-user-z8AZOq` | Relationships, numeric choices, totals, privacy and imports |
| `mx-eight-user-mSoYPo` | Collaboration paste/drop/files and Office |
| `mx-eight-user-tvuCJ6` | Record attachment permissions and Office |

The earlier failing UI attempt is retained at `mx-eight-user-Yk7L7E`; the
deterministic pre-fix reconnect reproduction at `mx-eight-user-SkCxd5`.

## Acceptance gate

The minimum target is 100 distinct authenticated accounts performing concurrent
work, with live connections and the real, latest N1 release build. A separate
1,000-user stress run characterizes overload; it does not extend the supported
capacity automatically. Neither test establishes the absence of all bugs.

Required evidence includes:

- No unexpected failed record creates, independent edits or reads; expected
  stale-field and insufficient-stock conflicts are checked separately.
- Unique identifiers and sequence allocation, payload-bound replay without
  duplicate records/messages, correct formulas and exact ledger/stock balances.
- Current row/field/module permissions enforced under load, including live
  transport, notification preference handling and permission changes.
- Durable notification counts, complete record history and audit evidence;
  background failures must not be hidden by successful HTTP responses.
- Byte-identical real N1 file round trips and partial reads, recovery of
  acknowledged writes after process crashes and SQLite integrity checks.
- Concurrent import/read/write behavior on an existing large dataset, sustained
  workload and reconnect recovery. Record latency distributions and resource
  use, not only average throughput or the number of connected idle sockets.
- Final optimized-build repetitions and the native/frontend/browser regression
  suites after patches. Failed attempts remain diagnostic evidence, not passes.

The test machine has one HDD. This does not qualify independent-disk redundancy,
physical power loss, every organization's hardware, 100 Office/WebRTC renderers,
or a specific Windows/browser notification environment.

## Isolation and reproduction

Keep source changes in the original repository. Build targets, Cargo caches,
temporary files, disposable accounts/databases, N1 pool and reports must be on
the project HDD. Do not reuse deployment configuration, account credentials,
fragments or the populated preview database. The harness binds owned services to
loopback, creates a fresh 1+0 N1 pool, uses strict N1 upload durability and keeps
the engine's configured rate policy. No N1 source modification is required.

Build the selected MX and N1 sources with `cargo build --release --locked` using
HDD `CARGO_HOME`, `CARGO_TARGET_DIR` and `TMPDIR`. Use Node 24 and the installed
frontend development dependencies. For example, from the MX repository:

```bash
MX_TEST_ARTIFACTS=/mnt/PROJECT-DISK-1TB/tests \
TMPDIR=/mnt/PROJECT-DISK-1TB/build/tmp \
MX_TEST_BINARY=/mnt/PROJECT-DISK-1TB/build/target/release/litiaina-mx \
MX_TEST_STATIC_SITE_PATH="$PWD/frontend/dist" \
N1_TEST_BINARY=/mnt/PROJECT-DISK-1TB/Development/N1/target/release/litiaina-n1 \
MX_LOAD_USERS=100 MX_LOAD_ROUNDS=3 MX_LOAD_SEED_ROWS=10000 \
MX_LOAD_IMPORT_ROWS=1000 MX_LOAD_SOAK_SECONDS=120 \
node --no-warnings frontend/tests/concurrency.integration.mjs
```

`N1_TEST_SOURCE` can identify the N1 source checkout; the default is a sibling
`N1` directory. `MX_LOAD_USERS` supports up to 1,000. Seed rows are explicitly
synthetic fixture preparation, not API throughput. Virtual users use real HTTP
and WebSocket protocols, not hundreds of graphical browser instances.

Each run prints its private evidence directory. `results.json` identifies binary
hashes, scope, phases, failures, timings and process RSS/thread/descriptor samples;
`http-samples.json` retains per-request status and duration. Runtime databases,
test secrets, TLS keys and detailed private reports must not be published.

## Diagnostic history (earlier candidates, not current release status)

The following chronological findings retain the state of each earlier attempt,
including gates that were pending at that point. Use the release checks and
current result sections above for the final status.

### Initial findings

Published RC.9 failed the first 100-user burst with a normal PATCH returning
HTTP 500 (`database is locked`); an audit write also failed. Creation and update
notification fan-out competed through individual recipient autocommits.
Batching fan-out alone was insufficient: the larger-fixture rerun still lost
four ordinary creates to SQLite busy timeouts. Raising timeouts or retrying all
writes would not prove correctness or fix unfair write admission.

Local hardening now adds a FIFO mutation lane before connection checkout while
leaving pure reads concurrent, and batches record/channel notification fan-out.
This remains **unqualified until the full acceptance gate passes**. Versions,
release assets and production services are unchanged.

Subsequent runs found a second failure mode: fair admission alone prevented
lock errors but requests still exceeded 45 seconds behind notification work.
Record/message notification jobs are now committed with the original mutation
and recovered by a batched outbox worker; queued writes recheck current grants.
The importer now returns committed progress on a time budget instead of waiting
for twenty separately admitted rows regardless of load.

The local implementation uses one dedicated, FIFO-admitted database writer
(16 MiB bounded page cache) in addition to the configured reader pool. It does
not serialize pure read handlers, turn off audit/history/permissions, or weaken
SQLite's existing synchronous setting. Initial diagnostics used a 45-second
ordinary request deadline. Subsequent verification uses the production browser's
15-second read and 30-second write deadlines, without automatic client retries.
File uploads use 180 seconds, below the production browser's existing
30-minute upload deadline; they are not treated as ordinary JSON requests.
Private live events are routed to the recipient account's connections instead
of being copied/scanned by every unrelated socket. These changes require final
optimized-build qualification and regressions; a passing intermediate phase is
not a release approval.

### Storage findings and explicit test configuration

A complete 100-user run passed record/chat/import/process/book/privacy phases
and a five-minute soak, then failed simultaneous file uploads: N1's adaptive
disk admission queue exceeded its default 30-second budget on the shared HDD.
This is a measured configuration/hardware capacity limit, not evidence of an N1
source bug. MX incorrectly reported the upstream 503 as a generic 502; local
hardening preserves temporary upload statuses and `Retry-After` without blindly
replaying one-shot writes.

The optional `hdd-100` fixture profile keeps adaptive admission and rate limiting
enabled, with `foreground_wait_timeout_ms=120000`, view rate/burst 1000/3000 and
global rate/burst 2000/5000. The view policy accounts for all MX users sharing one
trusted backend connection/IP. This is **not N1's default configuration**, does
not make the disk faster and must not be silently applied to a public service.
Default-policy failures are retained. No production N1 configuration was changed.

A separate two-user reproduction exposed a same-filename upload race: the losing
attempt could soft-delete the shared object behind the winning attachment.
New record attachments now use a unique internal object namespace per attempt;
filename uniqueness is rechecked in the metadata transaction. Old keys/bytes are
not rewritten. The focused reproduction now preserves the winner's exact bytes,
including subsequent MX/N1 process-crash recovery. That two-user pass alone does
not qualify the 100-user release gate.

The explicit longer-admission profile exposed a separate MX transport mismatch:
reqwest 0.12.28 sets Linux `TCP_USER_TIMEOUT` to 30 seconds by default. Queued
uploads can encounter a closed receive window while N1 waits for disk admission;
the resulting OS error 110 terminated MX's request before its 300-second upload
deadline. MX now leaves that socket option at the OS policy, retaining its
10-second connection, 60-second read-idle and per-operation total deadlines.
This is not a blind retry, an N1 source change, or proof that slower uploads meet
an acceptable latency target. The correction still requires load qualification.

### Authentication memory findings

The original shared blocking executor spread memory-hard password verification
across many threads. In the longer 100-user diagnostic, MX retained about
1.66 GiB RSS. A dedicated eight-worker password executor now bounds concurrent
login verification; a focused 100-user diagnostic peaked at about 461 MiB and
completed its 100 login/refresh pairs in under one second on this machine.
These are different-length diagnostic workloads, not an identical end-to-end
benchmark. Account provisioning/password changes are not all routed through
that executor. The full target and overload reruns remain required.

## Regression evidence in progress

Before the latest transfer-budget addition, the native suite reported
**232 passed, zero failed, two explicitly
ignored**. Frontend unit tests report **385 passed**. Focus-aware notification
browser checks passed in Chromium, Firefox and WebKit against the local patched
server. These tests simulate the desktop API and do not certify actual OS banner
delivery. CRM/ERP workflow, accounting/stock, advanced relationships and privacy
regressions also passed. Collaboration Office rendering passed in Chromium and
Firefox; this host's Linux WebKit explicitly falls back because it lacks worker
WebGL. Its fallback and unchanged download were checked separately, not counted
as rendering. The first WebKit test invocation omitted that known-host option
and failed; the failure was retained.

## First complete target run (verification still open)

The final local executable, SHA-256
`3a1535710037699d5ed2389666efb24ac7264055abad3d830385ed63a2f23c70`,
completed the 100-account run in about 912 seconds with **78,972 HTTP requests**
and 100 real live connections. N1 was built from commit
`90f436ff54a6b67fe892235cdbd6424503eca353`, binary SHA-256
`c7b1e4ded7194073ad85b90c84966244854022c8f98da8a00fb04163129fcd5f`.
The explicit `hdd-100` profile above was used; this is not a default-policy pass.

| Scenario | Result |
| --- | --- |
| 10,000 existing records, three simultaneous create/edit/read rounds | 300 new records, 300 independent edits; unique sequences and correct formulas |
| Replay and intentional same-field collision | No duplicate rows; exactly one winner and 99 expected conflicts |
| Collaboration | 100 messages, 9,900 recipient notifications, no duplicate replay rows |
| Import with 100 active readers/editors | 1,000 imported, zero failed rows; 47,843 successful requests; maximum 6.86 s |
| Workflow / books | 100 workflow actions, 100 balanced journals, 50 stock reservations and 50 expected insufficient-stock conflicts; no overdraw |
| Privacy / changed permissions | Creator isolation, raw-live filtering, revoked reads/writes and grant revocation while a write waits passed |
| Five-minute sustained workload | 26,091 successful requests, 2,436 edits; p99 4.25 s, maximum 7.41 s |
| Real N1 files | 100 simultaneous 256 KiB uploads, exact downloads and range bytes; burst completed in 64.83 s |
| Same-name collision | One committed attachment; the loser did not replace/delete its bytes |
| Process failures | Acknowledged records/files survived forced MX/N1 restarts; 100 interrupted operations reconciled exactly once and 10,000 notifications recovered |
| Integrity / history / audit | SQLite integrity OK, no foreign-key violations or hidden server errors; record history and creation audit checks passed |

This run used the older 45-second harness deadline, but inspection of every
successful ordinary request found **zero exceeding the actual 15/30-second
browser deadlines**. Maximum ordinary response time was 9.64 seconds. Expected
stale-field/stock/access denials and deliberately severed crash-test connections
are not counted as unexpected failures. All other phases had no failed request.
Peak measured MX RSS was about 950 MiB; N1 about 60 MiB. The notification backlog
grew under import load, subsequently drained, and was zero at the last sample.

Private raw evidence:
`/mnt/PROJECT-DISK-1TB/tests/mx-concurrency-D7sxcb/results.json` and
`http-samples.json`. A repeat with actual browser deadlines, separate 1,000-user
overload characterization and final regression checks remain pending. This is
not release approval, a universal hardware SLA, a 1,000-user capacity claim or
evidence of physical power-loss durability. Source versions remain RC.9.

## 1,000-user overload result — NOT qualified

The same executable passed 1,000 concurrent password logins/refreshes (9.26 s)
and established 1,000 live connections. The first mixed-work burst then failed:
895 POSTs exceeded the real 30-second browser deadline and 11 GETs exceeded
15 seconds. No automatic retries were used. This hardware/configuration is
**not qualified for 1,000 active users**; a connected socket is not capacity.

All 105 acknowledged creates were still present; SQLite integrity was OK with
zero foreign-key violations and an empty server error log. Inspection after
shutdown found all 1,000 create rows committed, despite only 105 reaching the
client before its deadline, and 1,054 retained notification jobs. Thus a timeout
is an **unknown outcome**, not evidence that nothing was saved. Stable operation
IDs and reconciliation remain essential; blindly creating again risks a second
business record. This run did not complete its later workflow/file phases and
must not be described as a 1,000-user pass or complete recovery qualification.
Peak MX RSS was about 607 MiB; the failure was deadlines under offered load,
not a memory-ceiling trigger or an observed SQLite locking error.

Private evidence: `/mnt/PROJECT-DISK-1TB/tests/mx-concurrency-2Oo2F2/`.
The 100-user target is being repeated separately, with 15/30-second deadlines.

## Repeat failure and active-transfer budget

The stricter repeat passed record/message/process/books/privacy phases and its
one-minute sustained workload, but simultaneous files again failed: several
upstream requests exceeded MX's 60-second read-idle timer. The earlier full pass
was therefore **not repeatable qualification**, and does not open the release
gate. All 300 acknowledged record creates remained present, with clean integrity
and no pending notification jobs in that failure snapshot.

Rather than lengthening every timeout again, local hardening now limits active
storage transfers before disk-spooled module/chat files are read into part
buffers or assigned upstream sockets. The shared budget defaults to eight and
is operator-configurable with `[n1] max_concurrent_uploads` (1–64). Drive batch
and durable part/finalize requests share the budget. It is not a bandwidth or
per-user rate cap; N1's configured policy is unchanged. Cancellation drops the
async waiter; a five-minute admission wait returns a truthful retryable 503.
This latest implementation still requires native and final-binary load reruns.

Repeat failure evidence:
`/mnt/PROJECT-DISK-1TB/tests/mx-concurrency-zH3NH9/`.
