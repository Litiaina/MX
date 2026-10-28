# Concurrent-user qualification

## Final RC.11 package — 28 October 2026: PASS ON THE STATED SSD PROFILE

The final `1.0.0-rc.11` executable is
`b20dfe9f1d712e449cc0b48da4ae2fe407fbb90141a4f8a0e680682191e87099`.
Runtime Rust source differs from the reviewed build below only in the Cargo
package version; final frontend assets are byte-identical. N1 remains clean
commit `f1e78c58963b28efb6ae382fe95b1b4d61cc8008`, executable
`ed24827830674306debd09e5cc834bc2a92d38a50d0e1fa3e333479eea2197da`.

The exact final executable completes 117,134 HTTP requests with 100 real
password logins/refreshes and live clients. Three record rounds, a 1,000-row
import with active readers/editors, workflow/journal/stock/privacy checks and a
60-second soak pass. The 602.723-second mixed phase has zero failed measured
operations: 7,421 creates, 7,293 edits, 3,861 attachments, 2,420 Drive versions,
32,388 reads and 64 real Office saves. Uploaded/downloaded bytes each total
1,782,437,523 with matching hashes. Mixed latency p95 2.666 s, maximum 8.427 s;
this is a measured result, not an SLA. Peak MX/N1 RSS: 1,366,680 / 91,968 KiB.

After killing/restarting MX and N1, all acknowledged snapshots and bytes verify:
300 initial and 1,000 imported snapshots, every mixed create, 20 final edits,
all 3,861 attachments, 2,420 historical Drive versions and 64 actual Office
versions. All 100 read watermarks and live reconnects survive. The later injected
crash interrupts 100 transports; each reconciles exactly once and all 10,000
delayed notifications recover within that phase's 7.208 s. Final hidden-error,
audit/history and SQLite/FK checks pass. Expected injected transport failures and
intentional permission/constraint denials are not described as ordinary successes.

Host/profile remains the documented i5-12600K, 16 logical CPUs, 31 GiB RAM,
XFS SSD, same-host generator, owned N1 1+0 pool and explicit `backend-100`
admission profile (limiter enabled: view 1,000/s burst 3,000; global 2,000/s
burst 5,000; foreground wait 120 s). MX read/write/upload deadlines, transfer
budgets and FULL-synchronous commits are unchanged. MX's existing Drive client
requests batched parts; do not infer individual-part fsync or physical-power-
cut/EC-failure certification from the process-crash result.

The final publisher-signed RC.10-to-RC.11 package also passes activation, final
backup, exact-data/file preservation, deliberate failed-activation paired
rollback and killed-worker recovery, with four-browser update UI. Native/unit
counts are 252/408, with two opt-in native tests ignored and no frontend
diagnostics. Separate final-build theme/loading, 10,623-record preservation,
notification receipt migration and false-uniqueness regressions pass.

Evidence: `/media/altear/TEST-1TB/mx-tests/rc11-release-20261028.YpiRMK/qualification.json`,
`final-ssd-proof/results.json` beside it, and remote
`/mnt/DATA_SSD/mx-rc11-final-20261028.GJUYTP/final-100-01/mx-concurrency-ijq6RL`.
Initial fixture-only zero-wait SQL/immutable-copy failures and their corrections
are retained there and explained in [TESTING.md](../TESTING.md).

Limits accepted for this RC remain explicit: the heavily styled workbook still
takes about 80–83 seconds (fails the 60-second target); the shared USB rotational-
HDD mixed profile remains unqualified. This is 100 API workers with one real
graphical Office witness, not 100 graphical editors, 1,000-user capacity or a
guarantee for arbitrary hardware, business schemas or physical power cuts.

## RC.11 reviewed-source gate — 28 October 2026: SSD PROFILE PASSED

Historical status/hold statements in the retained earlier attempts below refer
to those exact attempts. They are not the final RC.11 publication status.

Do not use the historical RC.10 pass below to authorize RC.11. The expanded
scenario now overlaps independent record creators/editors, random attachment
uploads/downloads, Drive document-version commits, readers and 100 authenticated
live connections. An additional real browser types into an Office document,
saves it and verifies the downloaded document contains the actual typed text.
That witness is **one graphical editor, not 100 graphical Office instances**.
The workload has 100 API accounts, not just 100 idle network connections.

### Current local repair — complete 100-account SSD scenario PASS

