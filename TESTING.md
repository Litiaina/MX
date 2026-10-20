# MX validation

## 1.0.0-rc.6 — Collaboration and record Office recovery

RC.6 packages the locally tested fixes after RC.5 as a newly numbered candidate,
with maintainer authorization on 2026-10-20. Published RC.5 is not replaced.
Native application behavior and N1 are unchanged; the MX executable and frontend
are rebuilt with matching RC.6 identities. No new schema migration is introduced.

- The RC.6 native rebuild passes **188 tests**, with two opt-in cases ignored.
  Native `--version` reports `1.0.0-rc.6`; runtime dependencies remain unchanged.
- Production frontend build passes with zero Svelte errors/warnings; **353 tests
  across 57 files pass**. The existing Vite large-chunk advisory remains.
- `test:collaboration:files` passes in Chromium, Brave, Firefox and Linux WebKit:
  multi-format clipboard files, duplicate prevention, native text paste/drag,
  file drops without browser navigation, nearby rejection feedback, per-chat
  drafts, actual upload/download through the isolated storage contract, and
  outsider denial. Conversation changes close old previews. Shared Drive cards
  open Office directly without bypassing viewer permissions or changing revisions.
- `test:records:office` passes in those four engines: stored module attachments
  open without server PDF conversion, preserve return context, and leave the
  record and history unchanged. Private attachment fields and inaccessible
  modules remain protected. Blocked popups retain Open/Download recovery; ordinary
  text previews still work.
- Actual bundled-engine Word/Excel rendering passes in Chromium, Brave and
  Firefox for chat and record attachments. **Linux WebKit on this host lacks
  worker WebGL**: its tested outcome is an explicit error and byte-exact original
  download, not successful Office rendering. Tests neither bypass the graphics
  gate nor download the large engine after this capability failure.
- The existing People selection regression passes again in Chromium, Firefox
  and WebKit, including fresh accounts, profile cards, search and light/dark themes.
- A cache regression confirms an older cache-first Office worker cannot serve
  its Drive-only shell for the new attachment entry. Attachment viewers do not
  create Drive identities, autosave, or retain private local/offline drafts.
- The final RC.6 build repeats both attachment suites in four engines and the
  People regression in three. Native CLI diagnostics create no deployment files.
  An isolated **RC.5 → RC.6** independent-systemd rehearsal passes signed staging,
  configurable automatic/manual policy, final backup, activation, competing
  install requests, post-migration rollback and killed-worker recovery. The
  Chromium Updates UI retains unsaved policy and reconciles a lost acknowledgement.
  This does not upgrade or restart an existing production or populated test service.
- Signed-package verification checks all **29 runtime files**, the existing
  publisher signature, paired binary/frontend identities, bundled Office notices
  and exclusion of deployment credentials/databases. The installer retains the
  existing README branding block and logo asset.

Repeat from `frontend` with the isolated test paths described below:

```sh
npm run test:collaboration:files
npm run test:records:office
npm run test:collaboration:people
```

Browser clipboard/drop tests dispatch real browser `DataTransfer` events; they
do **not** certify a physical Windows clipboard. Manual acceptance must include
copying files from the operator's actual desktop/Explorer and dropping files in
their browser. If that platform exposes no clipboard file bytes, use Attach or
drag/drop; MX does not fetch pasted local paths. Chat/record copies are read-only;
editing/versioned saves use the shared Drive item instead. Production, real OS
notification banners and every Office format are not qualified by these suites.

## 1.0.0-rc.5 — candidate qualification

This candidate is published with maintainer authorization on 2026-10-20, not
certified as stable 1.0.0. Publication does not deploy it to production servers.
Existing data and modules are preserved. Synthetic QA modules were added only
to the explicitly authorized local test, never to product seeds or packages.

### Coverage

- Native checks: **188 passed, two opt-in cases ignored**. Frontend: **346 passed
  across 56 files**. Production compilation, paired versions and bundled Office
  checksums pass; Svelte reports zero errors/warnings. Vite retains its large-chunk advisory;
  this is not a claim that initial-load bundle size has been optimized.
- Advanced real-HTTP tests cover combined names/searchable relationship labels,
  select codes with independent numeric values, filtered incoming totals,
  edit/cancel/trash/restore, and concurrent idempotent retries without double
  deductions. Invalid numeric reconfiguration rolls back rather than committing
  half-recalculated records. Cycles are rejected even in an empty module.
- Privacy checks use eight accounts and inspect direct reads/writes, pagination,
  keyset windows, search/filter/counts/summaries, exports, history, attachments,
  preview-ticket revocation, imports and raw live-event payloads. Creator and
  administrator restrictions are API policies, not just hidden UI controls.
  Required administrator-only fields do not block ordinary users' imports.
- Chromium, Firefox and Linux WebKit exercise the configuration builders,
  full-record editing, contextual sends, module-selection/dirty guards, personal
  bold fields, 200% text and mobile/light/dark layouts. Live privacy changes
  purge newly restricted dirty inputs, local drafts and cached record views.
  Drive/dropdown regressions also pass in all three engines. Targeted Chromium
  regressions cover importer corrections, conditional forms, column totals,
  archive errors and record-window scrolling; relationship auto-fill runs in
  all three engines.
- Field-error dialogs and in-place datatype conversion run in all three engines:
  errors stay visible in focused dialogs, keyboard focus is retained, archived
  keys are explained, concurrent creates have one winner, and widening Text to
  Long text preserves 800 imported records and their values/history exactly.
- Fresh-account Collaboration People regressions cover no selection before a
  chat exists, profile viewing, exactly one real selected chat, search and view
  switching in Chromium, Firefox and WebKit, in light/dark themes. The regression
  first reproduced the original failure (three highlighted rows instead of zero).
- The release pass repeats native/frontend checks, the optimized advanced-record
  HTTP suite and three-engine notification routing. A real RC.4 → RC.5 independent
  systemd-worker rehearsal verifies signed staging, configurable maintenance,
  activation, the final database backup, post-migration rollback, eight competing
  install requests and recovery after killing the activation worker. The Updates
  UI is checked in Chromium; this does not restart an existing deployment.
- A **100,000-linked-record fixture** verifies filtered balances, eight creator
  scopes, exact totals and bounded 50-row cursors. **24 concurrent HTTP reads**
  finish while an independent connection holds an uncommitted WAL writer.
  The optimized build's slowest measured request was **7,008 ms** on this local
  fixture. This is a correctness/contention check, not an ingestion benchmark
  or a promise of subsecond response at every scale.
- Migration rehearsal copies the existing test database before starting the
  candidate. **10,563 records, four modules and 14 Drive items** survive startup,
  authenticated reads and restart. All **17 business-table content hashes**
  remain unchanged; 10,563 creator-provenance rows are added, and SQLite integrity
  and foreign-key checks pass. The original database is opened read-only for
  that rehearsal.
- Notification tests instrument browser delivery rather than claiming a visible
  OS banner. They exercise foreground suppression, background/minimized routing,
  multi-tab deduplication, offline/reconnect recovery, a cancelled page exit and
  a one-day incorrect workstation clock, plus recovery after initial inbox
  initialization exhausts its request retries. Unique IndexedDB receipts coordinate
  tabs; storage-restricted contexts have a documented best-effort fallback.
  An additional diagnostic regression retains constructor failures after
  refocusing/switching modules and prevents a test using unsaved preferences.
  This fixes misleading feedback, not an unverified claim that every client's
  OS banner now works. The optional `MX_TEST_BRAVE_EXECUTABLE` also exercises a
  locally installed Brave; real Chrome/Windows banners remain a manual gate.

### Repeat the new suites

Use Node 24+, installed frontend test dependencies, and a freshly built native
binary with its matching production frontend. From `frontend`, set disposable
paths once; do not point a runner at a live N1 service:

```sh
export MX_TEST_BINARY='/absolute/test-build/release/litiaina-mx'
export MX_TEST_STATIC_SITE_PATH='/absolute/test-build/dist'
export MX_TEST_ARTIFACTS='/absolute/disposable/test-root'
npm run test:records:advanced
npm run test:records:advanced-ui
npm run test:records:privacy
npm run test:records:advanced-scale
npm run test:notifications:focus
npm run test:collaboration:people
```

These suites create isolated MX/SQLite/storage-contract fixtures and retain
JSON results and browser screenshots in the printed artifact directory. Browser
audio is disconnected. The scale and upgrade runners require the explicitly
allowed test-storage roots used by the local qualification environment. For an
existing-data rehearsal, `test:upgrade:existing` additionally requires explicit
`MX_TEST_UPGRADE_DB` and `MX_TEST_UPGRADE_ADMIN` paths; it backs up the source and
never launches the candidate against that original database.

### Acceptance gates and limits

The populated local test now runs the optimized RC.5 preview at
`https://localhost:21001`. Its post-replacement read-only smoke passes in
Chromium, Firefox and WebKit: all 10,563 records and 17 business-table content
hashes remain unchanged, and an existing small Drive object downloads through
MX/N1. N1's binary hash and process ID are unchanged. The fresh verified backup
and smoke report are retained under
`/mnt/ST1000LM049/mx/backups/before-local-rc5-20261020.JpqNkR`.
That rehearsal preceded the authorized RC.5 publication; production deployments
were not modified. Later local QA data is additive and explicitly requested.

The candidate is eligible for **local acceptance testing**, not an unconditional
stable-release certification. Before production acceptance:

1. Verify a real OS banner on the organization's actual Windows/browser versions
   and trusted HTTPS origin: focused MX stays silent at OS level; minimizing or
   focusing another application produces one alert. Headless Linux tests cannot
   prove Windows banners, OS policy, Do Not Disturb or browser suspension behavior.
2. Approve the administrator workflow and sample relationships/balances/privacy
   using ordinary accounts. Accessible subtotals are not an organization-wide
   ledger, approval engine or overdraft-prevention mechanism.
3. Retain a verified backup and the paired previous binary/frontend. Older MX
   versions do not enforce RC.5 privacy/configuration: rollback requires the
   corresponding pre-update database snapshot, not just an older executable.

This pass is not a formal penetration test, assistive-technology audit,
Safari/iOS certification or physical-power-loss qualification. It does not
promise OS notifications when a browser is closed/discarded. N1 source, storage
format, production servers and publisher credentials are unchanged.

## Earlier local workspace interaction preview — historical baseline

That earlier pass changed the local source and disposable test builds only. It did not
publish RC.5, replace any deployed service, seed business modules, or modify N1.

Native unit checks: **180 passed, two opt-in cases ignored**. Frontend unit
checks: **337 passed across 55 files**. Production frontend compilation and
paired-version/Office-asset verification pass; Svelte checks report zero errors
and warnings. Vite retains its existing JavaScript-chunk-size advisory.

- Record cells are read-only/selectable; the full record form retains revision
  checks, relationship auto-fill and field-conflict decisions.
- Direct contextual record sends exercise the real collaboration/notification
  APIs, bounded name search, eight concurrent direct-conversation creates, and
  two deliberately lost send acknowledgments followed by an idempotent retry.
  A recipient without module access receives no record metadata or new grant.
- Personal field emphasis is checked in tables/forms and after reload, with
  account isolation and stale-preference rejection. Actual computed table text
  size is measured, not just the setting input. Cancel restores the prior size.
- The module library starts with no selected editor. Tests create a separate
  module, verify the original is unchanged, and retain unsaved field/module
  drafts after cancelling a scope change. Configuration sections are separate.
- Browser coverage uses Chromium, Firefox and Linux WebKit, light/dark themes,
  desktop/mobile layouts and 200% text. Extra checks cover enlarged-text reading
  and send dialogs and keyboard navigation in the compact workspace menu.
  These are not physical-device, assistive-technology or Safari/iOS certification.
- The isolated Drive grid/Deployment-spacing regression suite also runs in all
  three engines. Targeted Chromium suites cover full-form relationship auto-fill
  and conflicts, conditional forms, column totals, archive-error dialogs and
  bounded record scrolling/anchors. No production database or live N1 is used.

Run the new browser suite from `frontend`, with an updated native binary and
the matching production frontend build. Choose a disposable absolute test root:

```sh
MX_TEST_BINARY='/absolute/test-build/debug/litiaina-mx' \
MX_TEST_STATIC_SITE_PATH='/absolute/test-build/dist' \
MX_TEST_ARTIFACTS='/absolute/disposable/test-root' \
npm run test:records:workspace
```

The runner closes its isolated MX/SQLite/storage fixture after testing and writes
screenshots plus `record-workspace-results.json` inside its printed artifact
directory. `MX_TEST_BROWSERS=chromium` selects a focused run. Never set a live
`MX_TEST_N1_URL` for this suite. At that point frontend/native versions remained
`1.0.0-rc.4`; it was a local preview, not a released RC.4 update. The RC.5 section
above supersedes that earlier version/count baseline.

## RC.4 importer replacement — 2026-10-18

- Remove the importer-only required-attachment rejection at both preview and
  commit. Attachment configuration and the normal entry form are unchanged;
  spreadsheet migration writes field values, not fabricated files. Required
  non-attachment fields, types, relationships, permissions, uniqueness and atomic
  record/version/import markers remain enforced.
- Native debug suite: **178 passed, zero failed, two opt-in tests ignored**.
  Frontend: **335 passed across 54 files**; production frontend build, version
  consistency and Office-asset verification pass, with zero Svelte diagnostics.
  Vite retains its existing large-JavaScript-chunk advisory. Formatting and diff
  checks pass. These source checks precede the optimized replacement package.
- Read the reported local job using SQLite **read-only/query-only** mode, then
  replay its fields, source cells and exact mapping in a separate MX database.
  Of **5,682 selected rows**, **5,681 validate** and **one retains its genuine
  CONTROL NO integer error**. The first 20 rows commit with empty attachment lists
  while FILE ATTACHMENTS remains Required. The full migration is deliberately
  not run, and the operator's job/records/settings are untouched. Evidence:
  `/mnt/ST1000LM049/tests/mx-eight-user-Pf5F64/source-replay-results.json`.
- Diagnostics count affected rows, including rows on separate sheets, without
  double-counting repeated messages. They are fetched for preview/reopen rather
  than every processing batch. Revalidation of an all-failed job creates no
  records; any successfully imported row prevents remapping that job.
- Browser testing also exposed the saved-mapping render mutating its own state
  when restoring ignored columns. Restore those inputs before rendering instead.
  Earlier attempts that hit that exception, a transient completion-state mismatch
  or a fixture that incorrectly marked the header as failed are not passing runs.
  All generated databases, builds and browser audio-disconnected artifacts remain
  on the external test disk. Those test runs do not replace any deployment.
  The maintainer subsequently authorized replacing the published RC.4 and the
  local test, with backups; no RC.5 is published or equal-version guard disabled.
- Final import API/browser suite passes in **Chromium, Firefox and Linux WebKit**:
  eight accounts, lost acknowledgements, restart/deduplication, permissions,
  injected transaction failure, relationship/conditional scalar validation,
  attachment-free imports with unchanged Required settings, all-failed historical
  job recovery, ignored-column remapping, normal-form file rejection and honest
  completion feedback. Evidence:
  `/mnt/ST1000LM049/tests/mx-eight-user-NkmA0B/import-results.json`.
  Separate failed-row layout/correction checks pass in all three engines across
  light/dark themes and 320–1440px viewports, including long errors and paging:
  `/mnt/ST1000LM049/tests/mx-eight-user-OZfoIL/import-layout-results.json`.
- The actual optimized replacement repeats the complete three-engine import
  suite successfully:
  `/mnt/ST1000LM049/tests/mx-eight-user-T7Z0Rf/import-results.json`.
  Its read-only source replay repeats 5,681 valid rows, one genuine integer
  problem and 20 attachment-free commits with Required still enabled:
  `/mnt/ST1000LM049/tests/mx-eight-user-qRjhNX/source-replay-results.json`.
  The optimized RC.3 → replacement RC.4 updater passes independent systemd
  activation, verified snapshots, interrupted downloads, signed offline uploads,
  eight competing installs, failed-readiness rollback and killed-worker recovery,
  plus Chromium's operator UI checks:
  `/mnt/ST1000LM049/tests/mx-updates-nuMNbj/update-results.json`.
  Replacement release artifacts use
  `/mnt/ST1000LM049/releases/1.0.0-rc.4-import-fix-20261018`; earlier RC.4
  artifacts and annotated tags remain archived. The signed package is verified
  against the unchanged pinned publisher key, every runtime file and paired
  frontend identity. No database migration or updater-source change is added.

## RC.4 N1 RC.1 compatibility refresh — 2026-10-18

