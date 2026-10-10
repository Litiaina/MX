# MX validation

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