Executable `04e0b2605b043150cea31fc13e441200c5a44943790afdf622b76687bb273998`
passes the complete extended scenario: 115,767 HTTP requests, 100 authenticated
accounts/live connections, 1,000 imports under active readers/editors, a
60-second soak and a 603.694-second mixed phase. There are zero failed measured
mixed operations. Mixed p95 is 2.79 s, maximum 8.18 s. Mixed totals:

- 7,328 record creates, 7,190 edits and 32,086 reads.
- 3,788 attachments and 2,375 Drive versions, 1,746,720,285 bytes each uploaded
  and downloaded with matching hashes.
- 63 real Office saves overlapping the workload; all typed edits verified.
  Save confirmations: 4.82–12.87 s, p95 12.35 s. One real editor, not 100.

Forced MX/N1 restart verified all 300 initial and 1,000 imported snapshots,
all mixed creates, final edits, attachment/history/Office hashes, read watermarks
and 100 live reconnects. The later deliberately interrupted 100 writes reconcile
exactly once and all 10,000 delayed notifications recover. Final hidden-server-
error/history checks pass; SQLite integrity is `ok`, zero foreign-key violations.
Peak observed MX RSS is 1,429,744 KiB, N1 93,952 KiB; sampled outbox peak 245.
The separate copied-history regression below also passes; no previous executable's
capacity result is substituted for this run.

252 native tests and 408 frontend unit tests pass. A copy of the failed
808,646-notification fixture reproduces HTTP 500 on the old executable; the
repair passes three 100-reader rounds in 2.67–2.75 seconds each, with zero
retries/errors. A 100-account empty-unread burst takes 0.16 seconds. This focused
regression uses fixture-signed JWTs, not password-login or mixed storage traffic.
The independent full scenario uses real login/refresh and the unchanged frozen
N1 binary, FULL-synchronous MX commits, transfer caps and request deadlines.
Hardware: i5-12600K, 16 logical CPUs, 31 GiB RAM, XFS SSD; generator on the same
host over loopback. A shared rotational-HDD profile remains unqualified, and
the original workbook's 60-second opening target remains failed. The maintainer
accepted these disclosed limits and the populated local preview, then authorized
RC.11 publication subject to final versioned-package and updater checks. The
hash above is the reviewed RC.10-labelled build; final RC.11 artifact identity
and packaging checks are recorded separately, not silently substituted here.
N1 uses an owned pool A with 1 data + 0 parity on that SSD, not the operator's
existing storage images. Strict upload durability is the engine default; MX's
existing Drive protocol requests batched parts. This is process-restart/hash
verification, not proof of physical power loss, EC redundancy or every part
being individually fsynced before its acknowledgement.

Standalone Office idle/durable suites also pass on Chromium, Brave and Firefox.
They cover unfinished cell input, scroll/drag/composition deferral, automatic and
queued manual saves, preserved focus, closing a tab mid-upload, lost final save
acknowledgements, offline recovery and explicit local-storage failure feedback.
Product-theme checks pass on Chromium/Firefox/WebKit, including 200% text reflow.
These are disposable fixture/browser checks, not additional live-N1 capacity
runs or physical-power-loss/Safari certification.

Evidence: `/mnt/DATA_SSD/mx-rc11-inbox-20261028.pGU99b/inbox-before` and
`candidate-inbox-01`; native/frontend logs are under
`/media/altear/TEST-1TB/mx-tests/rc11-candidate8-20261028.2t9rKo`.
Full proof: remote `candidate-extended-100-01/mx-concurrency-uwQ1Tv/results.json`
and the local `ssd-extended-proof/results.json` copy.

### Extended-repeat failure and follow-up repair

The ten-minute repeat of `775c6dff…a4787` passed the measured mixed phase and
forced MX/N1 restart checks but **failed the final concurrent inbox read burst**
with HTTP 500. No tracked acknowledgement was missing: all 8,328 tracked rows,
3,518 attachment hashes, 2,157 historical Drive versions and 65 actual Office
saves survived. The retained fixture contains 808,646 notification rows.
Sorting each account's whole inbox occupied the read pool long enough for an
authentication connection checkout to fail at its unchanged ten-second limit.
The follow-up local repair uses indexed ordering, scalar unread boundaries and
per-query module-grant sets. Its independent passing regression and full repeat
are reported in the current-candidate section above.