- N1 `5759dd01e5c11a43f07510dd384c723a4fc2819b` reports product/storage
  specification **1.0.0-RC.1**, **storage encoding 4**, **NOA v1** and native
  **n1/0.5**. Its reset does not change MX's API routes or require rewriting
  encoding-4 data/backups. No native/frontend application source or dependencies
  change in this refresh; the signed RC.4 runtime package/metadata and publisher
  key remain byte-identical to the original release.
- Fresh frontend unit suite: **333 passed across 54 files**. Fresh native suite:
  **177 passed, zero failed, two opt-in tests ignored**, including the held-writer
  regression. The 125,000-record/import and updater qualifications below remain
  the original 2026-10-17 evidence, not new RC.1-backed import/updater claims.
- The real-engine runner provisions its own N1 RC.1 process, filesystem root,
  fragment and credentials under a unique external test directory. It runs the
  existing Drive API/browser suite and durable Office suite against actual N1
  bytes through the existing fault-injecting loopback proxy. Evidence:
  Drive (18 cases):
  `/mnt/ST1000LM049/tests/n1-rc1/mx-n1-rc1-yiGzFn/mx-eight-user-Up1kve/drive-results.json`.
  Office:
  `/mnt/ST1000LM049/tests/n1-rc1/mx-n1-rc1-gvSr8o/n1-rc1-results.json`.
  The default repeat command runs both; `MX_TEST_N1_SUITES=drive` or `office`
  repeats one suite without claiming coverage of the other.
- Drive covers multipart resumed after an MX restart, a 130 MiB upload and a full 50 MiB
  part for a >1 GiB declared upload, eight accounts, quotas, exact range bytes,
  metadata-only rename/copy, sharing/revocation, version history/restore, batch
  lost acknowledgements and durable background cleanup. Browser coverage is
  Chromium, Firefox and Linux WebKit. Office coverage is Chromium, Brave and
  Firefox: real exports, tab closure during upload, uncertain acknowledgement,
  offline recovery and visible local-storage failure, with saved contents checked.
- Earlier runner attempts are not counted as passing: the first omitted required
  disabled-feature N1 config fields; another correctly refused physical audio;
  a third completed browser cases but misclassified its own cancelled upstream
  sockets as N1 faults. The final runner merges N1's complete default template,
  disconnects browser audio and separately observes downstream cancellation.
  Unexpected proxy transport failures still fail the suite.
  The pre-existing Office guard also rejected the first real-engine attempt;
  it now accepts only the owned disposable process after verifying its kernel
  working directory/executable, loopback port, private credentials and roots.
  Arbitrary live-N1 URLs remain rejected before starting MX or browser tests.
- This is **1D+0P single-root functional compatibility**, not EC redundancy,
  N1 process-restart, throughput, Safari/iOS or physical-power-loss qualification. Test-only N1 rate
  limiting/healing are disabled to isolate API behavior. It does not change the
  running local test service, MCH/DGS services, their objects or configuration;
  generated test data and credentials remain on the external HDD.
- At the maintainer's explicit request, RC.4's initial-install archive, guides,
  release notes/provenance and tag are refreshed before departmental adoption.
  Original artifacts/commit remain under
  `/mnt/ST1000LM049/releases/1.0.0-rc.4`; refreshed assets use
  `/mnt/ST1000LM049/releases/1.0.0-rc.4-n1-rc1-20261018`. Existing RC.4
  installations need no runtime reinstall; equal-version install rejection is
  retained. No live N1 upgrade or data migration is implied by this refresh.

## 1.0.0-rc.4 database-contention hotfix — 2026-10-17

- Fresh native suite: **177 passed, zero failed, two opt-in tests ignored**.
  Optimized native build succeeds. Frontend: **333 tests across 54 files**;
  production build, release consistency and bundled Office verification pass,
  with zero Svelte errors/warnings. Frontend behavior/styles are unchanged.
  Signed-manifest comparison confirms 26 runtime files are byte-identical to
  RC.3; only the native binary and paired release-identity JSON change.
- The preserved optimized RC.3 reproduces the fault after schema warm-up: with
  an independent `BEGIN IMMEDIATE` writer, normal modules, notifications, calls
  and storage-layout reads return 500 after roughly 10 seconds; record/schema
  access also fails. Evidence:
  `/mnt/ST1000LM049/tests/rc4/mx-eight-user-DSdLkl/baseline-contention.json`.
- A new native regression holds a real WAL writer against **50,000 records**
  while **16 read-only connections** call every initializer and query data.
  It also checks that initialization readiness does not leak to another DB.
- The optimized HTTP regression uses **100,000 existing records/versions**,
  imports **25,000 additional records** through normal upload/configure/process
  endpoints and keeps **eight authenticated accounts** reading concurrently.
  It verifies seven affected endpoints behind an independent held writer,
  retained record/version counts, atomically committed import audit entries,
  post-import reads, reconstruction of older missing audit entries and two
  restarts without duplicate history. No database-lock errors are accepted.
  Evidence:
  `/mnt/ST1000LM049/tests/rc4/mx-eight-user-uQ3PJo/database-contention-results.json`.
  Final result: **125,000 records, 25,000/25,000 imported, zero failed rows,
  10,653 requests, 9,253 background polls and zero database-lock errors**.
  Seven held-writer reads finish in **74 ms**. The external-HDD import takes
  **1,214,708 ms** (about 20 minutes); combined request p95 is **920 ms**, max
  **5,743 ms**, including import/authentication calls. This qualifies contention
  handling, not high-throughput import performance or guaranteed latency.
  Synthetic seed rows advance the hidden control sequence, just as normal
  creation does; an initial fixture without that step correctly rejected
  duplicate control numbers and is not counted as a passing import test.
  A subsequent extended run expired its 15-minute access tokens; the final
  harness uses normal refresh-token renewal and eight clients polling once per
  second. Token-expiry/fixture failures are not counted as passing runs.
- An independent restart test reuses the stopped disposable database from the
  expired-token run: **121,640 records and versions** remain, **100,000 missing
  audit events** are reconstructed, and `quick_check` returns `ok`. Startup
  takes **8,386 ms**, then **5,944 ms** on the second restart without duplicate
  events. Evidence:
  `/mnt/ST1000LM049/tests/rc4/mx-eight-user-g2ydfY/startup-scale-results.json`.
- The actual optimized **RC.3 → RC.4** updater passes independent systemd
  activation, verified final snapshot, interrupted-download recovery, offline
  signed upload, eight competing installs, failed-readiness rollback and
  killed-worker recovery. Record contents/history and fixture-backed Drive
  bytes survive. The update UI is repeated in Chromium; other browser/Office
  results below are historical, not rerun RC.4 qualifications. Evidence:
  `/mnt/ST1000LM049/tests/rc4/mx-updates-WxTvsJ/update-results.json`.
- The signed package is verified against the unchanged publisher key, all
  28 runtime hashes/sizes, paired native/frontend identity and Office notices.
  The initial-install tree uses the same runtime bytes and includes operator
  documentation/templates without credentials or databases. Reports are under
  `/mnt/ST1000LM049/releases/1.0.0-rc.4`.
- Native CLI version/help/error handling creates no deployment files, and key
  generation retains mode 0600 and refuses to overwrite an existing key:
  `/mnt/ST1000LM049/tests/rc4/mx-update-cli-P2T3RB/cli-results.json`.
- Tests/builds stay on the external drive with disposable SQLite and N1 HTTP
  contract fixtures; browser audio is disconnected. No live N1, DGS/MCH records,
  deployment credentials or production services are changed by these tests.
  SQLite still serializes genuine writes. This test is not unlimited-scale,
  physical-power-loss, security or final stable-release certification.

Repeat the HTTP regression with an optimized candidate and matching frontend:

```bash
env TMPDIR=/mnt/ST1000LM049/tests/rc4/tmp \
  MX_TEST_ARTIFACTS=/mnt/ST1000LM049/tests/rc4 \
  MX_TEST_BINARY=/mnt/ST1000LM049/releases/1.0.0-rc.4/staging/update-input/litiaina-mx \
  MX_TEST_STATIC_SITE_PATH=/mnt/ST1000LM049/releases/1.0.0-rc.4/frontend-dist \
  node --no-warnings frontend/tests/database-contention.integration.mjs
```

Use Node 24+ with the existing frontend test dependencies. The test rejects
live-N1 configuration and requires the external test-artifact directory.

## 1.0.0-rc.3 Office background preparation — 2026-10-16

- Frontend unit tests: **333 passed across 54 files**. Production build and
  bundled Office asset verification pass; Svelte reports zero errors/warnings.
  Existing Vite chunk-size/plugin-timing advisories remain. The Office behavior
  changes are frontend/worker-adapter changes; the native version is rebuilt
  to keep the signed candidate identity consistent. Fresh Rust tests:
  **176 passed, two opt-in tests ignored**. Logs/builds are under
  `/mnt/ST1000LM049/releases/1.0.0-rc.3`.
- The preserved optimized RC.2 executable and actual optimized RC.3 candidate
  pass `test:updates`: automatic maintenance-window activation, verified final
  snapshot, interrupted transfer/restaging, signed offline upload, eight
  competing installs, failed-readiness rollback and worker-termination recovery.
  Record values/history and fixture-backed Drive bytes survive. Update UIs
  pass Chromium, Brave, Firefox and Linux WebKit. Evidence:
  `/mnt/ST1000LM049/tests/mx-updates-0eqK2K/update-results.json`.
- Native CLI diagnostics pass without deployment side effects:
  `/mnt/ST1000LM049/tests/mx-update-cli-77mFSR/cli-results.json`. The actual
  distributable passes the unchanged Ed25519 publisher key, all 28 signed
  runtime sizes/hashes, matching native/frontend identity, bundled Office
  notices and deployment-secret exclusion. The initial-install staging tree
  matches those same signed runtime bytes and includes operator guides and
  templates. Reports:
  `/mnt/ST1000LM049/releases/1.0.0-rc.3/package-verification.json` and
  `initial-install-verification.json` in the same directory.
- Chromium, Brave and Firefox pass `test:office:idle` with the real bundled
  XLSX engine. Tests verify unfinished cell text survives an idle pause,
  scrolling/held-pointer/composition events defer preparation, genuine idle
  saves actual workbook contents, and an explicit save during a held automatic
  upload includes the newer unfinished cell. Native selection, browser focus
  and formulas remain intact. Evidence:
  `/mnt/ST1000LM049/tests/mx-eight-user-maXm7D/office-idle-results.json`.
  The final bundle repeats the Chromium pass after bounding the cost-based
  cooldown to one minute; its timing boundaries also have unit coverage:
  `/mnt/ST1000LM049/tests/mx-eight-user-WclBxU/office-idle-results.json`.
  All three engines repeat the complete pass with the final optimized RC.3
  executable/production bundle:
  `/mnt/ST1000LM049/tests/mx-eight-user-b6GsN8/office-idle-results.json`.
- Chromium and Firefox repeat `test:office:durable`: one export per changed
  snapshot, no export for unchanged contents, tab closure during upload,
  uncertain acknowledgement reconciliation, offline automatic recovery and
  visible local-storage failure before upload. All five acknowledged revisions
  contain the expected edits. Evidence:
  `/mnt/ST1000LM049/tests/mx-eight-user-sBvbBG/office-durable-results.json`.
  Both engines repeat the pass with the final optimized RC.3 executable and
  production bundle in
  `/mnt/ST1000LM049/tests/mx-eight-user-jTGTkL/office-durable-results.json`.
- Tests wait for the native target-cell selection before typing; they do not
  qualify rapid native navigation/input latency. Composition deferral uses
  synthetic browser events, not an OS input-method qualification. Native export
  remains synchronous and cannot be safely preempted once started. No claim of
  pause-free large-workbook exports, Safari support or physical power-cut
  durability follows from these results.
- Builds and disposable MX/SQLite/N1-contract fixtures stay on the external
  drive. Browser audio is disconnected. No live N1, business data, MCH deployment
  or running encoding-test service is changed. Published RC.2 assets remain
  unchanged; these fixes use the separately versioned RC.3 signed release.

Repeat the RC.2 → RC.3 updater qualification with explicit preserved/candidate
versions. The default fixture remains RC.1 → RC.2 for historical repeatability:

```bash
cd frontend
env TMPDIR=/mnt/ST1000LM049/build/tmp \
  CARGO_TARGET_DIR=/mnt/ST1000LM049/build/target \
  MX_TEST_ARTIFACTS=/mnt/ST1000LM049/tests \
  MX_TEST_BOOTSTRAP_VERSION=1.0.0-rc.2 \
  MX_TEST_BINARY=/mnt/ST1000LM049/releases/1.0.0-rc.2/staging/mx-1.0.0-rc.2-x86_64-unknown-linux-gnu/litiaina-mx \
  MX_TEST_STATIC_SITE_PATH=/mnt/ST1000LM049/releases/1.0.0-rc.2/staging/mx-1.0.0-rc.2-x86_64-unknown-linux-gnu/frontend/dist \
  MX_TEST_CANDIDATE_VERSION=1.0.0-rc.3 \
  MX_TEST_CANDIDATE_BINARY=/mnt/ST1000LM049/releases/1.0.0-rc.3/staging/update-input/litiaina-mx \
  MX_TEST_CANDIDATE_STATIC_SITE_PATH=/mnt/ST1000LM049/releases/1.0.0-rc.3/frontend-dist \
  npm run test:updates
```

The harness checks native version output, rejects equal/older candidate
versions and creates only disposable managed services/data. Its deliberately
broken subsequent candidate is test-only, not a distributable release.

## 1.0.0-rc.2 navigation, records and blank setup — 2026-10-16

- Optimized native build and matching production frontend succeed. **176 Rust
  tests pass, two opt-in tests remain ignored; 327 frontend tests pass across
  53 files**. Svelte reports zero errors/warnings. Existing Vite chunk-size and
  plugin-timing advisories remain.
- Chromium, Firefox and Linux WebKit pass 24 sidebar cases covering collapsed
  and expanded rails, 85/100/160% interface scales, both themes, many long-name
  modules and mobile drawers. Logo files/README logo markup are unchanged.
  Evidence: `mx-eight-user-ZPiCSp/sidebar-results.json`.
- The same engines pass Drive row/column and sharing layout plus Deployment
  section-gap regressions at 1440/1024/390/320 px in both themes. Representative
  screenshots inspected. Evidence: `mx-eight-user-PA6jn0/ui-cascade-results.json`.
- Blank-installation onboarding, restart without invented modules, creating and
  deleting the only test module, Pages by default, retained column widths and
  horizontal position, bounded DOM and delayed/rapid view changes pass in all
  three engines. Deliberately delaying the first page confirms an early switch
  uses readable datatype widths rather than freezing the placeholder layout.
  Switching does not refetch the schema. Evidence:
  `mx-eight-user-nglepb/record-navigation-results.json`.
- Chromium, Brave, Firefox and Linux WebKit pass the 620-record continuous-window
  regression: bounded cache/DOM, bidirectional cursors, deletion/insertion
  boundaries, tall full-text rows, fast drags and wheel input, exact UID/offset
  restoration across dialogs/navigation, failed-read retry and page fallback.
  A spacer-to-measured-row drift was reproduced and corrected before packaging.
  Evidence: `mx-eight-user-V9CnoQ/record-window-results.json`.
- The numbered-page horizontal-control regression passes in all three engines:
  pointer/keyboard movement, nested clipping, native-bottom handoff, resizing,
  column visibility, dialogs, mobile and pagination. Evidence:
  `mx-eight-user-Fgfcwm/record-scroll-results.json`.
- Native CLI diagnostics pass (`mx-update-cli-vngBBd/cli-results.json`). The
  actual distributable passes Ed25519 verification with the unchanged publisher
  key, all 28 signed file sizes/hashes, exact archive membership, matching native
  and frontend identity, bundled Office notices and exclusion of deployment
  secrets. Evidence: `releases/1.0.0-rc.2/package-verification.json`.
- The final optimized RC.2 executable/frontend pass the preserved RC.1 → RC.2
  updater regression: automatic-window deferral/activation, verified snapshot,
  interrupted transfer, signed offline upload, eight competing installs, failed
  migration/readiness rollback and independent worker termination/recovery.
  Actual record values/history and fixture-backed Drive bytes survive. All four
  update UIs pass (Chromium, Brave, Firefox, Linux WebKit). Evidence:
  `mx-updates-NjBL6q/update-results.json`. The deliberately compressed fault
  fixture disables systemd's start-burst limiter; deployment templates do not.
