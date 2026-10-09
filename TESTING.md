# MX 4.0.0 validation — 2026-10-09

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