The original workbook remains over budget: 82.7 s cold and 80.1 s warm on the
home PC, versus the unchanged 60-second target. No workbook data or formatting
was removed. RC.11 remains on hold.

Evidence: `/mnt/DATA_SSD/mx-rc11-balanced-20261028.Ieepvu/candidate-extended-100-01`
and `candidate-workbook-01` in the same directory.

### Earlier candidate: expanded SSD pass, HDD mixed failure

Current build `775c6dff7feb7aea7fc18800e59be8ed8a7e220d70fd6508a90c6275482a4787`
passed the complete 100-account home-SSD run: 64,686 requests and a five-minute
mixed phase with zero failed measured operations. Mixed totals: 3,873 creates,
3,774 edits, 2,017 attachments, 1,273 Drive versions, 13,036 reads and about
934 MB of byte-verified upload/download data. Mixed p95 request time was 2.45 s,
maximum 6.43 s. One real Office editor made 37 verified saves (3.31–11.37 s).
All 300 initial/1,000 imported snapshots, mixed creates, file/history hashes,
Office versions, account watermarks and live reconnects passed forced MX/N1
restart verification. All 10,000 crash-delayed notifications recovered in 10.88 s;
sampled outbox peak 243. Final hidden-error/history/integrity checks passed.

The same build failed the shared rotational-HDD mixed workload. Its preliminary
record bursts, Collaboration, receipt edits, 1,000 imports (254.75 s), books,
privacy and read/write soak passed. Simultaneous uploads then pushed record
create/patch and Drive initialization beyond their unchanged 30-second deadlines.
All 1,300 tracked acknowledged records remained present and SQLite/FK checks
were clean; later full restart/hash gates were not completed. This profile is
**not qualified**. The SSD pass must not be presented as an arbitrary-disk or
arbitrary-configuration capacity guarantee.

Evidence: `/mnt/DATA_SSD/mx-rc11-balanced-20261028.Ieepvu/candidate-full-100-01`
and `/media/altear/TEST-1TB/mx-tests/rc11-candidate7-20261028.uguhG0/candidate-full-100-01`.
Current source passes 250 native tests (zero failed, two opt-in ignored), plus
Administration/Collaboration/admission/audit and graphical smoke controls. The
ten-minute mixed SSD repeat and final browser/rendering checks remain in
progress. RC.11 is still on hold; the workbook opening target remains unresolved.

### First complete expanded pass — scoped to the home SSD build

Build `b8b65fb84554fd3765042d70aad07fc06276e52b0e9f16c170d5e32959c621ff`
passed the complete expanded 100-account scenario on the home i5-12600K,
31 GiB RAM / XFS SSD profile. It used the same frozen patched N1 binary and
unchanged FULL-synchronous MX commits, transfer caps and request deadlines.
The load generator ran on that same host over loopback HTTPS/WebSockets; this
is not 100 graphical Office instances or a physical power-cut certification.

- 66,521 HTTP requests, 101 peak in flight, 100 independent live connections.
- 1,000 imports with active readers/editors, followed by a 60-second soak and
  303-second mixed phase with zero failed measured operations.
- Mixed: 3,886 creates, 3,783 edits, 2,149 attachments, 1,335 Drive commits,
  13,267 reads and 993 MB of byte-verified uploads/downloads.
- One real Office editor: 39 byte-verified saves; confirmations 1.79–9.35 seconds.
- Forced restart: exact 300 initial/1,000 imported snapshots, every mixed create,
  attachment/hash/history version and Office version, account watermarks and
  100 reconnected live streams verified. SQLite integrity/FK checks passed.
- Interrupted writes reconciled exactly once; all 10,000 delayed notifications
  recovered in 10.41 seconds. Sampled outbox peak 80, versus about 14,800 in the
  preceding failed run. Final hidden-error and history/audit checks passed.

Evidence: `/mnt/DATA_SSD/mx-rc11-fair-20261028.wYBSUw/candidate-full-100-01`.
The same executable failed on the ext4 rotational HDD: 36 creates exceeded
30 seconds in its first record burst. No acknowledged row was lost. One-for-one
delivery turns added too much foreground latency on that device. **This SSD pass
is not an HDD pass, universal capacity guarantee or authorization to release.**