- Evidence paths above are under `/mnt/ST1000LM049/tests`, except the package
  report under `/mnt/ST1000LM049`. Builds/tests/packages stay on that drive.
  These use disposable MX/SQLite and an N1 HTTP contract fixture, never existing
  business data. No desktop audio devices, live N1, MCH restart or replacement
  of the running encoding test are involved. Linux WebKit is not real Safari
  or iOS qualification; existing RC.1 acceptance limits still apply.

## 1.0.0-rc.1 release assembly — 2026-10-16

- Rebuild the optimized native executable and production frontend for the
  candidate; refresh the tracked frontend bundle to match. Fresh unit runs:
  **174 Rust tests passed, two opt-in ignored; 322 frontend tests passed**.
  Svelte checks report zero errors/warnings. Release metadata and bundled
  Office asset checksums pass. Existing Vite size/timing advisories remain.
- Repeat the complete updater integration against these release builds:
  real automatic activation, verified backup, failed-candidate rollback, eight
  competing installs, worker termination/recovery and all four browser UIs pass.
  Evidence: `/mnt/ST1000LM049/tests/mx-updates-WcwlLz/update-results.json`.
  These use disposable SQLite and the N1 HTTP contract fixture, not live data.
- Verify the actual distributable's Ed25519 signature with the published public
  key, all 28 signed file sizes/SHA-256 values, exact archive membership, native
  version, matching frontend identity and bundled Office license/notices files.
  Check that deployment credentials, database files, TLS/private signing keys
  are absent. This verifies inclusion/integrity, not a legal/security audit of
  the upstream Office engine. Evidence:
  `/mnt/ST1000LM049/releases/1.0.0-rc.1/package-verification.json`.
- Build logs/artifacts remain under `/mnt/ST1000LM049/releases/1.0.0-rc.1`.
  The Linux x86-64 GNU binary's glibc/OS-library requirements are documented in
  the [release notes](docs/releases/1.0.0-rc.1.md); other platforms are not
  qualified by this release. Existing MX/N1 deployments are not changed.

## Signed updates and maintenance windows — 2026-10-15

- Optimized native build succeeds. Rust: **174 passed, zero failed, two opt-in
  tests ignored**. Frontend: **322 passed across 52 files**. The final production
  frontend has zero Svelte errors/warnings; existing Vite chunk-size/plugin
  timing advisories remain. Build/test logs use
  `/mnt/ST1000LM049/build/mx-updater-*`.
- `npm run test:updates` passes with a real optimized `1.0.0-rc.1` bootstrap and
  independent worker, disposable SQLite, test-only signing keys and actual
  systemd user-service restarts. The candidate is a separately compiled
  `1.0.0-rc.2` development-profile native executable, not a simulated version
  response. A deliberately broken signed candidate mutates the disposable
  database to exercise post-migration rollback.
- Coverage: administrator-only authority, invalid signatures, release outages,
  interrupted transfers, restaging, automatic-window deferral and installation,
  signed offline upload, verified final snapshot, matched binary/frontend
  activation, eight competing install requests with one winner, paired rollback,
  worker `SIGKILL` and independent recovery. A committed journal never rewinds
  subsequent acknowledged writes; a failed candidate is not retried automatically.
  Record contents, normal history and Drive references/bytes are checked after
  activation and recovery. Drive storage uses the **N1 HTTP contract fixture**,
  not a live N1 deployment.
- Chromium, Brave, Firefox and Linux WebKit pass policy editing/validation,
  draft preservation across workspace refresh (also used by live reconnection), lost job
  acknowledgements without POST replay, history and responsive light/dark
  layouts at 1280/1024/390/320 px. Representative screenshots inspected.
  Evidence: `/mnt/ST1000LM049/tests/mx-updates-dQ1MkK/update-results.json`.
  Linux WebKit is not real Safari/iOS qualification.
- Native CLI diagnostics pass without creating deployment files: `--version`,
  `-V`, `--help`, `-h`, unknown options and malformed worker arguments/config.
  Key generation enforces mode `0600` and refuses to overwrite an existing key.
  Evidence: `/mnt/ST1000LM049/tests/mx-update-cli-jMv70G/cli-results.json`.
- The final optimized binary/frontend also pass the Chromium backup regression:
  real snapshot upload/verification, eight competing create attempts, navigation,
  localized feedback, uncertain acknowledgement reconciliation and responsive UI.
  Evidence: `/mnt/ST1000LM049/tests/mx-eight-user-Lb5tra/backup-results.json`.
- Disposable runtime unit registrations are removed after the test. Existing
  external MX/N1 services remain stopped and unchanged. All build/test artifacts
  stay on the external drive. No production keys, real accounts, live N1 data or
  physical audio devices are used. This does **not** certify physical power-loss
  recovery, system-service Polkit policy or a complete production security audit.
  No release is published or existing deployment automatically enrolled.

Repeat the opt-in updater integration test with Node 24+, Cargo, systemd user
services and the native/frontend builds already prepared on the external drive:

```bash
cd frontend
env TMPDIR=/mnt/ST1000LM049/build/tmp \
  CARGO_TARGET_DIR=/mnt/ST1000LM049/build/target \
  MX_TEST_ARTIFACTS=/mnt/ST1000LM049/tests \
  MX_TEST_BINARY=/mnt/ST1000LM049/releases/1.0.0-rc.1/staging/mx-1.0.0-rc.1-x86_64-unknown-linux-gnu/litiaina-mx \
  MX_TEST_STATIC_SITE_PATH=/mnt/ST1000LM049/releases/1.0.0-rc.1/staging/mx-1.0.0-rc.1-x86_64-unknown-linux-gnu/frontend/dist \
  MX_TEST_CANDIDATE_BINARY=/mnt/ST1000LM049/build/target/release/litiaina-mx \
  MX_TEST_CANDIDATE_STATIC_SITE_PATH=/mnt/ST1000LM049/releases/1.0.0-rc.2/frontend-dist \
  npm run test:updates
```

This explicit RC.1 → RC.2 fixture uses the preserved RC.1 bootstrap and the
actual optimized RC.2 candidate. Adjust the paths on another machine; do not
use RC.2 as its RC.1 bootstrap. If the candidate variables are omitted, the
suite compiles a development-profile RC.2 fixture into the supplied Cargo cache;
rebuild the repository before distributing an executable from that cache.
For `npm run test:updates:cli`, point `MX_TEST_BINARY` at the candidate being tested.
See [deployment setup, trust and recovery](docs/updates.md) before enabling
updates on a real installation.

## 1.0.0-rc.1 release identification — 2026-10-15

- Rust/frontend manifests, root lockfile entries and README channel agree on
  `1.0.0-rc.1`. The frontend production build runs `check:release` automatically;
  eight regression tests reject inconsistent versions or a candidate described
  as a final release. README logo/branding and historical releases are retained.
- Fresh frontend unit run: **311 passed across 51 files**. Rust:
  **161 passed, zero failed, two opt-in tests ignored**. The production frontend
  builds with zero Svelte errors/warnings; existing Vite chunk-size/plugin timing
  advisories remain. Logs and artifacts are under
  `/mnt/ST1000LM049/build/mx-1.0.0-rc.1-*`.
- The optimized native build succeeds. Running its `--version` and `-V` from
  separate empty working directories prints exactly `litiaina-mx 1.0.0-rc.1`,
  exits zero with no stderr and creates no files. Evidence:
  `/mnt/ST1000LM049/tests/mx-1.0.0-rc.1-cli-qNTnmP/version-results.json`.
- The same candidate binary and rebuilt frontend pass the Chromium backup suite
  against disposable SQLite and the N1 HTTP contract fixture: normal startup,
  snapshot verification, eight rejected concurrent create attempts, navigation,
  local feedback, lost acknowledgements and narrow layouts. Evidence:
  `/mnt/ST1000LM049/tests/mx-eight-user-akCefL/backup-results.json`.
  This run does not qualify live N1 or physical audio/video devices.
- This is local candidate preparation, not public publication or a completed
  production qualification. No existing deployment, SQLite database, N1 data or
  configuration is reset by the version change. Follow the
  [candidate notes and remaining acceptance gates](docs/releases/1.0.0-rc.1.md).

## Backup activity feedback — 2026-10-15

- `npm run test:backups:browser` uses disposable MX/SQLite and the N1 HTTP contract
  fixture, not live N1 data. A real snapshot upload is held while checking
  immediate button activity, indeterminate progress, elapsed time, automatic
  status recovery and feedback after tab/module navigation. Each browser run
  rejects eight concurrent create attempts while the first backup is active.
  Stored bytes have the SQLite header and pass the real verification endpoint.
- Chromium, Brave, Firefox and Linux WebKit pass. Tests also cover localized
  failures, visible verification activity, lost commit acknowledgements without
  create replay, uncommitted request reconciliation, stopped idle polling and
  responsive light/dark layouts at 1280/1024/390/320 px. Screenshots inspected.
  Evidence: `/mnt/ST1000LM049/tests/mx-eight-user-rA56HC/backup-results.json`.
  Linux WebKit is not a real Safari/iOS qualification.
- Final unit suite: 303 passed across 50 files. Request-contract tests ensure a
  long, non-replayed backup POST and bounded, cancellable status reads. Production
  build: zero Svelte errors/warnings; the existing Vite chunk advisory remains.
- Frontend-only installation on the external test deployment; no database or
  backend/N1 binary changes. Both external test services were already stopped
  and were left stopped, so no live deployed smoke is claimed for this update.
  The previous frontend and immutable assets are retained for rollback.

## Compact records controls and full long text — 2026-10-15

- Remove the permanent continuous-view footer; its keyboard-accessible up/down
  actions share the results toolbar. The table fills the remaining viewport,
  including when no totals are configured. Errors still show a visible retry.
- Remove the editable long-text three-line clamp. Tests use tall, multiline
  paragraphs and verify full wrapping, expanded rows and no hidden text overflow.
- Stress rapid scroll-position changes, native wheel input, variable-height
  measurements, reverse loading, cache eviction and exact record/offset
  restoration. A Firefox restoration drift was reproduced during the stricter
  run; retain the intended offset through layout/rounding corrections and release
  it on actual user scrolling. Two consecutive Firefox runs pass, alongside
  Chromium, Brave and Linux WebKit. Evidence:
  `/mnt/ST1000LM049/tests/mx-eight-user-fSpl7u/record-window-results.json`.
  This is not a real Safari/iOS qualification.
- The existing numbered-page horizontal scrollbar regression passes in Chromium,
  Firefox and WebKit (`mx-eight-user-FDsfgt/record-scroll-results.json`). Final
  frontend unit tests: 301 passed across 49 files. Production build: zero Svelte
  errors/warnings; existing Vite chunk-size advisory remains.
- Frontend-only atomic exchange on the external test deployment; previous bundle
  and immutable assets retained. No backend/N1 restart, database migration or
  business-data edits are needed for this correction.
  The deployed records view, page-mode fallback and narrow-screen Drive smoke
  pass in Chromium, Brave, Firefox and WebKit:
  `/mnt/ST1000LM049/tests/deployed-records-compact-results.json`.

## Bounded records and durable Office saves — 2026-10-15

- `npm run test:records:window` creates 620 disposable records and checks
  bidirectional query-bound cursors, insertion/deletion boundaries, full-result
  totals, bounded cache/DOM, record/offset and horizontal restoration across
  dialogs/navigation, failed-edge retry, responsive light/dark layouts and the
  paginated fallback. Chromium, Brave, Firefox and Linux WebKit pass. WebKit's
  resize-observer loop was reproduced and fixed by batching measurements outside
  observer delivery. Evidence: `mx-eight-user-ZJaOU8/record-window-results.json`
  under `/mnt/ST1000LM049/tests`.
  The final action-label bundle also passes the Chromium window suite, including
  labels matching visible row positions after eviction
  (`mx-eight-user-41KeGR/record-window-results.json`). Its frontend-only exchange
  does not restart either service.
- Rust: 161 passed, two opt-in tests ignored. New keyset tests cover nulls, tied
  text/numeric/boolean sorting in both directions, deleted-anchor recovery and
  rejection of cursors crossing modules, filters or sort settings. Frontend unit
  tests additionally cover sparse rendering, bounded eviction, pinned editors,
  snapshot reuse and recovery-file identity. Build output stays on the external
  drive; no N1 source/configuration changes are part of this feature.
  Final frontend unit run: 300 passed across 49 files; Svelte checks report zero
  errors/warnings and the production build retains only the existing chunk-size
  advisory. Numbered-page horizontal scrolling and summaries regressions pass
  in Chromium, including eight concurrent summary readers during writer commits
  (`mx-eight-user-HIlxWA`, `mx-eight-user-tNURaX`).
- `npm run test:office:durable` uses the real bundled document engine with
  disposable MX/SQLite and the N1 HTTP contract fixture. It checks one export per
  changed save, no export for unchanged content, automatic recovery without
  offline retention, tab closure during upload, lost commit acknowledgements,
  offline checkpoints, exact restored contents and visible storage failures
  before upload. Recovery payloads retain operation IDs, timestamps and base
  revisions. The Firefox runs reproduced a hang uploading restored IDB-backed
  bodies; recovery now materializes a memory-backed snapshot using the bytes
  already read to open it. Normal saves take no extra recovery copy/export.
  The final Chromium/Brave suite passes in `mx-eight-user-5HfTX3`; Firefox passes
  in `mx-eight-user-OwomHq` (each has `office-durable-results.json`). All five
  committed versions contain the expected actual edits; lost acknowledgements
  do not manufacture an additional version.
  The final error-feedback/cadence bundle repeats the Chromium pass in
  `mx-eight-user-A73b9n`; its production build has zero Svelte errors/warnings.
- Focused Chromium XLSX tests verify actual stored cell input from native Save,
  Ctrl+S and File → Save without requiring Enter; preserve formulas and edits
  typed during an earlier save; unchanged saves create no version. Eight
  simultaneous prepared Office commits still yield one winner/seven conflicts.
  Evidence: `mx-eight-user-Sc3Fvl/office-results.json`.
- Loading tests pass in Chromium/Firefox: real held document/resource transfers,
  five honest stages, final focus/geometry and warm reopening
  (`mx-eight-user-K7Eo26/office-loading-results.json`). Cancellation/retry and
  unchanged-original fallback pass in Chromium/Firefox/WebKit
  (`mx-eight-user-RQp3Ax/office-recovery-results.json`). Linux WebKit lacks the
  required worker graphics; that is a download fallback, **not** validated Office
  editing or a real Safari/iOS qualification.

These are browser-tab interruption and injected network/storage fault tests,
not physical power-cut certification or a live N1 hardware-durability test.
Changes after the last completed checkpoint can still be lost. Large native
workbook parsing/export remains engine-dependent; startup overlap and skipped
redundant exports are not a claim that every document now opens instantly.

Run the two focused suites against a disposable server, with
`MX_TEST_ARTIFACTS=/mnt/ST1000LM049/tests`, an externally built `MX_TEST_BINARY`
and `MX_TEST_STATIC_SITE_PATH` pointing at the candidate production bundle.
They refuse a live N1 test URL. Browser processes are disconnected from desktop
audio services; no physical microphone/speaker tests are performed here.

The release backend and final frontend are deployed to the external test at
`https://localhost:21001`. Read-only records/Drive smoke checks pass in Chromium,
Brave, Firefox and WebKit; the served Office shell, isolation headers, worker
generation and snapshot-reuse adapter are verified
(`deployed-cursor-office-results.json`). Binary/frontend rollback copies and an
online SQLite backup are retained on the external drive. N1 binary/PID and both
configurations are unchanged. Every original record/value/history, Drive and
schema row matches the pre-deployment backup. Concurrent user module/field
creation and imports increased counts during verification and were retained,
not mistaken for a migration or rolled back
(`cursor-office-deployment-after.json`). Existing Office tabs must be reopened
to use the new editor; preserve old unsaved edits first.

## Office launch prompt layout — 2026-10-15

`npm run test:office:prompt` reproduces the old left-aligned action row (about
102 px off center), then verifies the fixed production bundle in Brave, Chromium,
Firefox and WebKit. All 80 editor/guest × light/dark × 1440/900/768/390/320 px
checks pass: icon/prompt/action rows align, controls fit, original downloads are
byte-identical, guest access stays download-only and the editor action opens the
correct file tab. No Office engine or server PDF conversion is started by this
layout suite. Screenshots inspected. Evidence: `mx-eight-user-bonusk` (baseline
failure), `mx-eight-user-G5opQ3/office-prompt-layout-results.json` (pass), under
`/mnt/ST1000LM049/tests`. Frontend: 290 unit tests; zero Svelte errors/warnings;
production build passes with its existing chunk-size advisory.
The frontend-only test deployment also passes read-only prompt checks in all
four engines. Both service PIDs/binaries and record/Drive metadata hashes remain
unchanged (`deployed-office-prompt-results.json`, `office-prompt-deployment-check.json`).

## Cross-browser playback recovery and Office scroll diagnosis — 2026-10-15

- Rust: 158 passed, two opt-in tests ignored. Frontend: 290 tests across 46
  files; Svelte check has zero errors/warnings. Release/backend and production
  frontend builds pass; the existing Vite large-chunk advisory remains.
- `npm run test:media:recovery` uses disposable SQLite/N1 contract storage,
  eight accounts and real browser media elements. Each simultaneous storage
  rejection produces the original 429/503 and Retry-After, with exactly one
  upstream request per client. Exact-byte ranges recover after the fault clears.
  No streaming rate limit or server retry loop was added.
- The first focused run (`mx-eight-user-mqU1HX`) passes Brave, Chromium, Firefox
  and Linux WebKit: an actual HTTP 429 blocks the initial synthetic VP8 video,
  visible feedback appears and native video playback recovers. Mid-play audio
  recovery injects the terminal network-error event/property; HTTP diagnosis,
  reload, position restoration and subsequent playback are real. It is not a
  claim of a real packet-loss media soak. Closing/navigation cancels recovery.
- A late-probe race is covered separately: if playback recovers during a
  connection check, its late result cannot restart healthy playback. Unit tests
  also cover finite retries, HTTP-date Retry-After, revoked/expired access,
  unsupported codecs, paused state, persistent stalls and teardown cancellation.
- The production-bundle rerun (`mx-eight-user-AVZeb4`) passes all four
  browsers after the late-probe fix, including visible desktop feedback and
  dark/mobile wrapping. Screenshots were inspected. This is synthetic media and
  contract storage, not a full playback soak of the live 12.5-GB MP4.
- The final terminal-error guard bundle (`mx-eight-user-ZHm3yw`) repeats that
  four-browser pass. Once access/codec failure or the recovery budget stops
  automatic recovery, later native waiting/error events cannot restart checks.
- Existing Drive sharing/navigation regression (`mx-eight-user-fyiBBI`) passes
  Chromium, Firefox and WebKit, including delayed metadata, module changes,
  Back/Forward, dismissed-video non-resurrection, ACL revocation and light/dark/
  narrow layouts. All browser audio is disconnected from desktop audio servers.

Read-only Office diagnostics use the actual current saved workbook, revision 2,
served from memory after one download to isolate native rendering from storage.
The corrected drag targets advance the native view to row 91 and column 33 in
Brave, Chromium and Firefox. Post-drag controller acknowledgements are about
21–37 ms. Outer-page frame timing is not native-canvas paint timing. These short
headless probes do **not** reproduce or fix the reported desktop scroll freeze.
Opening remains slow (approximately 123–186 seconds); some runs overlap release
compilation, so these are not controlled speed comparisons. File revision stays
unchanged and no Office runtime/vendor patch is being represented as a fix.

Linux WebKit fails the required worker-WebGL capability check and offers the
unchanged original as a download. This is an editor **unsupported/fallback**
result, not a passed spreadsheet editing/scrolling test. Real Safari/iOS and
the user's GPU/compositor path remain unverified. Read-only diagnostic reports
are under `/mnt/ST1000LM049/tests/office-scroll-*-recheck.json`.

The user reports the N1 limiter fixed; inspection confirms the reciprocal-period
fix in their source checkout. The external test service still had the previous
N1 binary when checked. N1 source, binary, configuration and service are left
unchanged by this work. This is separate from the MX transport/player fixes.

The MX-only release build is deployed at the external-drive test URL. The
deployed UI smoke passes all four engines with stream failures mocked in-browser
(no real video bytes downloaded): `deployed-media-results.json`. Ten bounded
direct-N1/MX probes of the live MP4 return HTTP 206 with matching sample hashes
and Content-Range: `video-stream-after-media-recovery.json`. These samples do not
qualify a complete long-video playback soak. Before/after hashes of schema,
records, values, history and Drive metadata match; N1 binary/config/PID match:
`media-deployment-data-check.json`. Binary/frontend rollback copies are retained.

## Import results layout and safe field widening — 2026-10-15

- Rust: 156 passed, two opt-in tests ignored. Frontend: 270 passed across 45
  files. Production build: zero Svelte errors/warnings; existing chunk advisory.
- `test:records:import-layout`: Chromium/Firefox/WebKit, light/dark at
  1440/1024/700/390/320 px. Actual failed rows include long field labels,
  unbroken source values and long error messages. Visible text does not overlap
  the action, mobile actions stack below issues, expanded details fit, results
  paginate, and the ordinary correction form opens above an inert importer.
  This adds the failed-row geometry coverage missing from earlier tests.
- `test:fields:datatype`: 800 real imports in disposable SQLite. Hashes of all
  stored records, values and versions stay identical after Text → Long text.
  Field ID/key stay unchanged, schema revision advances once and sorting is
  disabled. Unsafe/narrowing types and stale bases are rejected. Eight parallel
  conversions have one winner/seven conflicts; required uniqueness survives.
  Three browser engines verify confirmation, multiline editing and stale-sort
  recovery. No deployed user field is automatically converted.
- Existing full importer tests (eight accounts, restart/replay, row correction,
  mapping/history), error popups and Drive/Deployment layout regressions pass
  again in all three engines.

Evidence under `/mnt/ST1000LM049/tests`: `mx-eight-user-jPamNl/import-layout-results.json`,
`mx-eight-user-4WcTXH/field-type-change-results.json`,
`mx-eight-user-TWIOAs/import-results.json`,
`mx-eight-user-Nb58wT/field-errors-results.json` and
`mx-eight-user-zwzxqy/ui-cascade-results.json`. All implementation tests use
disposable storage and disconnected browser audio.

Deployment verification: `deployed-customization-results.json` passes with the
provided operator account in Chromium, Firefox and WebKit. It checks the real
datatype selector and actual failed import log at desktop/mobile widths, using
only schema/log reads; no field or record is changed. Field-error popup smoke
checks also pass. `customization-deployment-data-check.json` verifies matching
pre/post hashes of live fields, records, values, history and Drive metadata.
Release binary/frontend installed with rollback copies; one scoped MX restart,
N1 unchanged. Private screenshots have owner-only permissions.

### Earlier video/Office diagnosis (superseded by the verification section above)

`video-stream-diagnostic.json` reproduces N1 429 responses followed by MX 502
for the same Drive video. Initial bounded head/middle/tail ranges succeed.
MX discards the upstream throttling status/retry information. The N1 read
limiter passes `view_limit_per_second=30` to a builder whose argument is the
replenishment interval in seconds, not requests per second; the bucket is shared
by fragment. N1 source/configuration and services were not changed by diagnosis.

`office-scroll-current-diagnostic.json` uses the actual saved workbook bytes,
served locally after one download to separate rendering from storage throttling.
Opening takes about 132 seconds in headless Chromium. The outer page stays at
approximately 60 fps during the short interaction probe; native input and paint
are asynchronous. Vertical view movement is observed; the attempted horizontal
drag does not establish a changed column, so it is **not** a successful horizontal
scroll qualification. These probes do not establish or fix the reported desktop
scroll freeze. The original/saved workbook revision remains unchanged.

## Visible field configuration errors — 2026-10-15

- Frontend unit suite: 270 passed across 45 files. Rust: 156 passed, two opt-in
  tests ignored. Production build/Svelte check: zero errors and warnings;
  the existing JavaScript chunk-size advisory remains.
- `test:fields:errors`: real disposable release MX/SQLite, fixture N1, production
  frontend; Chromium, Firefox and WebKit pass the archive/recreate workflow.
  Coverage includes immediate popups, preserved drafts, archived-field jumps,
  suggested-key application, stale suggestions, non-key relationship errors,
  existing-field save failures, focus trapping/Escape and 320/390/1440 px
  light/dark popup bounds. Eight concurrent creates commit exactly one field;
  the seven conflicts identify the winner. Archived values and the schema
  revision remain unchanged on rejected creation.
- API checks cover active/archived and case-insensitive duplicates, per-module
  keys, the default module and legacy namespaced long-key collisions.
- Existing archive/dependency dialogs pass in all three engines, including
  changed dependencies after preflight and preservation of historical values.
  Drive row/column alignment and Deployment spacing regressions also pass
  across the same engines, themes and four viewport sizes.

Evidence: `/mnt/ST1000LM049/tests/mx-eight-user-ddaR3W/field-errors-results.json`
(final release binary),
`mx-eight-user-EFGirz/field-archive-results.json` and
`mx-eight-user-kseBvM/ui-cascade-results.json`. All these checks use disposable
data on the external drive, disconnected browser audio and no live N1.

Deployed release smoke checks also pass in Chromium, Firefox and WebKit:
`/mnt/ST1000LM049/tests/deployed-field-errors-results.json`. These use real
authentication/frontend/schema reads but mock the rejected write in-browser;
no deployed field or record is created/edited. Before/after hashes of fields,
records, values, history and Drive item/version metadata match exactly
(`field-errors-deployment-data-check.json`). One scoped MX restart; N1's binary
and running process are unchanged. Rollback copies are
`/mnt/ST1000LM049/mx/litiaina-mx.before-field-errors-fix-20261015` and
`/mnt/ST1000LM049/mx/frontend/dist.before-field-errors-fix-20261015`.

## Office/Drive opening reliability — 2026-10-15

The reported workbook failure was reproduced on the external-drive deployment:
N1 returned the exact 9,079,199-byte original in under a second; the old editor
failed its 60-second opening acknowledgement deadline. Four duplicate native PDF
conversions of the same workbook were also running, consuming about 6 GiB combined.
Those four jobs were stopped without stopping MX/N1 or changing the stored file.

Verification for this scoped fix:

- Rust: `cargo test --offline --locked`, 155 passed, two opt-in tests ignored.
  The host-converter opt-in test was run separately and passed. New regressions
  cover eight duplicate callers, failure cooldown, one-converter admission,
  expanded ZIP budgets and process-group termination on deadline.
- Frontend: 267 tests passed across 44 files. Production build and Svelte check
  report zero errors/warnings; the existing main-chunk size advisory remains.
  Timeout/cancellation tests use fake clocks; ZIP inspection tests verify bounded
  slices rather than a complete-file read or decompression.
- Uploaded communication workbook: real Chromium and Firefox rendering reviewed;
  a keyboard edit was exported with its complete marker and all ten sheet entries.
  Stored original stays at revision 1 and matches its original SHA-256. Opening
  measured roughly 2–3.5 minutes under varying test/build load; export took roughly
  8–16 seconds. No cells/styles were stripped before opening. These checks are not
  a complete fidelity comparison of every cell/style in the workbook.
- Focused actual XLSX saves: native toolbar Save, Ctrl+S, File → Save, pending
  cell input, edits during a delayed commit, original-account re-login after
  revocation, wrong-account rejection and exact exported text passed in Chromium
  and Firefox. Eight editor accounts produce one winning version and seven
  conflicts. Initial Calc input is primed again only after final layout/focus;
  ordinary refocus does not reset a user's unaccepted input.
- Recovery: Chromium/Firefox/WebKit direct Drive launches and linked-file choices
  start no PDF preview. Cancellation before session completion cannot start a
  late editor; Chromium/Firefox cancellation terminates real runtime workers and
  retains the exact original download. Retry opens the actual engine. A synthetic
  20-GiB metadata response triggers streamed original-download fallback without
  buffering that document. WebKit coverage is fallback, not Office editing.
- Actual preview API with disposable N1: eight identical requests return one
  identical PDF; oversized metadata is rejected before any N1 read, and the real
  complex workbook is rejected by the PDF expansion budget without conversion.
- Loading and UI cascade: real five-stage loading, warm reload, original-download
  fallback, Drive row/column alignment and Deployment spacing passed across the
  three browser engines. Tests disconnect audio and block public Internet traffic.

Evidence is under `/mnt/ST1000LM049/tests`: `office-workbook-chromium.json`,
`office-workbook-firefox.json`, their screenshots/private edited copies,
`mx-eight-user-QrPK02/office-results.json`,
`mx-eight-user-w5z0tG/office-recovery-results.json`,
`mx-eight-user-N2MLP8/office-preview-results.json`,
`mx-eight-user-ZmAB8i/office-loading-results.json`, and
`mx-eight-user-BALdWG/ui-cascade-results.json`.

Deployment: release MX binary and production frontend installed on the external
drive with rollback copies. One MX restart; N1 not restarted or modified. Temp
preview work/cache now uses the external drive. This does not certify all Office
formats, mobile hardware, every browser/driver combination or offline recovery
scenario; previous broader qualification limits still apply.

Final deployed HTTPS check: the original workbook opens/renders in Chromium in
about 112 seconds (`office-workbook-deployed.json`), with its original revision
unchanged. Eight real requests for its optional PDF preview return actionable 422
responses in 143 ms total with zero native conversion processes remaining
(`deployed-preview-rejection.json`). No live save/upload or workbook rewrite was
performed by these checks.

## Module import menu recovery — 2026-10-15

- Reproduced the production regression: adding the import details control put
  Refresh and Module actions before Search, and the popover was partly hidden
  beneath the sidebar. Baseline screenshots are under the external-drive tests
  directory, `mx-eight-user-jigpjS`.
- `test:records:import-menu` passes in **Chromium, Firefox and WebKit** at
  1440/1024/390/320 px in light/dark themes. It checks matching control heights,
  square icon sizing, toolbar order, mobile alignment, viewport bounds, stacking,
  repeated open/close, arrow/Home/End navigation, Escape/Tab, click-away,
  mutually exclusive row menus, and both import actions.
- Menu focus is bounded, paint-aware and cancelled when the user navigates;
  repeated Firefox/WebKit openings do not leave keyboard focus on the trigger.
- The full importer workflow and Drive/Deployment layout regressions pass in
  all three engines. **258 frontend tests pass across 42 files**; Svelte check
  reports **0 errors and 0 warnings**, and the production build succeeds with
  the existing chunk-size advisory. No global stylesheet or Drive styling changed.
- Evidence under `/mnt/ST1000LM049/tests`: `mx-eight-user-2x6VTY/module-menu-results.json`,
  `mx-eight-user-TmtX4q/import-results.json`, and `mx-eight-user-LHnzbF/ui-cascade-results.json`.
  Browser audio is disconnected from workstation devices; databases and N1 fixtures
  are isolated. No live records, N1 objects or deployment configuration were altered.
- The verified frontend was atomically deployed to the external-drive MX test
  instance. The previous bundle is retained as
  `/mnt/ST1000LM049/mx/frontend/dist.before-module-menu-fix-20261014`.
  Existing immutable assets remain available for open tabs. MX/N1 binaries did
  not change and neither server required a restart; reload the browser for the fix.

## Module spreadsheet importer — 2026-10-14

- Rust suite: **150 passed, 1 ignored** (live N1 integration remains opt-in).
  Added streaming/parser checks for sparse XLSX dimensions, 1904 dates, cached
  formulas, missing formula caches, ODS repetitions/entities, CSV multiline/CRLF
  positions, UTF-8 cell errors and malformed/oversized shared-string metadata.
  Import authority, log cleanup after account/module deletion and success-marker
  transaction rollback have dedicated tests. Date/time and duration preservation
  are covered, and archive sizes are checked against actual bounded decompression.
- Frontend suite: **258 passed across 42 files**. Svelte check reports **0 errors,
  0 warnings**; the production build passes with the pre-existing chunk-size advisory.
- `test:records:import` passes with a real disposable MX/SQLite server: eight
  accounts × 45 records, eight concurrent requests for one job, server restart
  mid-import, private logs, grant revocation, typed/required/unique/select/date
  failures, correction replay, normal versions and 20-row result pagination.
- An injected SQLite success-marker failure rolls back the entire record write;
  the row stays pending, not incorrectly failed. Removing the fault and resuming
  commits exactly once. Unique-field errors name the offending field.
- Relationship tests exercise ID-based selection, auto-fill preview and saved
  classification, conditional required/hidden fields and denied source-module access.
  Select values are canonicalized so case differences do not bypass conditions;
  a file made required after preview causes a manual-entry skip at commit, with
  no incomplete record created.
- Browser workflows pass in **Chromium, Firefox and WebKit**: mapping, preview,
  progress, a deliberately lost post-commit response, correction and history.
  Light/dark and 1440/390-px layouts are captured; testing found and fixed dialog
  overflow on mobile. Browser audio is disconnected from physical devices.