The current source instead allows up to four FIFO foreground turns between
waiting bounded delivery slices, preserving progress for both lanes without
increasing transaction limits or deadlines. All 250 native tests pass; that
changed executable needs its own optimized qualification on both machines.
Large-workbook opening and other RC.11 gates remain unresolved.

### Latest home SSD run — mixed/restart passed, full recovery FAILED

Candidate `7f72381b…d0d56`, with the unchanged patched N1 binary, ran on the
authorized home SSD. The five-minute mixed phase completed with zero failed
measured operations: 7,073 creates, 7,011 edits, 4,357 attachments, 2,946 Drive
versions and 16,196 reads. Roughly 2.01 GB of uploaded bytes were downloaded and
verified. One real Office editor made 55 overlapping byte-verified saves, with
confirmations of 2.3–5.4 seconds. Forced MX/N1 restart checks verified all tracked
record acknowledgements, 300 initial record snapshots, every mixed attachment
and historical version hash, all 55 Office versions, 100 account read watermarks
and 100 reconnected live streams. SQLite integrity/FK checks passed.

The final interrupted-write/notification gate **failed** its existing 120-second
deadline: 3,140 outbox jobs remained. Notification debt had grown to about 14,800
jobs during the burst, as each bounded delivery turn queued behind the entire
foreground queue again. No tracked record acknowledgement was missing; no
unexpected server error was found. This remains a failed full qualification,
not a successful capacity release or an N1 failure. Private evidence is under
`/mnt/DATA_SSD/mx-rc11-verify-20261028.ghkjFH/candidate-full-100-01`.

The next local patch fairly interleaves bounded delivery turns with FIFO
foreground writes. It does not increase slice/transfer limits, extend deadlines,
discard notifications, weaken durability or modify N1. Native checks pass
**250 tests**, zero failed, two existing opt-in ignored. A new deterministic
queue test covers both lane orders and prevents starvation. Fresh optimized
full qualification remains required. The recovery harness additionally verifies
all imported records against expected values and exact pre-crash snapshots.

Candidate `7f72381b…d0d56` also passed Administration/Collaboration contention,
upload admission, audit persistence and legacy receipt migration controls.
Drive, sharing, recovery and import browser suites passed Chromium, Firefox and
WebKit. The initial recovery-browser failure exposed a test race: bytes stored
by N1 are not yet an HTTP acknowledgement observed by the browser. The corrected
scenario waits for the actual browser acknowledgement before disconnecting,
retaining original deadlines and byte/deduplication assertions. The failed run
is retained; no application code was changed for that harness synchronization.

### Local MX concurrency repairs — verification in progress

The first repaired candidate (`883bb160…40baed4`) passed three 100-account
record rounds, replay/conflict checks and Collaboration, but **failed** the new
notification-receipt/edit phase at the unchanged 30-second write deadline.
Its failure is retained under
`/mnt/PROJECT-DISK-1TB/tests/mx-rc11-concurrency-fix-20261028.xBSUz2/full-100-01`.
This does not override the earlier mixed-load failure below.

Safe operation-name telemetry separated FIFO wait time from SQLite execution.
Notification read-all was rewriting hundreds of payload/index pages per account;
a diagnostic on independent copies of the failed 70,200-notification database
recorded roughly 3 MiB of WAL writes per account acknowledgement. Moving an I/O
cost does not remove it or prove capacity. The next local candidate therefore:

- Shares the record writer's FIFO with Drive metadata and notification receipts;
  keeps N1 network I/O outside database turns and transfer slots away from
  metadata waits. The eight-transfer cap and ten-second admission wait remain.
- Delivers notification jobs in soft bounded slices **between whole activities**.
  Recipient inserts and outbox retirement still commit together, with current
  permissions/preferences checked inside the transaction.
- Stores a read-all watermark per account, rather than rewriting every inbox
  row. The API combines it with existing individual read times; new deliveries
  are newer than the watermark even within the same millisecond or after a clock
  correction. Existing notification payloads are not rewritten by read-all.
- Performs passive checkpoints on a dedicated connection/thread, retrying pinned
  work and falling back to automatic checkpoints if the worker fails. Foreground
  WAL commits use **FULL synchronous**, stronger than the earlier NORMAL policy;
  acknowledgement does not depend on a later checkpoint to issue its WAL sync.
  Storage must still honor sync requests. A pinned reader can retain WAL pages;
  the retention-size setting is not a hard bound during active reads.