- Read the supplied 8.7-MB Communication workbook: nine usable sheets and one
  independently flagged October sheet with headers repeated across thousands of
  columns. Its January/February sheets import **2,327 records** using independent
  header rows/mappings into a disposable ten-text-column module. This demonstrates
  file parsing and multi-sheet commits, not automatic inference of business types.
- Existing Drive grid/alignment and Deployment-gap regressions also pass in all
  three engines. No Drive/global cascade styling was changed for this addition.
- All new test databases, fixtures, logs, screenshots, caches and build output
  are under `/home/altear/Temp Environment/mx-import-tests`. Final importer evidence:
  `mx-eight-user-qTn0aW/import-results.json`; layout-regression evidence:
  `mx-eight-user-LCxME1/ui-cascade-results.json`.
- No live MX database/configuration/N1 data was reset or modified. The verified
  frontend is in the temporary directory's `dist`, not the deployment's existing
  `frontend/dist`; build the deployment frontend and restart/rebuild MX to activate.
- Scope: create-only XLSX/ODS/UTF-8 CSV/TSV; no XLS/upsert/fuzzy relationship-name
  matching or automatic attachment migration. Parser budgets and log cleanup are
  documented in [the operator guide](docs/importing-records.md).

## Drive styling recovery and Deployment gap — 2026-10-14

- Reproduced the regression against the withdrawn production bundle: the new
  browser assertion failed because Drive file rows computed to **flex**, not grid.
  Evidence: `/tmp/mx-eight-user-Q7Y4Di/cascade-failure.png`.
- Restored unlayered component styling and removed the application-wide polish
  layer/metrics. Drive component source matches its pre-polish version. Earlier
  feature changes are retained; no live database/configuration/N1 changes.
- Deployment now uses independent section columns. Branding and Terminology
  have a normal 16-px section gap instead of sharing a row with tall Appearance.
  Only Deployment controls receive its scoped sizing adjustments.
- `npm --prefix frontend test`: **253 tests pass across 41 files**. Three new
  source-level cascade/scope guards replace four withdrawn polish assertions.
- Production build passes with zero Svelte errors/warnings. The existing main
  JavaScript chunk-size advisory remains; frontend/offline assets are rebuilt.
- `test:ui:cascade` passes in Chromium, Firefox and WebKit: list rows remain grids,
  file columns align, sharing dialogs retain their dimensions, Deployment gaps
  and control heights align, and both pages fit 1440/1024/390/320 px in light/dark.
  Artifacts: `/tmp/mx-eight-user-O0pkAh/ui-cascade-results.json` and screenshots.
- `test:drive:file-manager` passes in all three engines: real uploads, image
  preview, Ctrl/Shift selection, drag-selection, context menus, copy/cut/paste,
  clipboard-image upload, file/folder drops, aligned columns, activity tabs and
  cleanup across module navigation. Folder entry readers are simulated; the MX
  drop/upload handlers and stored metadata are real. Artifacts:
  `/tmp/mx-eight-user-sNZArI/file-manager-results.json`.
- `test:drive:polish` also passes in all three engines: 135-item pagination,
  cross-page previews with bounded 256-KiB text reads, people/space sharing,
  membership-revocation cleanup, guest pagination and shared-root ACL confinement.
  Artifacts: `/tmp/mx-eight-user-QM21K4/`.
- A read-only request to the running local MX server confirms it serves the
  rebuilt recovery frontend, not the withdrawn bundle. Existing open tabs still
  need reloading to replace their in-memory JavaScript and styles.
- Tests use temporary MX/SQLite and an N1 HTTP contract fixture. UI browser
  processes are disconnected from workstation audio servers; no devices/sinks
  are created or changed. These checks do not constitute full release qualification.

## Withdrawn UI polish: insufficient layout coverage — 2026-10-13

**Superseded by the 2026-10-14 recovery.** The test runs below passed, but missed
a genuine production-style regression: Drive rows became flex rows instead of
their file-column grid. Component CSS was emitted before the named layer order,
so shared styles took precedence. Input dimensions and lack of overflow were not
adequate acceptance criteria for a file manager. The broad polish, its stylesheet
and its test command have been withdrawn; these artifacts are historical evidence,
not proof that the withdrawn UI was correct. The replacement suite checks grids,
column alignment and the exact Branding-to-Terminology gap.

- `npm --prefix frontend test`: **254 tests pass across 41 files**, including
  shared control metrics, density/text scaling, multiline/compact exceptions and
  existing application regressions.
- `npm --prefix frontend run build`: Office assets verified; Svelte reports zero
  errors/warnings; production frontend and offline manifests rebuilt. The existing
  Vite advisory about the main JavaScript chunk exceeding 500 kB remains.
- `npm --prefix frontend run test:ui:polish`: real disposable MX API/SQLite forms
  in Chromium, Firefox and WebKit. Twelve screens in light/dark themes at 1440,
  1024, 390 and 320 px: Deployment, audit, schema, rules, reporting configuration,
  account creation, dashboard filters, preferences, record editing, Drive, sharing
  and sign-in. Check actual theme tokens, consistent control fonts/heights,
  unclipped values, viewport containment and dialog geometry after layout settles.
- Deployment checks also cover compact/normal/comfortable density, large base
  text with 85%/160% interface scaling, top-aligned fieldsets, identical text/select/
  color-control edges and heights, predictable gaps and open mobile navigation.
  Exercise actual Deployment identity and record saves; compare stored results.
- Cross-browser regressions pass for native dropdown keyboard selection, inset
  chevrons, system themes/high contrast, Drive roles/guest expiries/sorting/pages,
  named field-archive dialogs and the reachable horizontal record scrollbar.
  The modular form-rule suite also passes in all three engines, including its
  eight-account stale-edit/branch checks and held-upload admission race.
- Screenshots reviewed for desktop/mobile Deployment, schema, rules, records and
  sharing. This is UI/regression evidence, not complete release qualification or
  new live-N1/network/Office-device testing. All test writes use temporary SQLite
  and an N1 HTTP fixture. No deployment data/configuration or audio devices changed.

Artifacts:

- UI layout: `/tmp/mx-eight-user-JLULzd/ui-polish-results.json` and per-screen PNGs.
- Dropdowns/sharing: `/tmp/mx-eight-user-mbmTP0/`.
- Field archive: `/tmp/mx-eight-user-qzM1b3/`.
- Record scrolling: `/tmp/mx-eight-user-mYIkSj/`.
- Form rules: `/tmp/mx-eight-user-tZQ8iQ/`.

## User-authorized existing OFF/LEAVE configuration — 2026-10-13

- Identified the existing Inventory Status Select with OFF/LEAVE choices through
  the authenticated application API. Added optional Start date and End date Date
  fields, and configured the existing File Attachments visibility, using Status
  equals OFF for all three details. Retained other fields and their requirements.
- Read back all three rules from the running server. Actual new-record form
  checks pass in Chromium, Firefox and WebKit: OFF shows the file field and both
  dates; LEAVE hides them; Status stays available to change the selection.
- This was an explicitly requested configuration change, not a reset or a test
  against production record contents. No business records were saved/deleted,
  no attachments were uploaded/deleted, and no runtime configuration or N1 code
  was changed. Credentials were supplied in memory without terminal echo and
  were not written to documentation or screenshots.

Evidence: `/tmp/mx-off-setup-LwyVol/`, with new-form-only OFF/LEAVE screenshots.
This verifies visibility configuration, not a new approval workflow, mandatory
file-byte enforcement, date-range validation or file-transfer acceptance test.

## Documentation organization — 2026-10-13

- Documentation-only follow-up: README reduced from 2,578 to 86 lines. Detailed
  material moved into topic references under `docs/`; operator, relationship,
  conditional-form and deployment walkthroughs added alongside a documentation index.
- Checked operator labels against the current module, relationship and login
  components. Explain Lookup versus editable copy, the single/multiple relationship
  setting, unsupported copy destinations, current CSV scope and save-error placement.
- Read-only local-link/heading validation: **19 Markdown documents, 114 local links,
  zero missing targets or anchors**.
  Recheck links when moving or renaming documentation headings.
- `git diff --check` passes. No application code or frontend build changed in this
  documentation pass, no destructive commands ran and no UI-error-placement fix
  is claimed. The subsequent user-authorized module setup is recorded separately above.

## Modular conditional forms — 2026-10-13

- Rust: **140 passed, one opt-in live-N1 test ignored**. Rule tests cover nested
  predicates, typed numeric/Boolean zero/false values, stable option labels/copies,
  explicit overrides, cascading mappings, exclusive groups, required details,
  dependent options, disabled writes, full-replacement semantics, named cycles,
  missing/competing controllers and rejection of scripts/file requirements.
- Frontend: **250 passed across 40 files**. Shared-engine checks cover local
  predicates, choice filtering, requirements, multiple/cascading fills, explicit
  overrides, exclusivity and clear detection. Production build passes with zero
  Svelte errors/warnings; Vite's existing large-chunk advisory remains.
- `npm run test:forms:rules` passes with real disposable MX HTTP/SQLite and a
  contract N1 fixture. Create two different configurable modules; verify relationship
  copies can drive conditions, nested predicates, choices plus multiple ordinary
  cells, zero/false, unavailable options, direct API validation, PATCH/PUT and normal
  history. Old versions violating tightened current requirements return an explained
  conflict. Stored files remain after switching branches.
- Eight accounts save different fields from one revision: all merge. A newer
  detail cannot be erased by a stale parent switch; its conflict includes current
  controls for cancelling that switch. Eight competing switches have one winner
  and seven conflicts. Stale dependent edits/uploads are rejected. Administrator
  configuration is protected; archiving names form-condition/mapping dependencies.
- Hold a **17 MiB multipart** part in fixture N1 after upload admission, change
  the branch, then finish the transfer. Final metadata admission returns a named
  `409`; no attachment is admitted, the upload's own orphan is cleaned, and existing
  objects are retained. This is a contract/race test, not a live-N1 durability test.
- Actual UI passes in **Chromium, Firefox and WebKit**: focused configuration,
  in-dialog cycle error, option-to-field mapping through controls, checkbox/radio
  presentation, immediate local copies, relationship-driven available choices,
  required branch details, cancelling/confirming a clear, typed zero-value save,
  inline choice opening a completion form and exclusive Boolean groups. Verify
  cancelling a radio switch leaves the original button selected. A typed numeric
  control hides its detail but does not clear the saved value until the user confirms
  Save. Keeping latest branch-conflict details also restores the current controls
  while preserving an unrelated local note.
- Validate light/dark and 390px rule-dialog layout and focused keyboard entry.
  Existing relationship auto-fill, field archive and record-summary suites also
  pass in all three engines. No deployment database/configuration, N1 repository,
  live objects or audio devices were changed; test audio stays disconnected.

Artifacts: `/tmp/mx-eight-user-sS0G9w/form-rules-results.json` and rule screenshots.
Regression artifacts: `/tmp/mx-eight-user-vS8vY1/relationship-autofill-results.json`,
`/tmp/mx-eight-user-M8ruUZ/field-archive-results.json` and
`/tmp/mx-eight-user-DUngrK/record-summaries-results.json`.

Limits: calculated/file fields support visibility only; dynamic file-byte requirements
are not atomic with record saves. Rules are not read-access isolation or an automatic
old-row migration. This focused matrix is not blanket production, mobile-device,
million-row performance or unreliable-network certification.

## Field archive dependency dialog — 2026-10-13

- Rust: **125 passed, one opt-in live-N1 test ignored**. New tests cover named
  cross-module dependencies without confusing duplicate keys, inactive dependents,
  lookup/rollup and auto-fill references, formula references, active/trashed link
  counts, storage folders/prefixes and attachment ownership. Archive checks do
  not remove data. Rust build passes without warnings.
- Frontend: **240 passed across 39 files**. Structured archive failures must
  match the requested field; old unstructured and network errors remain distinct.
  Production build/check passes with zero Svelte errors/warnings; Vite's existing
  large-chunk advisory remains.
- `npm run test:fields:archive`: real disposable MX HTTP/SQLite and fixture N1.
  Verify all blocker categories through real schema/record/file/storage APIs,
  eight concurrent checks without a schema revision change, administrator-only
  access, missing-field handling and retained historical values after a safe archive.
- Actual UI passes in **Chromium, Firefox and WebKit**: popup names the module
  and dependent fields, explains the fix, disables unsafe archive, and jumps to
  the exact settings with focus/highlight. Test light/dark/390px layout with balanced
  viewport margins, forward/reverse Tab trapping, Escape/opener focus restoration,
  failed-check retry inside the dialog and absence of an off-screen page error.
- Add a dependency after a successful preflight and attempt Archive: the commit
  rejects it and updates the same dialog with the new named blocker. Archive only
  succeeds after explicitly removing that dependency and checking again. Keep the
  popup pending until the refreshed field card displays its archived/Restore state.
- Relationship auto-fill regression also passes in all three engines, including
  eight-account field merging/conflicts, immediate previews, drafts and inline
  per-field conflict decisions. No deployment configuration/database, N1 repository,
  live objects or workstation audio devices were changed; test audio stays disconnected.

Artifacts: `/tmp/mx-eight-user-SF8mB2/field-archive-results.json` and screenshots;
auto-fill regression `/tmp/mx-eight-user-etDL8u/relationship-autofill-results.json`.

One intermediate UI run produced a blank initial page; it did not reproduce in
the subsequent Chromium check, cross-browser auto-fill regression or final full
archive-dialog rerun. This focused validation is not a blanket deployment or
unreliable-network certification.

## Relationship auto-fill — 2026-10-13

- `cargo test --locked --bin litiaina-mx -- --test-threads=2`: **122 passed,
  one opt-in live-N1 test ignored**. Validate mapping structure, compatible
  types, unique destinations and single-selection requirements. Rust build passes.
- `npm test`: **238 passed across 38 files**. Verify ordinary revision-based
  editing and pure preview application, preservation of typing during requests,
  null/zero/false handling and click isolation while an inline save is pending.
  Frontend build/check passes with zero Svelte errors/warnings; Vite's existing
  large-chunk advisory remains.
- `npm run test:relationships:autofill`: disposable real MX HTTP/SQLite with
  an N1 fixture, not deployment configuration or live storage. Verify configured
  source/destination mappings, numeric/boolean/date copies including zero/false,
  current formula values, missing required data, invalid dropdown values, explicit
  overrides, foreign/missing selections, mapped-field archive dependencies and
  denial of inaccessible/unauthenticated source reads. Previously saved copies
  remain historical after source and formula changes.
- Eight editor accounts concurrently change independent fields successfully.
  Eight competing relationship changes produce one winner and seven conflicts;
  copied Classification fields are included in those conflicts. Relationship
  and copies share the normal atomic record version, not new collaboration state.
  This is real HTTP concurrency, not eight simultaneous browser tabs.
- Actual production UI passes in **Chromium, Firefox and WebKit**: save mappings
  in administration; select/change/clear names; immediate copies and live lookup
  previews; form and inline saves; delayed obsolete responses; preserved typing
  and draft overrides; interrupted-draft retry; failed-preview save blocking;
  reset during a held response; and separate stored/mine choices for two copied
  field conflicts. Inspect light/dark and 390px layouts. Early tests caught a
  missing JSON request header, draft-startup timing and cell-click bubbling during
  an in-flight commit; all were corrected before the passing rerun.
- Column-summary regression suite also passes in all three engines, including
  paging/filtering, totals, inline edits and concurrent snapshot reads/writes.
  Browser audio uses disconnected per-process endpoints; no workstation audio,
  deployment database/configuration, N1 repository or live objects were changed.

Artifacts: `/tmp/mx-eight-user-ciyomp/relationship-autofill-results.json` and
screenshots; summary regression `/tmp/mx-eight-user-fOPyoU/record-summaries-results.json`.
Copied data follows destination-module access after saving; it is not redacted
when source access later changes. Use read-only lookups for live source values.

## Configurable record-column summaries — 2026-10-13

- `cargo test --locked --bin litiaina-mx -- --test-threads=2`: **120 passed, one
  opt-in live-N1 test ignored**. New tests validate summary configuration, scalar
  aggregates, null/zero/negative values, module/trash exclusion, empty results and
  current formula definitions over 300 records spanning several bounded batches.
- `npm test`: **235 passed across 37 files**. Summary rendering distinguishes
  an unavailable result, an empty average and genuine zero; display precision
  and whole-number counts are verified. Rust build and frontend build/check
  pass; zero Svelte errors/warnings, with Vite's existing large-chunk advisory.