The earlier native candidate checks passed **246 tests, zero failed, two existing
opt-in tests ignored**. New tests cover read-all rollback, constant receipt writes,
account isolation, same-millisecond/new deliveries and checkpoint progress with
an independent active reader. Legacy-upgrade, browser and full mixed-load results
must also be recorded before qualification. All builds/evidence stay on the HDD;
N1 source, production deployments, published releases and version remain unchanged.

The next FULL-synchronous build (`4d95bce5…e94fb55`) passed three 100-account
record rounds, replay/conflict checks, Collaboration and all 100 read-all/edit
checks. Its import phase was **intentionally aborted for inadequate throughput**:
19/1,000 rows committed in 283.97 seconds. SIGTERM was sent only to the owned
test MX child; subsequent ECONNREFUSED errors in raw results are caused by that
intentional stop, not a newly observed spontaneous crash. No later mixed/recovery
phase passed. See `full-v2-100-01/ABORT-NOTE.md`; raw results remain unchanged.

The successor uses one bounded transactional import slice (up to 20 rows/one
second of execution after writer admission), with row savepoints and one durable
progress commit. Invalid rows are isolated; infrastructure faults roll back the
unacknowledged slice. Audit work uses a bounded 256-entry queue and transactions
of up to 32 events; every caller still awaits persistence confirmation. This
avoids separately queueing and syncing every row/progress/audit write. Native
checks now pass **249 tests, zero failed, two existing opt-in tests ignored**.
The optimized successor (`2e4dbde1…e213`) passed all preliminary 100-account
phases. All 1,000 imported rows succeeded in 62.82 seconds with 100 active
readers/editors. During the following mixed phase, 97 creates, 89 edits,
58 attachments, 31 Drive versions and 2,980 reads completed without unexpected
API errors. The run nevertheless **failed**: its real Office save did not
confirm within the unchanged 60-second budget. The 397 tracked acknowledged
record IDs were present and SQLite integrity/foreign keys were clean, but the
later full crash/hash recovery phases were not executed. No full pass is claimed.

A diagnostic recorded about two seconds of native Office export, under 250 ms
of local recovery persistence, 29.72 seconds for Drive initialization and 13.58
seconds for its part request. Finalization was still pending at the deadline.
These measurements identify the server upload path as the next investigation,
not proof of a new N1 defect or a completed save.

The next upstream-timing diagnostic failed earlier: MX channel creation returned
500 during notification delivery. Collaboration mutations still used independent
pooled writers. The subsequent local patch coordinates them with the shared
writer and uses atomic transactions for membership/history and pin/tombstone
changes. A fault-injection regression demonstrates that the old executable
committed membership despite returning an error. The new patch passes all
**249 native tests**, but still needs optimized regression/mixed qualification.

From the operator's latest path instruction, all new generated builds, test
databases/storage, browser profiles and evidence use `/media/altear/TEST-1TB`.
Historical failed runs remain on their original volume. No further attempts to
reach the remote SSD host are made. N1 source, production and published releases
remain untouched; the source repository stays `/home/altear/Development/mx`.

### Later local run and renewed home-host access

The Collaboration successor (`0405dda5…83774`) passed its atomic mutation,
permissions, direct-conversation race and restart regression. Admission/audit
checks and the complete five-account Office/mixed/restart control passed too.
The full 100-account run on the newly authorized ext4 HDD still **failed**:
records, Collaboration, receipt/edit checks, all 1,000 imports (119.84 seconds)
and operational books passed, but module visibility reconfiguration returned
HTTP 500 after an independent pooled writer exhausted its ten-second busy wait
during notification delivery. Mixed Office and full crash recovery were not
reached. See `candidate-full-100-01` under
`/media/altear/TEST-1TB/mx-tests/rc11-verification-20261028.AKHzJ4`.

The next patch coordinates module/field/preference mutations, keeps grant GETs
read-only, reconciles new Drive sessions without an extra writer turn, and fills
bounded audit batches after admission. Native tests pass **249/249**, with two
existing opt-in ignored; optimized integration and full qualification are pending.
The earlier small-run Chromium socket-path failure is retained as a test setup
failure, not a product pass or capacity result.

The operator has since restored access to the home test PC. This supersedes the
earlier instruction not to reconnect: tests on that PC use `/mnt/DATA_SSD` only.
Local generated artifacts remain under `/media/altear/TEST-1TB`; canonical source,
N1 source and published releases remain unchanged in location/authority.