- `npm run test:records:summaries`: disposable real MX HTTP/SQLite and the N1
  fixture. Verify 120 records across three pages (50 + 50 + 20), matching totals
  on every page, search/field filters, null/negative/zero values, empty sums/counts
  versus empty averages, delete/restore and a changed formula definition without
  relying on its older stored results. Unauthorized reads/configuration and
  unsupported field functions/precision are rejected.
- Eight simultaneous read requests run alongside seven different editor-account
  commits. Every returned full-page dataset has a matching record count and
  numeric/formula totals from one SQLite read snapshot. An initial migration
  write inside that snapshot caused a locked-database failure; revision and
  attachment schema initialization was moved outside it, with read-only helpers
  inside. The concurrent rerun passes. This is HTTP concurrency, not eight UI tabs.
- Actual UI passes in **Chromium, Firefox and WebKit**: administrator settings,
  aligned column footer, paging/search updates, real inline edits using different
  values per browser, live deletion/restoration and empty filtered results. Inspect
  light/dark and 390px layouts after navigation transitions settle. An initial
  mobile footer clipped numeric values; row sizing/overflow were corrected and
  element-bound checks now verify that each number fits inside its cell/footer.
- Test browsers remain disconnected from physical audio endpoints. No live N1
  data, deployment database/configuration or module relationships were changed.

Artifacts: `/tmp/mx-eight-user-zEt6R1/record-summaries-results.json` and screenshots.
Configured formulas are recalculated in 128-record batches, while plain numeric
fields use SQL aggregates; this is not a million-row latency benchmark. These
table summaries use existing numeric precision, not a new fixed-point ledger.

## Detailed Office loading — 2026-10-13

- `npm test`: **233 passed across 36 files**. Added loading-stage/format feedback,
  byte-unit and valid-total checks, real streamed-download progress, unknown or
  compressed totals and retry resets without counting error bodies as file bytes.
  Existing file-transfer idle-deadline and byte-preservation tests still pass.
- `npm run build`: zero Svelte errors/warnings, bundled Office checksums verified,
  and rebuilt `frontend/dist` with an updated offline-cache generation. Vite's
  existing large-chunk advisory remains. No Office vendor binary was changed.
- `npm run test:office:loading`: disposable real MX/SQLite, the N1 contract fixture
  and loopback-only traffic. Chromium/Firefox open a real DOCX in the pinned
  WebAssembly engine and show all five stages. Hold metadata and the engine
  script, split/pause an actual document download and delay only the real worker's
  ready/open acknowledgements to inspect loading feedback deterministically.
- Received document bytes and a partial measured percentage appear even while
  the server pauses after the first chunk; an initial UI-throttle bug found by
  this check was fixed with a trailing update. Unmeasured startup has no invented
  percentage. Inspect light/dark/390px layouts; the ready view has no loading UI
  and retains the 38px toolbar/862px canvas stage in a 900px viewport. Warm reload
  opens the real document again without creating another saved version.
- Missing/inaccessible-file failures stop animation and remain stopped after
  dismissing the error banner. WebKit/WPE's **existing unsupported worker graphics**
  are rejected before engine startup; the exact loaded original remains
  downloadable. This is fallback validation, not a WebKit Office editing pass.
- Browsers use disconnected per-process audio endpoints and ALSA null output.
  No deployment SQLite/configuration/certificates, live N1 data or desktop audio
  settings were changed. Module relationships/lookup behavior are unchanged.

Artifacts: `/tmp/mx-eight-user-llyzkq/office-loading-results.json` and screenshots.
This focused loading suite does not clear the existing Office native input,
Firefox rendering, presentation-editing or live-storage qualification gates.

## MX 4.1.1 release packaging — 2026-10-11

Rust/frontend manifests and both lockfiles are aligned on **4.1.1**. This patch
packages the sharing, preview-navigation and upload-recovery fixes below. The
4.1.1 binary/frontend were rebuilt and the Rust, frontend and sharing/navigation
suites rerun after the preview fix. N1 compatibility stays at v4.0.0. Historical
validation entries retain their tested version, and remaining release gates are
not cleared by this packaging change.

## Shared-video preview navigation regression — 2026-10-11

- Reproduced the bug in the actual production UI before the fix: open a shared
  video, close it, switch to a record module and return to Drive. The old target
  was fetched again, failing the no-reopening assertion. `history.pushState`
  did not emit `hashchange`, leaving the previous shared-file target in memory.
- Synchronize route targets before destination mounting, clear the `item` URL
  parameter when dismissing a preview and abort/invalidate obsolete metadata
  requests when a link changes, the user navigates or the Drive view unmounts.
- `npm run test:drive:sharing` passes on rebuilt **MX 4.1.1** in Chromium,
  Firefox and WebKit. Use a generated one-second silent VP8 video, real MX HTTP/
  SQLite and the isolated N1 contract fixture. Verify decoded video and muted
  playback, Records/Dashboard/Collaboration round trips, reload, back/forward,
  Close/Escape/backdrop dismissal and repeated explicit opening of the same card.
- Hold real metadata responses to simulate latency, then leave the module,
  select another Drive view or open a different link. Late responses do not
  reopen the old video or issue its preview ticket; a newer preview stays selected.
- `npm test`: **223 passed across 35 files**. Rust tests: **116 passed, one
  opt-in live-N1 test ignored**. Rust build and frontend build/check pass; zero
  Svelte errors/warnings, with the existing Vite large-chunk advisory unchanged.

Before-fix evidence: `/tmp/mx-eight-user-D60nUG/sharing-results.json`.
After-fix evidence: `/tmp/mx-eight-user-zBsfBS/sharing-results.json` and screenshots.
The browsers remain audio-isolated. This does not qualify audible call media,
live N1 bandwidth or the separate native WebKit upload limitation below.

## Drive share delivery — 2026-10-11

- `cargo test --locked --bin litiaina-mx -- --test-threads=2`: **116 passed,
  one opt-in live-N1 test ignored**. New tests cover transactional rollback
  when notification insertion fails, direct-chat reuse, unchanged-grant replay,
  role updates, current ACL resolution and group muted/mentions-only settings.
- `npm test`: **223 passed across 35 files**. `npm run build` passed with zero
  Svelte errors/warnings; Vite retains its existing large-chunk advisory.
- `npm run test:drive:sharing`: real isolated MX/SQLite and the N1 contract
  fixture, eight accounts, simultaneous personal shares and eight identical
  group requests. Exactly one message per changed grant, no duplicate retry
  delivery, no N1 byte-transfer requests during sharing, notifications/messages
  preserved after an MX restart, and unauthorized nonmember/nonowner denial.
- Verified 56 conversation shares on two server pages (50 + 6), metadata search,
  and discovery of older personal/group grants without backfilling messages.
- Actual production UI in **Chromium, Firefox and WebKit**: share confirmation,
  recipient toast → conversation → Drive preview, group file cards and current
  Drive list, light/dark rendering and narrow-screen layout. Revoking all grants
  updates the card to unavailable, removes the list entry and denies preview
  ticket issuance. A second valid grant correctly keeps access until revoked.
- Test browsers use disconnected per-process PulseAudio/PipeWire endpoints and
  an ALSA null configuration. No physical audio output or workstation audio
  settings are used. Existing global alert preferences are reused; audible
  speaker playback/OS notification permissions were not manually qualified.

Latest artifacts: `/tmp/mx-eight-user-zBsfBS/sharing-results.json` and screenshots.
This suite validates sharing against the N1 fixture, not a live N1 service.
The existing WebKit native-file upload limitation below is not resolved by
these sharing tests; browser uploads and call-media release gates are separate.

## Drive upload recovery and scheduling — 2026-10-11

Implemented automatic transient recovery, explicit waiting/pause/reselection/
confirmation states, account-scoped persistent metadata and stable operation
IDs. File bytes are not persisted: reopening requires the original file.
Inactive pending reservations default to 72 hours, configurable from 1 to
168 hours; matching, reserved quota and garbage collection share that window.
Files above 1 GiB ramp from console's four workers toward eight after successful
acknowledgements, with eight shared part slots per browser document. Server
admission waits briefly before body buffering without raising its existing
eight-transfer limit. Console part sizes are unchanged; there is no full-file
pre-scan/read-back or new per-file size ceiling.

### Results and limits

- `npm test`: **223 passed across 35 files**. Covers bounded/adaptive scheduling,
  immutable retries, late-worker failure, FIFO slot cancellation, terminal errors,
  offline/manual-pause behavior, per-account/per-transfer recovery, multiple tabs,
  more than one thousand saved entries, partial small-file batches, source loss,
  blocked storage and cancellation. Native synchronous send failures release
  timers/listeners; no endless retry of unreadable files.
- `cargo test --locked`: **113 passed, one opt-in live-N1 test ignored**
  (114 total). Added interrupted-body 408 versus oversize 413 checks, the unchanged
  eight-transfer bound, multipart-policy boundaries, configured retention and
  failed-reservation recovery/quota checks, and cancellation rollback/error
  propagation. Cancel is acknowledged only after its SQLite transaction commits.
  `cargo build --locked` passed.
- Production frontend check/build passed: **zero Svelte errors/warnings**.
  Vite retains its existing >500-KB main-chunk notice. Rebuilt `frontend/dist`
  is included; functional tests ran on 4.1.0 before the 4.1.1 packaging bump.
- Chromium and Firefox pass real production UI against disposable MX/SQLite and
  the **N1 HTTP contract fixture**: 18-MiB native disk-backed files, module switches,
  offline waiting with acknowledged progress/no stale ETA, manual pause that
  survives reconnect/reload, wrong-original rejection, original-file reselection,
  two-hour-old retryable reservation recovery and automatic reconnect after an
  actual MX restart. A committed but held/lost finish response stays Confirming,
  then reconciles to exactly one file version. Completed queue metadata is removed;
  no upload reappears after reload. Final stored bytes match SHA-256 in the test.
- Eight simultaneous authenticated HTTP clients pass real multipart initialization,
  retry, part-driven retention renewal, finish and finish replay. Each has exactly
  one normal MX version and byte-for-byte stored content. Repeated cancellation
  releases the exact pending reservation without deleting completed files.
  This is **not eight simultaneous browser tabs**
  or a real multi-gigabyte/live-N1 bandwidth benchmark.
- **WebKit/WPE full native-file recovery is not qualified.** Selected source files
  become unreadable (`NotFoundError`), and native PUTs can send zero bytes. A
  minimal control outside the MX queue reproduces source loss after metadata
  initialization, using either fetch or XHR; retaining a Blob slice, delaying
  input cleanup or retaining the picker did not establish a full pass. Root cause
  remains unresolved. The separate source-loss guard test passes: show Select
  original file, stop automatic replay and publish no empty/truncated file.
  This limited guard is not a Safari/macOS/iOS or full WebKit upload pass.
- Controlled virtual 2-GiB scheduling with 100-ms part acknowledgements improves
  from 1,100 ms/four workers to 700 ms/eight workers (41 parts), while two jobs
  still peak at eight shared slots. These are **simulated scheduler timings**, not
  measured MB/s, CPU/memory qualification or a speed guarantee on an actual network.

Evidence (private test directories may contain test credentials/database data;
do not publish them wholesale):

- `/tmp/mx-eight-user-iq5ypw`: final Chromium/Firefox recovery UI and eight-client
  run; `recovery-results.json` records scope, fixture storage and
  `productionQualified:false`. Includes part-driven retention renewal, repeated
  pending cancellation and completed-file protection, actual desktop dark-palette
  assertions and inspected desktop dark/mobile light screenshots.
- `/tmp/mx-eight-user-XlbeJk`: final WebKit source-loss guard only plus eight HTTP clients;
  `fullRecoveryQualified:false`. This is not a successful WebKit file upload.
- `/tmp/mx-eight-user-pGXjIk`, `/tmp/mx-eight-user-wSrBio`: failed full WebKit runs,
  including zero-byte part rejections and unreadable native source evidence.
- `/tmp/mx-eight-user-5Pj5rX`: Chromium/Firefox UI passed, but the later direct
  test-only timestamp write hit SQLite's zero-default busy timeout. This combined
  run is not a pass; the isolated inspection helper now uses the same 10-second
  busy timeout as the disposable MX server. `/tmp/mx-eight-user-5D2ilH` passed the
  source-loss guard/eight-client retention-renewal scope, not full WebKit recovery.
- `/tmp/mx-upload-scheduling-benchmark.json`: controlled scheduler evidence, not
  real network throughput. Logs: `/tmp/mx-upload-unit-final.log`,
  `/tmp/mx-upload-rust-final.log`, `/tmp/mx-upload-frontend-final-build.log`.

Run `npm run test:drive:recovery` from `frontend` after building Rust/frontend.
`MX_TEST_BROWSERS=chromium,firefox` selects the passing full scope; the default
three-engine run retains the WebKit gate. Only
`MX_TEST_BROWSERS=webkit MX_TEST_RECOVERY_SOURCE_FALLBACK=1` selects the separate
limited safe-failure check. These tests do not use or change deployment config,
secrets, databases, certificates/trust, desktop audio devices or the N1 repository.
They use fixture storage, **not live N1**. Browser closure does not continue uploads;
retained MX metadata cannot guarantee survival of N1 parts outside N1's own
durability/retention settings. Existing Office/call/live-storage release gates
below remain open; this is not complete production qualification.

## MX 4.1.0 relationships and standalone Office — 2026-10-10

Status: native module relationships and the bundled browser Office integration
are implemented. Earlier real DOCX/XLSX editing, immutable MX-version saves and prepared
offline recovery passed in Chromium and Firefox. **The expanded current Office
save-UX acceptance run is not green**: freshly opened spreadsheets intermittently
lose leading typed characters, and a Firefox run also showed a blank native
canvas. Exported-text assertions catch these failures even when MX commits the
snapshot successfully. Save feedback is implemented and its normal/delayed-save
checks pass, but native input/rendering remains a release blocker; do not treat
the unit-test/build passes as complete Office qualification. **Presentation editing is not
qualified**: PPTX input tests failed and PPT/PPTX/ODP are now explicitly read-only.
The tested WebKit/WPE runtime lacks WebGL in workers; only its graceful download
fallback passed. This is not complete Office-suite or production certification.
The older 4.0.0 call/network/live-storage release gates below remain open.

Version packaging: Rust/frontend manifests and both lockfiles agree on **4.1.0**;
N1 compatibility remains **4.0.0 / storage format 4**. `cargo test --locked`
(108 passed, one opt-in live-N1 test ignored) and `cargo build --locked` passed
for the version update. The latest dropdown-inset follow-up reran all 202
frontend tests and the production frontend check/build. Svelte reports zero
errors/warnings; Vite still reports
its existing large main-chunk notice. This metadata/UI update does not clear the
Office release blockers above or relabel earlier storage/call tests as new passes.

### Checked scope