### Pre-repair rerun with the user's patched N1: FAILED

The 28 October rerun rebuilt clean N1 source
`f1e78c58963b28efb6ae382fe95b1b4d61cc8008`, without modifying it. Its optimized
binary SHA-256 is
`ed24827830674306debd09e5cc834bc2a92d38a50d0e1fa3e333479eea2197da`.
The MX native hash remains `c215d74e343a38c30c3d8009c21b12c4b15aeda00092b4b4116c7fcff175763a`;
the Office frontend generation is `mx-office-assets-6cb536e0036c02ed`.

The larger SSD host was unreachable. This rerun used the explicitly authorized
external XFS HDD on a Ryzen 5 PRO 4650U / 12 logical CPU / 15 GiB RAM laptop.
The database, N1 1+0 control/member roots, browser data, executables, temporary
files and evidence were on that HDD. This is **not** a rerun on the larger SSD
host. The same explicit backend profile below was retained; durability and rate
limiting were not disabled, and failed measured writes were not retried.

The run completed 63,028 HTTP requests with 101 peak requests in flight and
100 independent authenticated live connections. Eleven preliminary phases
passed, including all 300 ordinary creates/300 edits, replay/conflict checks,
100 Collaboration messages, 100 balanced journals, isolation and current grant
checks. All 1,000 import rows succeeded while the 100 accounts completed 45,631
background read/edit iterations. That import phase took **562.9 seconds** on
this HDD. The following mixed phase failed after 77 seconds of its intended
300 seconds:

- Four record creates exceeded the unchanged 30-second request deadline.
- Four Drive part requests returned MX's own `429` / `Other transfers are busy`.
  This response comes from the eight-slot, ten-second wait in
  `src/api/drive/resumable.rs`; it is not evidence of an N1 rate-limiter defect.
- The original N1 pending-binding error did not recur; neither server log
  contained warning/error lines. The absence of that error in an abbreviated
  phase does **not** qualify the full N1/MX integration.
- Only 20 mixed attachments and 16 document versions completed round trips;
  about 20 MiB of random/document bytes were compared. The real Office browser
  opened its document but completed **zero** verified graphical saves before
  the load aborted. Do not call its empty edit-check loop a save pass.

All 336 explicitly tracked acknowledged record IDs were present; SQLite
integrity and foreign keys were clean both before and after owned processes
stopped. A separate read-only audit found 40 stored mixed records versus 36
acknowledged ones, consistent with committed writes whose responses timed out.
A timeout must be reconciled using the original operation ID, not assumed to
mean the save failed. The full restart/file-hash and interrupted-delivery phases
were **not reached**, so this run makes no post-crash zero-loss claim.

Overall p95 request latency was 4.00 seconds and p99 10.96 seconds. Peak sampled
MX RSS was 864,248 KiB, N1 RSS 53,808 KiB, and pending notification jobs 2,227.
These are throughput/queueing concerns even where correctness sub-phases pass.
Native MX/N1 application source, production services, versions and release assets
were not changed by this verification. RC.11 remains on hold, including the
separate workbook-rendering gate below.

Private evidence:
`/mnt/PROJECT-DISK-1TB/tests/mx-rc11-patched-n1-20261028.xkBeaP`.
See `candidate-provenance.json`, `run-01-analysis.json`, `run-01.log`, and
`mx-concurrency-gHZY6Z/results.json` / `http-samples.json`. Frozen run-01 harness
inputs are retained; later diagnostic-harness changes do not rewrite this failure.

A separate **five-account diagnostic-harness regression**, not a capacity
qualification, subsequently passed. It verified all 97 mixed creates, 27 random
attachment hashes, 23 historical Drive version hashes and five real graphical
Office save hashes after SIGKILL/restart. Its credential-free acknowledgement
ledger was inspected. This validates the new inspection paths under a small
control workload; it does not override the failed 100-account run. Evidence:
`harness-regression/mx-concurrency-aKbNgh/results.json` and `acknowledged-data.json`
under the same private evidence root.

### Earlier larger-host attempts — retained failures

The lab is on an explicitly authorized SSD test volume, with 16 logical CPUs and
32 GiB RAM. Database, N1 control/member roots, temporary files, browser profiles,
binaries and evidence are under that volume. Neither production data nor N1
source was changed. N1 was rebuilt from clean tracked source
`16dd5c939f682bbc0f213c303a17cddc5e573dee`; the optimized executable SHA-256 is
`25f65e598f07a42910cdb1fe17839823ed38e75579ec636a4bd00fa62e486146`.
The MX native executable is unchanged from the local design candidate
`c215d74e343a38c30c3d8009c21b12c4b15aeda00092b4b4116c7fcff175763a`.

Two earlier failed mixed runs are retained:

1. N1 defaults: simultaneous file reads exceeded its configured 30/s view limit.
   MX returned truthful 429 responses. This is an admission/configuration result,
   not evidence of a SQLite fault or a successful capacity test.
2. Explicit backend profile: view 1,000/s (burst 3,000), global 2,000/s (burst
   5,000), foreground wait 120 s, **rate limiting still enabled**. A Drive upload
   initialization returned 502 while N1 logged `read pending upload binding` /
   `No such file or directory`. Inspection found an existence check followed by a
   fallible read in N1's pending-binding reader; another upload can remove that
   metadata during the scan. This result must be fixed/retested, not ignored,
   relabeled as a pass, or hidden behind benchmark retries.

The earlier record-only sub-phases did pass: simultaneous authentication,
idempotent writes, permission revocation/isolation, workflow/book commands, import
with active editors, and sustained reads/edits. In the first 180-second sustained
phase there were 28,577 HTTP requests, 2,632 edits and 320 ms p95 request latency.
The failed mixed run with the backend profile also verified about 655 MiB of
uploaded/downloaded bytes and actual Office saves. It remains a **failed run**:
other workers stop after an error, so later Office saves cannot establish the
full intended overlap. SQLite integrity and foreign keys remained clean, with no
missing acknowledged record IDs. There is no claim of physical power-cut testing.

### Reproduce on an authorized test volume

`frontend/tests/concurrency.integration.mjs` requires explicit optimized MX/N1
binaries, an isolated N1 fragment and HDD/SSD paths. It never uses deployment
credentials. Select `MX_TEST_DISK_ROOT=/mnt/PROJECT-DISK-1TB` (default) or the
explicitly authorized `/mnt/DATA_SSD`; put `TMPDIR`, `MX_TEST_ARTIFACTS`, binaries,
static-site files and browser cache/configuration below that same root.

Set `MX_LOAD_USERS=100`, `MX_LOAD_SEED_ROWS=10000`,
`MX_LOAD_IMPORT_ROWS=1000`, `MX_LOAD_MIXED_SECONDS=300`,
`MX_LOAD_RANDOM_FILES=1`, and `MX_LOAD_OFFICE_BROWSER` to a locally installed
Chromium-compatible browser. `N1_TEST_CAPACITY_PROFILE=backend-100` applies the
explicit lab profile above only to the disposable N1 configuration. It does not
edit any deployed N1 configuration or disable limiting. Save the full result,
per-request samples, exact binary hashes and both server logs, including failures.

### Separate client-rendering gate

`frontend/tests/office-performance.browser.mjs` opens an explicitly supplied
workbook with the real bundled Office engine and records actual loading stages,
resource timing and cold/warm readiness. It stores only private lab copies and
never modifies the supplied original. Its default responsiveness target is
60 seconds; failing that target prevents this performance gate from passing.
The test's isolated storage fixture deliberately has its injected first-part
failure disabled, so storage-fault injection is not confused with render timing.
It is not a live-N1 capacity test or proof of 100 client renderers.

The supplied workbook is about 8.7 MiB compressed / 58 MiB expanded with roughly
3.2 million cell entries. Before the window-layout adjustment it opened in about
119 s (Chromium), 109 s (Brave) and 166 s (Firefox) on the laptop. Most time was
inside native workbook import/layout, after transfer and engine startup. A hidden
native-window experiment reduced one laptop opening to about 107 s; this alone
does **not** satisfy the responsiveness target or establish a controlled speedup
percentage. The candidate retains the data, formulas and formatting; it does not
strip cells or silently change the workbook to make a benchmark pass.

The later SSD-host measurement with the hidden-window candidate was 77.205 s
cold and 65.078 s warm in Brave. Both still exceed the 60-second engineering
target. Rebuilding N1 does not establish a faster browser-native import/layout
result, so the client-rendering gate remains unresolved separately from the
latest mixed-user failure above.

## Prior RC.10 qualification

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