| Check | Result and limits |
| --- | --- |
| Rust | `cargo test --locked`: 108 passed, one opt-in live-N1 test ignored (109 total). Includes relationship edge/lifecycle integrity, explicit Office formats and existing Drive CAS/ACL/quota/idempotency regressions. |
| Frontend | `npm test`: 202 passed across 33 files. Includes shared dropdown inset/non-repetition/theme/high-contrast guards, record-scroll geometry/clamping, relationship requests, format/read-only gates, clipboard permission compatibility, serialized checkpoint/save work, native module-specific Open/Save dispatch boundaries, Calc input initialization and synchronous acceptance/selection restoration, native-focus acknowledgements, save/offline feedback, actual-worker capability selection and public-only offline-cache lifecycle/update tests. Svelte check/build: zero errors/warnings; Vite retains its >500-KB main-bundle notice. The earlier full npm audit reports zero advisories after the targeted build-only dependency patch. This does not audit the Office binary. |
| Native dropdown UI | `npm run test:drive:dropdowns` passes Chromium, Firefox and WebKit on the production UI with real isolated MX/auth/SQLite. Dashboard, module settings, report builder and account creation cover all visible selects in explicit light/dark and system-light/system-dark themes at 1440/390px widths. Checks verify exactly one non-repeating chevron inset 12px with at least 32px end padding, and native arrows without image overlays under emulated forced colors in all three engines. Reporting period keyboard input and committed Navigation icon changes are verified. Drive coverage includes actual keyboard choices for proposed/user/group roles and all four guest expiries against committed server ACLs/timestamps, all sort values and 25/50/100-item pages over a 55-item folder per engine. Native dropdown Escape keeps Share open, while Escape from another dialog control closes it. No page errors or horizontal body overflow; screenshots inspected. This is not Safari/iOS/device or live-N1 qualification. |
| Relationships/API | Eight independent stale-field writes merge; eight same-field writes have one winner and seven conflicts. 55 concurrent creates remain valid, linked IDs are indexed, all five rollups match expected values, selectors use 50-item pages, and target ACLs redact read/history/conflict/CSV data. Invalid references, referenced deletion/trash/module deletion and dependent-field archival are rejected. |
| Relationships/UI | Real form, inline, lookup/rollup, paged related-record and administrator controls passed in Chromium, Firefox and WebKit, including light/dark and 390px views. This uses real MX/SQLite and an N1 contract fixture, not live N1. |
| Reachable record scrolling | Chromium, Firefox and WebKit pass with a real 60-record module/14 visible fields and 50-item pages. Always-visible thumb dragging, Home/End/Left/Right, two-way table synchronization, vertical reachability, dark/light resizing, native-bottom handoff, retained offset, nested clipping, editor-dialog hiding, mobile cards, column visibility, next-page loading and navigation cleanup verified. The slim control appears only when horizontal overflow exists and the normal bottom scrollbar is off-screen; it creates no records/API edit activity. Screenshots inspected. |
| Office/version concurrency | Eight authenticated editor accounts prepare saves from one file revision. Simultaneous multipart finishes yield exactly one winner, seven revision conflicts and two normal versions (original plus winning save). All Office-open requests remain ACL checked. |
| Office editing | Real pinned WebAssembly engine; actual keyboard edits; DOCX XML checked after save; XLSX XML checked for the edited cell and preserved `A2*B2`/170000 formula result. Native Calc toolbar Save publishes a new N1 object. Distinct storage keys/normal version history verified; viewer typing cannot publish changes. Every public-internet HTTP request is blocked during these checks. |
| Office offline/retry | Explicit IndexedDB retention; no private API/root/credentials cached by the worker; edit with browser offline; close the tab and cold-open the cached engine/document against an actual TCP outage; reconnect and save. Lost finish acknowledgements survive reload and reconcile to exactly one version. Stale saves expose deliberate conflict choices; resolution preserves earlier versions. Save As also survives response loss/reload and creates one separate file without changing the original. |
| Office save UX | Actual native Calc toolbar Save, Ctrl+S and File → Save exercised, including input not yet accepted with Enter. A real multipart-finish request is held: UI stays Saving/Confirming, no success indication, and revision stays unchanged until release. Typing newer cell input during this hold must remain explicitly unsaved after the older snapshot is acknowledged; a second save verifies its actual exported text. Revision/time feedback and unchanged-save/no-extra-version checks pass in normal flows. Full-run early-input failures below remain blockers. |
| Office compact shell | Targeted Chromium/Firefox pass with the real bundled engine: one 38-pixel non-wrapping MX bar, no extra rows/footer, light/dark/390px layouts, menu focus/Escape/outside-click, actual DOCX keyboard edit/export, held Ctrl+S commit (no premature success), distinct-object revision save, Download and opt-in offline retention. Menus, dirty/saving feedback and offline options leave canvas dimensions unchanged. Genuine Chromium TLS rejection remains hidden in options until requested. This does not clear the existing XLSX input/Firefox rendering or full-suite qualification gates. |
| Office authentication/TLS | Sign-in is absent while authenticated and appears on actual server session revocation; cancel/wrong-account/re-login paths exercised. Some runs pass exact retained-text checks, but repeated full runs catch intermittent leading-character loss; do not qualify the entire recovery scenario as stable. Genuine untrusted-TLS service-worker rejection produces collapsed guidance in Chromium. Firefox WebDriver's certificate bypass also permits its worker, so that setup is not a certificate-rejection test. No deployment certificate/trust was changed. |
| Office browser limits | Earlier Chromium/Firefox editing passes do not clear the expanded current input/rendering gate. Presentation rendering/read-only/download is separate from editing. WebKit's real worker graphics probe rejects unsupported rendering before loading the large engine; Download returns the exact unchanged original and no JS crash. This is not a Safari/macOS/iOS editing test. |

All new storage checks above use the disposable **HTTP N1 contract fixture**, with
transient part failures. No live N1 instance was listening on the configured test
ports in this continuation. These results must not be relabeled as live-N1 passes.
Tests did not touch deployment `mx.env`, `mx.config` or the production SQLite
database, install an Office server, change system certificate trust, or use
microphones/speakers. The one-time presentation fixture authoring tool used an
existing host LibreOffice only to generate synthetic test data; deployment and
the browser runner do not need it.

### Evidence and remaining gates

New artifacts are in private temporary directories (may contain isolated test
tokens/database contents; do not publish them wholesale):

- `/tmp/mx-eight-user-CUiQ1d`: final dropdown-inset regression pass in Chromium,
  Firefox and WebKit. Each engine passes the 32 core theme/viewport layouts,
  an additional emulated forced-colors check, and the Drive checks below.
  Computed styles verify one non-repeating 12px-inset chevron and reserved text
  space; forced colors restores native arrows. Desktop Navigation icon has more
  grid space without changing the mobile layout. Screenshots inspected; all 202
  frontend tests and the production check/build pass. This is an isolated
  contract-fixture UI test, not live-N1 or full release qualification.
- `/tmp/mx-eight-user-CdmAHi`: final expanded shared-dropdown regression pass
  after removing dark/system SVG overrides and obsolete arrow spacing. Each of
  Chromium, Firefox and WebKit covers 32 dashboard/schema/report-builder/account
  theme/viewport layouts, plus the existing Drive checks. Native keyboard input,
  persisted Navigation icon and grant/expiry metadata are verified; screenshots
  inspected. All 200 frontend tests and the production check/build pass.
  `/tmp/mx-eight-user-nT4FgF` reproduced the repeated-arrow failure on the old
  production dashboard before the fix. Earlier Drive-only coverage below did
  not catch the shared dark/system rules: local Drive background overrides
  masked them. Do not label that earlier scope as a full-app dropdown audit.
- `/tmp/mx-eight-user-5PsSVC`: final 4.1.0 three-engine dropdown regression pass,
  recorded in `dropdown-results.json`; screenshots inspected. Browser processes
  use disconnected/null audio backends; no desktop audio, trust, deployment
  configuration or N1 repository was changed. The earlier
  `/tmp/mx-eight-user-BVZO1A` caught Firefox Escape closing Share while dismissing
  a native select; the dialog handler was fixed. `/tmp/mx-eight-user-KZ6OrZ`
  passed Chromium/Firefox but raced a WebKit ACL read before its grant POST was
  acknowledged; the runner now awaits the real commit response. Neither earlier
  combined run is labeled as a full pass.
- `/tmp/mx-eight-user-nhZ232`: relationship API/eight-user and three-engine UI pass.
- `/tmp/mx-eight-user-BcWZ4O`: compact Office shell-only Chromium/Firefox pass;
  screenshots inspected. `office-results.json` marks the limited scope and
  `productionQualified:false`; this is not a replacement for full Office acceptance.
- `/tmp/mx-eight-user-mNyH8Z`: final three-engine reachable-record-scroll pass,
  including nested clipping; `record-scroll-results.json` records the exact scope.
  Browsers are disconnected from desktop audio servers with null audio fallbacks;
  no devices/defaults/volume/system trust/deployment data were changed.
  `/tmp/mx-eight-user-HtYcNT` passed Chromium/Firefox but the WebKit process failed
  while initializing a disconnected audio backend. The per-process OpenAL null
  backend fixed that test-environment failure; `/tmp/mx-eight-user-A71CVK` and the
  final combined run passed WebKit. The initial failed run is not a full pass.
- `/tmp/mx-eight-user-DZ5XV2`: current-build Chromium completed the full scenario,
  including exact authentication-recovery text; Firefox then failed an XLSX
  exported-text assertion and its native canvas appeared blank. This combined
  run is **not a pass**. `/tmp/mx-eight-user-RUbca8` repeated Chromium and failed
  its exact retained-text assertion, so the first Chromium success is not a
  reliable clearance of the initial-input gate.
- `/tmp/mx-eight-user-u0iQzd` and `/tmp/mx-eight-user-vS9dRL`: full runs passed
  normal/delayed native save, offline/retry/conflict checks before failing exact
  early spreadsheet text retention. Native-frame activation, selection refresh
  and event-queue experiments did not establish a consistently green run;
  ineffective experimental toolkit changes were removed.
- `/tmp/mx-eight-user-iOuQja`: diagnostic export proved missing text before
  authentication was revoked, separating native input loss from MX authentication
  recovery. Diagnostic/focused passes such as `/tmp/mx-eight-user-y25we6` are
  not full-suite release passes. Input timelines contain synthetic fixture keys,
  not deployment credentials.
- `/tmp/mx-eight-user-wrbskV`: WebKit worker-graphics/download fallback pass,
  not an editing pass.
- `/tmp/mx-eight-user-Vxo1MK`: final Chromium/Firefox Office pass on the polished
  build: real edit/export/version checks, cold TCP-outage reopening, retained
  normal/Save As retries across reload, explicit conflicts, read-only gates and
  visible keyboard-operable narrow-screen scrolling controls. Screenshots were
  visually inspected; phone controls are scrollable, not a mobile-native editor.
- `/tmp/mx-eight-user-NcyeWa`: final WebKit worker-capability and exact unchanged
  download fallback pass, with the corrected failure status (not a stuck
  "Starting" message or misleading certificate error).
- `/tmp/mx-eight-user-FkJm8q`: Chromium and Firefox real Office editing, full
  disconnected/reload/retry/conflict/Save As coverage, formula/native Save and
  read-only gates. This run precedes the final narrow-canvas/failure-message polish.
- `/tmp/mx-eight-user-D4Rqpr`: WebKit worker-capability/download fallback pass,
  not an editing pass.
- `/tmp/mx-eight-user-ZMFuNP`: earlier full Firefox DOCX/offline/idempotency/conflict
  and XLSX/native Save pass after the checkpoint queue fix.
- Failed presentation input runs `/tmp/mx-eight-user-9S9ITq`,
  `/tmp/mx-eight-user-1pZTBg` (save interception removed diagnostically), and
  `/tmp/mx-eight-user-2EItmf` (native focus diagnostic) remain failure evidence.
  Rendered slides/available export filters were not counted as successful edits.
- `/tmp/mx-eight-user-TVLh4u` reproduced the reported native File → Open crash.
  Expanded canvas checks then caught incorrect module-specific Open aliases,
  shortcut-state/resize mismatches and stale Calc input handling. Earlier failed
  diagnostic runs, including `/tmp/mx-eight-user-amhLjW` and
  `/tmp/mx-eight-user-rgFfLM`, are not passes. Native-cell navigation in the final
  runner waits for the real UNO-driven visible cell indicator before typing,
  rather than equating delivery of a synthetic arrow key with native UI readiness.
- `/tmp/mx-eight-user-DTith2`: pre-guard WebKit worker graphics initialization
  failure. `/tmp/mx-eight-user-yqVDnf`: pre-fix Save/checkpoint race caught after
  successful real offline recovery. These are not passing runs.

Before production distribution, review the pinned **2025-05-13** upstream engine's
current security/support status, exact corresponding source and **complete**
third-party/font distribution notices. Bundled core license texts and checksum
verification are not that audit. Office is approximately 131 MiB compressed in
the deployment package (Brotli plus gzip alternatives); the decoded engine/data
alone are approximately 250 MiB, before document/runtime memory. Test weak client
hardware, mobile/native-canvas accessibility and complex/legacy-format fidelity
on target devices. The editor currently starts from Drive, not record/chat
attachment modals; it does not implement real-time paragraph/cell co-editing.
The build-only `source-map-js` dependency was updated from 1.2.1 to 1.2.2 for
[GHSA-68fv-2mgg-jv7q](https://github.com/advisories/GHSA-68fv-2mgg-jv7q);
this targeted lockfile update is not an Office engine security qualification.

Reproduce editing on the validated engines after backend/frontend builds:

```bash
cargo build --locked
cd frontend
npm run build
MX_TEST_BROWSERS=chromium,firefox npm run test:office:browser
```

The separate unsupported-runtime check is
`MX_TEST_BROWSERS=webkit MX_TEST_OFFICE_GRAPHICS_FALLBACK=1 npm run test:office:browser`.
Use `MX_TEST_WEBKIT` when the installed WebKit requires a launcher wrapper. This
mode asserts fallback only, and the results JSON records that distinction.
The current expanded Chromium/Firefox run must pass repeatedly before clearing
the Office input/rendering gate. `MX_TEST_OFFICE_INPUT_DIAGNOSTIC=1` downloads and
inspects a pre-revocation snapshot to localize input loss; it changes the operation
sequence and must not substitute for the default acceptance scenario.
The default three-engine editing run is **not** claimed green for this WebKit
runtime. `MX_TEST_OFFICE_FORMATS_ONLY=1` is a focused diagnostic mode, not the full
offline/concurrency suite. No Office test requires an audio sink.

The compact-shell regression command is
`MX_TEST_BROWSERS=chromium,firefox MX_TEST_OFFICE_SHELL_ONLY=1 npm run test:office:browser`.
Reachable record-table scrolling is checked with `npm run test:records:scroll`.

## MX 4.0.0 baseline validation — 2026-10-09

Status: Drive pagination/share/preview polish and file-manager regressions passed
in Chromium, Firefox and WebKit on the built production UI with isolated storage
fixtures. Earlier file-manager UI passed on all three engines against live N1.
The current saved live-N1 credentials were rejected with HTTP 400, "Fragment not
found"; the new polish run is not a live-storage pass.
The latest combined stress run passed 15 API scenarios and Chromium/Firefox UI,
but failed WebKit's large-file transfer after its network process crashed.
Full eight-person call validation is also incomplete. This is not production
certification or a claim of Google Drive feature parity.
The 4.0.0 version designation does not resolve these outstanding deployment gates.

Release packaging check: Rust/frontend manifests and lockfiles agree on 4.0.0;
default branding and the rebuilt frontend use Digital Workplace Platform. Rust
tests (105 passed, one opt-in ignored), frontend tests (164 passed) and the
frontend check/build were rerun after the version/default-branding changes.
Browser evidence below is from the preceding feature-validation runs; browser,
physical-device and live-N1 stress tests were not rerun for this metadata update.

## Confirmed changes

- Drive lists return SQL-bounded metadata pages (25/50/100), stable sorting,
  result counts, numbered/jump navigation and empty-last-page clamping. Folder
  pickers and public guest lists use bounded pages too. Owned rows no longer
  perform one extra star query each; storage usage is aggregated once per request.
- Preview neighbors seek across list pages, with keyboard/button navigation and
  only selected-file bytes. Text previews request at most 256 KiB and cancel the
  stream at that bound. Preview focus stays inside the dialog; mobile controls,
  headers and truncation messages are visually inspected.
- Space/group grants inherit through folders and follow current membership.
  Leaving/removal/archive/revoke removes that access without revoking independent
  grants. User/space role controls and names/emails no longer collapse in Share.
  A member's real UI opens/previews the shared folder; refreshing after leaving
  clears stale rows, returns to Shared with me, and explains why.
- Deleted-row barriers protect against stale lists, module changes, out-of-order
  lifecycle events, and server sequence restarts. Explicit restore/reconciliation
  lift the barrier.
- Trash/restore advance revisions atomically and invalidate old drafts. Full PUT
  requires the loaded base_revision (428 when absent, 409 when stale); partial
  PATCH still merges unrelated fields.
- Calls attach tracks granted after peer discovery. Unhealthy connected transports
  attempt ICE recovery and then a revisioned fresh peer pair; old transport signals
  cannot replace that pair. Microphone/shared audio use separate playback elements.
- MX Drive owns metadata/ACL/quota/version/trash state, with immutable original-
  extension N1 keys, durable multipart resume, small-file batches, and persistent
  background N1 soft-delete/cancellation jobs.
- Live N1 caught a genuine compatibility error: durable part acknowledgements are
  plain text, not JSON. This is fixed and covered by a Rust regression.
- An expanded UI test caught saving before a Drive copy finished, which could
  omit the attachment. Save/send/close/reset now guard the pending copy; the
  three-engine test deliberately delays that download and attempts an early save.
- Drive commits recheck quota after expired reservations; revoked permissions,
  canceled uploads, and permanent deletion cannot publish a stale reservation.
  Preview tickets pin versions; guest requests reject changed revisions.
- Drive no longer imposes a per-file cap. Console-compatible part sizing is
  covered at every boundary, including 50 MiB above 1 GiB; existing resumable
  sessions retain their negotiated size. Uploads now initialize using metadata
  only, with no file pre-scan; finalization confirms committed N1 version/size
  without a full-object read-back. Per-part replay receipts remain enforced.
- Uploads belong to the signed-in workspace, not Drive's component. A global
  lower-right panel shows native in-flight progress, smoothed MB/s and ETA, with
  pause/resume across modules and account-exit cleanup. Browser close/reload
  warns for active work; reopening requires reselecting the original file.
- Administrators can assign per-user storage allowances or restore the 10-GB
  default. Revisions prevent stale administrator overwrites, a persistent audit
  records changes, and commits recheck the current owner's allowance (including
  shared-editor versions). Lowering an allowance never deletes existing files.
- The legacy raw-fragment-equals-secret configuration is normalized in memory
  before N1 authentication; already-hashed identifiers remain unchanged.
- Drive has checkbox/Ctrl/Cmd/Shift/marquee selection, scoped context menus,
  copy/cut/paste, external file/image paste and folder drops. Header/row columns
  share fixed tracks. Versions and Activity are separate views; upload progress
  is a ring around the icon. A mobile menu scroll-anchoring dismissal, stale
  native clipboard marker race, and WebKit file-entry fallback were caught and fixed.
- Drive copies/restores create independent MX version metadata over immutable
  content, without downloading/re-uploading bytes. Quotas count logical copies;
  referenced objects are excluded from cleanup. Transfers are revision-checked,
  transactionally committed and payload-bound/idempotent; tickets pin this item's
  exact version rather than looking up an arbitrary shared content key.
- Large-file investigation reproduced a Linux WebKit network-process crash
  without request interception. It erased session storage and invalidated file
  handles without unloading the page. Active-document token caching now retains
  authorization, with explicit signout/revocation tests; it does not repair
  crashed browser file handles. MX still rejects empty/truncated parts rather
  than committing corrupt content. An isolated 128-MiB WebKit rerun passed, but
  repeated combined runs failed: this remains a browser-runtime release risk.

## Completed checks

| Validation | Result and scope |
| --- | --- |
| Rust | `cargo test`: 105 passed, one opt-in live-N1 test ignored by default. Includes a 10,123-file-per-owner namespace, page bounds/clamping, all four sorts and neighbor tie ordering, tenant isolation, inherited space permissions, membership/archive/revocation boundaries, populated legacy schema migration, reference-safe copy deletion, recursive selection deduplication, quota checks, idempotent replay/no resurrection and atomic move rollback. |
| Frontend | `npm test`: 164 passed across 23 files. Includes page/search/sort/neighbor request scopes, space-grant payloads, unauthenticated scoped guest navigation, metadata-first large uploads, native binary retries/progress/cancellation, queue lifetime, selection, directory readers and account-scoped clipboard cleanup. Active-document authentication survives lost browser storage; explicit sign-out, rejected refresh and replacement-login race protections remain tested. Check/build: zero Svelte errors/warnings; production build passed on Node 24.21.0. Vite retains the >500-KB bundle warning. |
| Drive polish UI | `npm run test:drive:polish`: all three engines passed with 135 files per folder, 25/50-item pages, numbered/page-jump controls, visible page size, empty-page reconciliation after concurrent moves, cross-page button/keyboard previews, bounded 256-KiB text ranges, mobile preview/focus and desktop light/dark/mobile Share geometry. Actual HTTP user/space grants, inherited editor access, self-leave revocation, guest pagination/cross-page navigation and root-escape denial passed. Synthetic metadata fixtures reference one real HTTP-uploaded object; list pages do not read any storage bytes. Current storage backend is the explicit N1 contract fixture, not live N1. |
| Drive file-manager UI | Focused production-UI suite passed in Chromium, Firefox and WebKit against live N1, also repeated with the contract fixture. Real keyboard Copy/Cut/Paste/select-all, checkbox focus, toolbar Copy → immediate keyboard Paste, repeated paste with name collisions, copied-image preview, Shift and rectangle selection, viewport-clamped mobile menus, aligned columns, Activity tab, clipboard survival across module navigation and no global context-menu override. External image/file paste/drop traversed the real UI and HTTP flow; OS clipboard/directory entries were simulated (Firefox discards synthetic ClipboardEvent file data). Nested/empty folders and pasting an image while a deliberately held metadata action was still busy were verified. |
| Drive transfers/live N1 | Eight concurrent copy replays produced one resource; eight owners copied independently without access leaks. Eight moves from one base had one CAS winner. Large-file copy and version restoration sent no N1 byte reads/writes. Purging a copy kept the original usable, purging the original kept its copy usable after MX restart/GC, and replay never resurrected a purged copy. Copied/restored preview tickets pinned the correct version and streamed exact ranges. |
| Drive API/live N1 | Eight simultaneous owners, private default-deny even for administrators, CAS rename conflicts, original-extension keys, MX-only rename/trash, scoped guest links, exact ranges, immediate revocation, cancel/purge replay protection. |
| Drive multipart/live N1 | 18-MiB upload in 4-MiB parts, injected 503 retry, disposable MX restart, reconciliation skips already stored parts, checksum-identical download. N1 finalized successfully before its acknowledgement was deliberately dropped; MX recovered the existing version. A 130-MiB upload completed; a >1-GiB session negotiated 50-MiB parts and accepted one complete 50-MiB part before deliberate cancellation (not a complete >1-GiB upload test). |
| Drive quotas/live N1 | 10-GB default; non-admin reads/writes rejected; invalid assignments rejected; eight simultaneous administrator writes had one winner; assignments persisted across restart. Reducing a quota to zero blocked both pending commit and new upload; default reset and persistent audit verified. |
| N1 credential compatibility/live N1 | Disposable MX was deliberately configured with the raw fragment in both settings. Uploads authenticated and completed against live N1 without changing the deployment or N1 repository. |
| Drive batches/live N1 | Multiple small files, per-entry filename conflict, dropped N1 batch acknowledgement, checksum reconciliation, and same-operation replay without another N1 batch publication. |
| Drive background cleanup | N1 DELETE was faulted with 503. MX purge returned promptly, metadata stayed removed, jobs survived MX restart, and deletion completed after N1 recovered. No N1 permanent-delete API was used. |
| Drive browser UI | Earlier three-engine run `DoE1Jt` passed 128-MiB uploads with every Blob.arrayBuffer read forbidden, no hash worker, real MB/s/ETA, lower-right panel, pause/resume across unmounted modules, lost finish-response recovery, active-only reload warning and composer clearance. Latest combined run `HHT91t` repeated these plus file-manager controls successfully in Chromium/Firefox, then failed WebKit's large upload after a browser network-process crash invalidated file handles. Do not treat the current combined suite as green. |
| Folder/version coverage | All three engines uploaded a nested folder and verified the MX hierarchy; API tests verified original extensions per version, byte-stable old preview tickets, and expired-link rejection. |
| Version UI | All three engines uploaded a new file version, previewed an older version above the history dialog, and restored it through the confirmation dialog as a third normal version. |
| Drive attachment integration | All three engines chose Drive files through the real message/record pickers and saved attachments. Three distinct storage keys were verified. Record/chat copies remained downloadable after the Drive original was trashed. |
| Eight-account records | Unrelated saves merged with history; same-field writes had one winner/seven exact conflicts; conflict resolution was audited. 64 retried record POSTs created exactly eight resources; same for messages. |
| Record lifecycle/API | Concurrent module updates/stale writes could not revive deleted records, cleared cells, or archived fields. Explicit restore rejected pre-delete drafts. Eight full replacements yielded one winner/seven conflicts. |
| Record/collaboration uploads/live N1 | Eight 18-MiB multipart uploads and a 94-MiB six-part upload, injected 503/lost-finalize responses, exact bytes/ranges, and eight independent record attachment additions from one base revision. |
| Independent live N1 Rust test | The ignored round-trip test was run explicitly and passed: one-shot, 11-MiB/three-part upload, ranges, rename/trash/recovery, six concurrent storage-prefix writes. |
| Cross-browser network UI | 18 scenarios passed (six per engine): startup outage/reconnect, retained record create identity across reload, clipped action menus/mobile/inline base, chat retry, aborted native file body recovery, deleted-row ordering/reconciliation. |
| Native WebRTC/network | 14 passed: four signaling-loss variants per engine, plus two three-person mixed-engine jitter/source-transition scenarios. Tests checked native RTP and changing camera/screen pixels, not only connection state. |

## Retained artifacts

Recent evidence is under the private directory
`/home/altear/.cache/mx-release-tests-20261008.OIde4y`:

- `mx-eight-user-LGzaeV`: final three-engine polish pass with the group member's
  own UI and revoked-folder recovery, after the final frontend/backend builds.
  `mx-eight-user-URHZy4`: three-engine pass after metadata query optimization.
- `mx-eight-user-RUFRlb`: three-engine production Drive polish pass and screenshots;
  `mx-eight-user-PzM7R4`: repeated three-engine file-manager pass after preview focus
  and bounded-text changes. Earlier polish failures exposed test-fixture timing,
  expected-204 handling and cross-engine duplicate-name setup; they are retained,
  not counted as passing runs.
- `mx-eight-user-mJC2js`: focused file-manager UI passed on all three engines,
  with screenshots and explicit production interaction assertions.
- `mx-eight-user-OsHElJ`: final focused file-manager UI passed on all three
  engines against live N1 after the final build. Includes both keyboard and
  toolbar Copy/Cut availability during a held metadata write, external paste
  taking precedence over that internal clipboard, and every selection/drop/menu
  regression in the file-manager helper.
- `mx-eight-user-gEVguq`: earlier focused file-manager UI passed on all three
  engines against live N1, including external paste during a held metadata write.
- `mx-eight-user-HHT91t`: latest combined run; 15 live-N1 API scenarios and
  Chromium/Firefox full UI passed. WebKit failed during its 128-MiB upload;
  failure diagnostics verify that retries retained authentication after storage
  loss, but the browser's invalidated file handles submitted rejected parts.
- `mx-eight-user-E2CSWo`, `hsGdKB`, `y9NLyX`: isolated WebKit large-upload probes
  without request interception. First two reproduced native network-process/file
  handle loss (the second retained authorization after the cache fix); third
  completed all 128 MiB and finalization. Diagnostics omit credentials/tickets.
- `mx-eight-user-zedAXW`, `hjN8Br`, `a7s9xQ`, `sQ9Duu`: retained failed combined
  and WebKit-only stress runs, before active-document credential hardening.
- `mx-eight-user-Qzu6fI`: 15 live API scenarios passed; the combined UI run
  exposed the paste-while-metadata-busy race, retained as pre-fix evidence.
- File-manager investigations retained failed runs `5tF03s`, `OYYYWo`, `cKbmW2`,
  `nz8kcD`, `ZT4Qqi`, `Q3Sebq`, and `gPF79J` (all prefixed `mx-eight-user-`).
  These caught harness viewport/OS-entry differences as well as real mobile-menu,
  copy/clipboard timing and WebKit ordinary-drop compatibility issues.

- `mx-eight-user-DoE1Jt`: 15-scenario live-N1/API/three-engine UI pass for the
  metadata-first upload flow and persistent lower-right queue. 128-MiB upload
  per browser resumed after Drive unmounted and completed in collaboration;
  MB/s/ETA, pause, lost finish acknowledgement, close warning, composer clearance,
  nested folders, version restore, and independent record/chat copies passed.
- `mx-eight-user-a87i42`: failed UI run retained. The expanded completed queue
  intercepted file actions; fixed with automatic collapse after successful work.
- `mx-eight-user-obwRYx`: failed UI run retained. The collapsed queue covered
  Send; fixed by tracking the actual composer bounds and floating above it.
- `mx-eight-user-JyYeEg`: earlier 15-scenario live-N1/API/three-engine UI pass
  for uncapped Drive uploads and per-user quota controls, including 128-MiB UI
  upload per engine, grid preview cards, mobile administrator dialog, and
  independent record/chat copies. All 47 unique test objects were soft-deleted
  and verified stream-404 afterward. Screenshots and cleanup evidence retained.
- `mx-eight-user-nXk9R7`: 12 updated API/live-N1 scenarios passed, including
  130-MiB complete upload, console-size part acceptance, raw-fragment compatibility,
  eight-administrator quota contention, mid-upload quota reduction, and reset.
- `mx-eight-user-KUGCm9`: 12 API scenarios and Chromium UI passed; the following
  Firefox attempt exposed a test wait racing the initial storage-meter request.
  Retained as failed-run evidence; the assertion now waits for the actual quota.
- `mx-eight-user-t0S2lz`: final 12-case live Drive/API/production UI pass,
  including nested folders, history preview/restore, delayed-copy save guards,
  and independent record/chat copies. All 41 unique test objects were soft-deleted
  and checked for stream 404 afterward.
- `mx-eight-user-0fgaC5`: 11 Drive API/browser scenarios passed against live N1,
  including independent record/chat copies on all three engines.
- `mx-eight-user-Ijjz6x`: expanded 12-case live Drive pass, including nested
  folder upload, version/range/expiry checks, and the early-save copy race.
- `mx-eight-user-43nuwI`: failed early-save attachment-copy race, retained as
  evidence of the UI issue before the copy guards were added.
- `mx-eight-user-YMqTCc`: earlier 11-case live Drive pass.
- `mx-eight-user-ELhN20`: failed live durable-part acknowledgement test,
  retained as pre-fix evidence.
- `mx-eight-user-XI16cd`: failed Firefox image test. The synthetic PNG had an
  invalid IDAT CRC; replaced with an actual browser-canvas PNG. All engines
  subsequently decoded it successfully.
- `mx-eight-user-Kza5EL`: 14 record/API/live-storage scenarios, including the
  94-MiB upload; 17 unique test keys soft-deleted and stream-404 checked afterward.
- `mx-network-qNPtoF`: 14 native WebRTC/network scenarios passed.
- `mx-network-TfkJPq`: 18 cross-browser UI/network scenarios passed.
- `mx-eight-user-Eq5QKv`: substantial eight-person call stages passed (below),
  then a leave-request harness error. This run is still marked failed.
- `mx-eight-user-0C0sua`: call harness startup timing failure; fixed, but a full
  green eight-person rerun remains pending.

Earlier failures and investigations remain under
`/home/altear/.cache/mx-network-audit-2026-10-08.fnCQvP`, including the WebKit
eight-user page crash and pre-fix record resurrection/audio-playback failures.
Never publish complete artifact directories: they include synthetic-account
credentials and TLS private keys. Failed evidence is deliberately retained.

## Safety, limitations, and remaining release work

Browser runners refuse physical/default audio devices and require an explicitly
named `mx_test_*` sink verified as owned by `module-null-sink`. A temporary
test-only null sink was created on this host and removed after the final run,
with routing scoped to browser child processes and an ALSA-null fallback.
Physical defaults, volume, and audio services
were not changed. Playback probes used isolated synthetic tones, not physical
microphones/cameras. Remove only the owned test module after browsers exit.
Eight-person media tests still consume significant CPU even when silent.

The most recent eight-person call run passed 56 directed peer connections,
112 sender-screen/camera/viewer transitions, independent actual microphone/shared
audio playback, mute isolation, device-panel/light-dark UI, compact dock, and
fullscreen exit. It then failed on the harness's incorrectly encoded leave
request. That request and a later harness startup race were corrected, but
repeated rejoin, same-account replacement, reconnect soak, and final cleanup have
not passed together as a complete run. The eight-user WebKit crash is unresolved.
Do not describe the full call suite as green.

Tests used Node 24.21.0, Chrome for Testing 149, Firefox 155, and Linux Playwright
WebKit 26.6. Physical devices, Safari/iOS, actual capture-picker/system audio,
TURN topology, real UDP loss, and prolonged deployment stress remain unverified.
Linux WebKit is not Safari certification.

Drive has no desktop sync/offline outbox/Google Docs editing. Image thumbnails
currently load the original image (up to 10 MiB), not server-generated thumbnails.
Drive uploads do not pre-hash or re-download whole files. Drive-to-Drive copies
and version restoration are metadata-only; record/chat attachment copies still
buffer a file in the browser and retain their separate limits. Shared lists poll
every 15 seconds, not instant Drive-specific events.
Shared editors cannot create children in another owner's folders. Old versions
and activity currently show the latest 100 entries. Live N1 tests prove the
exercised workflows, not all storage failure modes.

Production MX database/configuration and the N1/console repositories were not
modified by these tests. Existing unrelated edits in those repositories remain.
Test cleanup sends N1 soft DELETE only for its unique keys and checks stream 404;
physical trash remains until N1 retention/GC. Rebuild/deploy frontend and backend
together; no production service was restarted by this work.
