# Changelog

This file records user-visible changes to Litiaina MX. Release dates use the
deployment maintainer's local date.

## [Unreleased]

## [1.0.0-rc.11] - 2026-10-28

Release candidate, not stable 1.0.0. The reviewed source passed the extended
100-account SSD workload, with the hardware and Office limits retained in
[the qualification report](docs/concurrency-qualification.md). See the
[upgrade and rollback instructions](docs/releases/1.0.0-rc.11.md).

### Concurrent records, Drive and notifications

- Coordinate Drive metadata writes, cleanup, quotas and notification read
  receipts through the same FIFO SQLite writer as record saves. Keep pure reads
  on the independent connection pool and N1 requests outside database turns.
- Reserve/reconcile upload metadata before taking N1 transfer slots, and release
  slots before metadata commits. Reject invalid initialization metadata before
  admission. Preserve the eight-part memory cap and existing admission deadline.
- Yield notification delivery between atomic activities using bounded soft
  work slices. Keep recipient inserts and outbox retirement in the same commit;
  retain current permission/preference checks and crash recovery.
- Reuse prepared notification statements with a bounded writer statement cache,
  without caching authorization results across transactions. Add non-sensitive
  slow-operation diagnostics distinguishing queue/pool waits from execution.
- Persist account-scoped read-all watermarks instead of rewriting every inbox
  payload and index entry. Preserve individual/legacy read times, unread counts,
  current visibility checks and new deliveries across read-all and restart.
- Seek notification pages and acknowledged unread ranges through existing
  indexes. Evaluate module grants once per query, retaining live record/field
  visibility checks and stable catch-up cursors instead of sorting whole inboxes
  during concurrent reconnects. Add large-history and permission regressions.
- Move passive SQLite checkpoints to a dedicated worker, retaining automatic
  checkpoint fallback if it fails. Use FULL-synchronous WAL commits so a save's
  WAL sync is independent of checkpoint timing; no physical-power-loss claim.
- Commit bounded import slices in one writer turn with per-row savepoints.
  Invalid rows remain inspectable without partial records; infrastructure
  failures roll back the unacknowledged slice and preserve pending rows.
- Batch audit writes through a bounded queue and up to 32 events per transaction.
  Requesters still await persistence confirmation; audit logging is not changed
  into a fire-and-forget success claim.
- Coordinate Collaboration channel, membership, message, reaction, pin and read
  mutations with the same writer. Make membership/history and pin/tombstone
  changes atomic; retain pooled reads when reusing existing direct conversations.
- Coordinate module/field administration and personal preference mutations with
  the shared writer. Keep viewing account grants read-only, including accounts
  without a revision row. Retain revision-conflict and permission checks.
- Reconcile new Drive sessions in the initialization commit instead of taking
  another writer turn for the reply. Fill bounded audit batches after writer
  admission so arrivals during contention can share the same durable commit.
- Interleave bounded notification-delivery turns with FIFO foreground writes.
  Neither a busy foreground queue nor a delivery backlog can monopolize writer
  admission; recipient checks, atomic job retirement and commit durability remain.
- Expand concurrency verification with simultaneous notification receipts and
  edits, timestamped real Office save overlap, complete acknowledgement ledgers,
  and historical version/attachment hash checks after process restart. Failed
  baselines remain recorded alongside the passing qualification.

### Office appearance and release qualification

- Share MX light/dark tokens, configured accents, readable text sizing and reduced
  motion with the Office loading screen, document controls and read-only viewers.
  Preserve the compact toolbar and native document canvas/input geometry.
- Use the configured MX logo for Office and attachment browser tabs as well as
  the workspace. Resolve relative branding paths from the MX root, remove stale
  icon MIME hints, and fall back safely when a custom image cannot load.
- Open the native document window out of view and reveal it after loading,
  avoiding intermediate visible layouts. This does not claim instant loading
  for complex workbooks or replace the bundled document engine.
- Add real-workbook cold/warm browser timings and a mixed-user qualification
  scenario covering concurrent records, random attachment bytes, Drive versions,
  live connections and a real graphical Office edit/save witness. Keep failed
  runs as failures and document the large-workbook rendering limit explicitly.

### Product theme and compact workspaces

- Rework records as console-style tables with sentence-case headers, quiet row
  separators, aligned numeric values, readable type and personal color/badge
  settings. Save settings per account with stale-save protection; preserve
  restricted-field boundaries and complete long text.
- Replace green-tinted dark surfaces with neutral charcoal and an adaptive blue
  default accent. Retain the product-page light palette and explicit custom colors.
- Add reduced-motion-aware view/dialog fades and module-browser outside dismissal.
  Guard unsaved module/appearance drafts and keep failed-save focus in the dialog.
- Preserve personal appearance across deployment-refresh/reconnect races. Reflow
  column settings at enlarged text sizes without clipping fields or save controls.
- Align MX with its product preview: warm white/sage surfaces, restrained
  terracotta accent, readable typography and consistent navigation/form chrome.
  Add the MX Sage deployment preset while preserving saved organization colors,
  existing presets, personal overrides and the original logo assets.
- Replace tall module banners with a compact title/action row and slim view tabs.
  Keep complete module/view instructions behind the Info control. Refine real
  pipeline cards, directories, operations summaries and inline book headings.
- Apply coordinated light/dark/system palettes to the shell, records, Drive,
  Collaboration, notifications, forms and administration; retain enlarged text,
  personal bold fields, configured layouts and existing permission boundaries.
- Anchor record dialogs while asynchronous process information arrives; retain
  keyboard focus and guarded Escape closing. Do not change record-save, upload,
  workflow-decision or posted-balance semantics.
- Preserve configured branding and publish matching executable/frontend assets.
  Publishing a release does not itself deploy or restart production servers.

## [1.0.0-rc.10] - 2026-10-27

### Concurrent saves and truthful conflicts

- Serialize supported record, workflow, book and notification writes through
  fair writer admission while retaining independent read connections. Recheck
  current record/module permission after waiting rather than trusting old grants.
- Keep record/message/workflow notification work in a durable outbox instead of
  performing recipient fan-out during foreground saves. Batch delivery, recheck
  current privacy/preferences and recover pending work after process restart.
- Preserve ordered public/private live events and route private deliveries only
  to their account streams. Bound password-verification workers and queueing.
- Fix hidden legacy control-number collisions on new records even when no field
  is Unique. Permit repeated non-unique values within/across modules; reconcile
  missing/stale internal sequence state without changing existing records.
- Name an explicitly unique field when create or edit rejects a duplicate;
  retain atomic rollback and log unexpected stored-data constraints privately.
- Return committed import progress in bounded batches under interactive load;
  keep invalid-row inspection and never silently skip infrastructure failures.

### File and interface reliability

- Give new record-attachment attempts independent immutable N1 object keys so a
  losing same-name upload cannot delete or overwrite the winning file's bytes.
- Admit N1 uploads before reading disk-spooled payloads or opening upstream
  connections. Default to eight active transfers, configurable from 1 to 64;
  do not change N1's own source, rate limits or storage encoding.
- Preserve upstream busy/timeout status and Retry-After on supported upload
  paths instead of misreporting them as generic 502 errors. Remove the conflicting
  Linux TCP timer while retaining explicit application upload deadlines.
- Preserve selected modules and unsaved administration drafts during automatic
  refresh/live reconnect. Retain explicit Refresh and read-only status refreshes.

Qualification uses real API/WebSocket traffic and N1 on a single project HDD;
100-user passes are not a 1,000-user or physical power-loss claim. See
[the exact evidence and limits](docs/concurrency-qualification.md) and
[RC.10 upgrade instructions](docs/releases/1.0.0-rc.10.md). Existing records,
relationships, file references and signed-update publisher trust are preserved.

## [1.0.0-rc.9] - 2026-10-26

### Configurable module workspaces

- Add administrator-configured opening views and ordered, named module tabs:
  process pipeline boards, card directories, operations overviews, document
  libraries, full-page accounting/inventory and the existing Records view.
- Offer editable starting layouts for CRM, operations, accounting, stock,
  document management and information systems. Do not seed business policy,
  modules or records; existing modules keep Records until explicitly configured.
- Add configurable card fields, overview summary tiles and named record-detail
  sections. Keep unassigned fields visible and preserve conditional form rules.
- Bound pipeline stage pages to 40 cards and directories/document registers to
  24 records. Read permission-filtered stage totals with one grouped query;
  limit stage loading to two concurrent requests and flag live board changes.
- Save layouts with optimistic revisions, administrator history and named
  field-archive dependencies. Preserve business records, process decisions,
  posted balances and existing server-enforced access restrictions.
- Open module configuration from its workspace with that exact module selected.
  Preserve the existing searchable module library and navigation order.
- Cancel record reads on page exit and component destruction; reject delayed
  schema responses before they can start another fetch during navigation/reload.

### Opt-in module processes

- Add a configurable process layer above records: stable stages, permitted
  transitions, creator/separate-reviewer policies, required fields/files and
  action notes. CRM pipelines, ERP purchase approvals and accounting expense
  approvals are editable configuration drafts, not preinstalled business modules.
- Add Administration → Modules & fields → Process & actions, module Work queues,
  record actions and paginated decision history with exact saved snapshots.
  Explain missing requirements and creator-policy restrictions beside actions.
- Separate record-link intent from in-flight record opens: allow reopening a
  record after queue review and ignore stale responses after closing/navigating.
- Apply actions with record/process/configuration revision checks and durable,
  payload-bound operation deduplication in one SQLite transaction. Save creator
  and eligible-next-reviewer notifications in that same transaction; respect
  module/row permissions and Workflow changes notification preferences.
- Optionally restart a process when record values, files or restored versions
  advance the record revision. Old decisions retain their snapshot and labels;
  they do not silently approve new values. Enabled approval drafts use this policy.
- Advance saved record revisions when formula recalculation actually changes
  stored values (including clearing them); unchanged calculations are no-ops.
  This prevents stale decisions from approving recalculated data accidentally.
- Preserve blank installations and existing modules; explicitly confirm enabling
  a process for existing records. Initialize new tables/indexes at startup,
  never on workflow reads; bound queues/history to 40 entries per page.

### Operational books

- Add optional per-module accounting and stock books with their own explicit
  operator policy. Keep fresh installations blank and existing records unchanged.
- Add exact decimal-string/scaled-integer journal posting, account balances,
  preserved line evidence, one-time reversing journals and irreversible period
  cutoffs. Reject unbalanced, overflowing or over-precision amounts atomically.
- Add explicit stock receipts/issues, reservations, release and consumption.
  Prevent negative available stock and concurrent overselling in one transaction.
- Add book settings, catalog definitions, confirmation forms, 40-entry paginated
  histories and inline failures. Retain payload-bound retry receipts in the tab;
  distinguish committed, rejected and unconfirmed operations honestly.
- Optionally require an unchanged source record and completed process stage for
  posting; capture its stored-value/file snapshot in the same transaction.
- Protect operational evidence against update/delete, including permanent module
  deletion. Book operators need explicit module Read/Edit/Reports grants and see
  the entire book; ordinary creator-only viewers do not gain book access.

These are configurable workspace foundations, not a complete ERP, certified
accounting/tax/payroll suite or automatic field-to-posting engine. Additive schema
migrations run before serving requests. Signed-update publisher trust, paired
application/database rollback and N1 storage encoding remain unchanged. Publishing
the candidate does not activate it on an existing deployment.

## [1.0.0-rc.8] - 2026-10-25

### Collaboration and module navigation

- Match clipboard item/file-list representations before naming screenshot
  attachments, including browser-generated copies with differing timestamps.
  Preserve distinct selections, document names, ordinary text paste and drops.
- Replace an always-expanded module sidebar with one Modules entry, a bounded
  expandable shortcut list and a searchable, grouped module library. Reuse the
  existing administrator navigation order; paginate library results instead of
  rendering every module into the left rail. Keep existing module permissions.

- Build Office's offline manifest after Vite removes inlined HTML-entry chunks,
  and include both public Office shells. Reject missing final cache/shell assets
  or mismatched worker generations during the production build. No document,
  credential or API response is added to the public asset cache.

No new database schema or N1 storage-format migration. Published RC.7 and earlier
assets remain unchanged. Release publication does not update production services.

## [1.0.0-rc.7] - 2026-10-21

### Upload-limit consistency

- Remove the module record form's hard-coded 50 MiB attachment limit. Display
  and validate the running server's actual policy, including copies from Drive;
  refresh it when selecting files and keep rejection details beside attachments.
- Add Administration → Deployment → Upload limits, with independent record/chat
  limits and explicit Server default/custom/No file-size limit choices. Save administrator overrides
  with revision checks; apply them to new requests without restart and retain
  them across restarts. Preserve unsaved upload-policy controls through live
  reconnect/global refresh. Unrelated identity saves cannot erase saved limits.
- Use the same effective limits for browser metadata, HTTP bodies and streamed
  file reads. Reject oversized/chunked uploads before N1 with actionable JSON
  errors. Keep Drive quotas, import budgets and Office memory budgets separate.
- Allow `0` in either startup upload-size setting to disable that per-file cap.
  Stage module/chat files on private temporary disk and send bounded N1 parts,
  rather than allocating a whole-file server RAM buffer. Report temporary-disk
  failures clearly; existing multipart retries/finalization checks remain intact.

## [1.0.0-rc.6] - 2026-10-20

### Fixed

- Accept clipboard files of all supported attachment types, not images only;
  preserve plain-text paste and document filenames, deduplicate pending files,
  and support file drops into the conversation without browser navigation.
  Explain rejected folders and attachment problems beside the composer.
- Open supported chat Office attachments in the bundled browser engine as
  read-only originals, without requiring a server LibreOffice converter or
  creating a writable Drive item. Retain original-byte download and access
  checks. Shared Drive documents have a direct MX Office action with their
  existing editing permissions/versioning.
- Use the same read-only browser viewer for stored module/record Office
  attachments, with existing record/field access checks and original downloads.
  Preserve the record form and its values; queued files explain that they must
  be saved first. A separate public entry avoids old cached Drive-only shells.
- Close attachment previews when switching conversations and ignore obsolete
  shared-file-list responses. Preserve published RC.5 and existing updater trust.

## [1.0.0-rc.5] - 2026-10-20

### Added and improved

- Add calculated text fields for complete-name relationship labels and automatic
  copies; retain stable record links and skip configurable empty name parts.
- Add optional numeric choice values independent of stored codes/display labels,
  filtered incoming linked-record totals and live balance formulas. Reject
  recursive calculations and incompatible source/dependency changes atomically.
- Add opt-in creator-only records and creator/admin-only fields with server-side
  filtering across reads, writes, counts, summaries, search, exports, history,
  collaboration, attachment preview URLs and live events. Preserve genuine
  creator provenance; never infer ownership from the last editor.
- Route desktop alerts only while MX is unfocused/hidden; deduplicate across tabs,
  recover missed durable notifications, and show permission/delivery diagnostics.
  Keep focused alerts inside MX and document browser/OS delivery limitations.
- Preserve the last desktop-alert result/time across focus and module changes,
  distinguish browser permission from the saved MX setting, and require saving
  notification changes before running a test. Do not replace failures with Ready.
- Serialize module configuration mutations and collaboration read-marker writes
  before their reads to avoid deferred-transaction upgrade races with notifications.

- Replace individual table-cell editors with selectable read-only values and
  full-record viewing/editing; retain normal optimistic save/conflict checks.
- Send a saved record and context directly to a person from its row menu or
  record dialog. Reuse direct conversations and message operation IDs on retry;
  keep recipient permissions unchanged and warn about unsaved form edits.
- Add account-specific bold-field reading preferences for each module, with
  stable field IDs and revision-checked preference saves. Make personal text
  sizing affect rem-based components consistently, up to 200% of the base size.
- Replace automatic first-module editing with a searchable module library;
  separate Fields, Details, Permissions, Rules, Totals and File storage, and
  preserve/caution about unsaved drafts when changing scope.

### Fixed

- Do not highlight every person in Collaboration on a fresh deployment with no
  selected conversation. Highlight only the person whose existing private chat
  is actually open; viewing a profile or filtering the directory does not select
  other people. Add fresh-account regressions in Chromium, Firefox and WebKit.

## [1.0.0-rc.4] - 2026-10-17

### Importer hotfix replacement - 2026-10-18

- Replace RC.4's native/frontend signed package at the maintainer's explicit
  request; retain previous artifacts and publisher trust. Do not create RC.5 or
  weaken equal-version installation protection. Already-installed RC.4 requires
  a verified operator replacement; the local test is replaced after backup.

- Import spreadsheet record values without requiring attachment uploads. Keep
  the module's attachment settings and normal entry-form requirements unchanged;
  imported records honestly start without files. Preserve non-attachment
  validation, permissions, unique values, normal versions and atomic commits.
- Show grouped validation problems and affected-row counts before importing,
  allow inspection of invalid preview rows, and disable Start when no rows are
  valid. Revalidate/remap an existing all-failed job only if it has saved no
  records; never remap a job with successful records or silently repeat imports.
- Restore ignored spreadsheet columns when reopening saved mappings, without
  mutating form state during rendering. Keep completion feedback consistent with
  the running state instead of briefly presenting a finished job with Pause.

### Earlier compatibility refresh - 2026-10-18

- Align current compatibility guidance with N1 `1.0.0-RC.1`, NOA v1 and unchanged
  storage encoding 4. Product numbering does not rename routes or migrate data.
- Refresh RC.4's documentation, initial-install archive and compatibility
  provenance at the maintainer's request before departmental adoption. Preserve
  the original runtime bytes, signed update package/metadata and publisher key;
  that earlier refresh required no runtime reinstall. The later importer hotfix
  above supersedes its runtime assets. Keep equal-version update rejection.
- Add an isolated real-engine compatibility runner for Drive and Office suites;
  never reuse production/test-deployment fragments, credentials or storage roots.

### Fixed

- Initialize application schemas and compatibility backfills before serving
  requests. Do not repeat schema writes or full-record revision repairs during
  normal reads, imports, notifications, calls, Drive cleanup and record edits.
  Scope readiness to the initialized database so isolated databases still migrate.
- Index legacy audit-history reconciliation and match indexed time ranges instead
  of repeatedly scanning the full audit log. Commit imported-record audit entries
  atomically with their records and versions, avoiding later reconstruction work.
- Add a regression with 50,000 records, an independent held write transaction
  and 16 concurrent read-only connections. Existing data, N1 objects, credentials,
  branding and signed-update trust are preserved.
- Qualify an optimized 100,000-record baseline plus a 25,000-row HTTP import
  alongside eight accounts, retained versions/audits and repeated restarts.

## [1.0.0-rc.3] - 2026-10-16

### Fixed

- Make Office background preparation respect scrolling, pointer drags, held
  keys, text composition and renewed interaction after queued local work. Space
  repeated whole-document exports according to their measured cost.
- Keep automatic saves from accepting unfinished spreadsheet cells or resetting
  the native input selection. Explicit Save still includes unfinished input;
  retain a manual save requested during an automatic upload. Keep background
  feedback compact while distinguishing local recovery from confirmed MX saves.

## [1.0.0-rc.2] - 2026-10-16

### Fixed

- Default new record views to Pages, keeping saved choices and making Continuous
  scroll optional. Reuse paged column widths when switching, use type-appropriate
  widths otherwise, retain horizontal position and keep its horizontal control
  visible without a permanent extra footer. Avoid schema reloads, mounting the
  entire continuous cache as a page, and stale work during view changes.
- Do not retain placeholder column widths when switching before the first
  record page has loaded; use readable datatype widths instead.
- Settle virtual spacer/row measurements before recording a restored scroll
  anchor, preventing tall overscan rows from shifting it after navigation.
- Fit the collapsed navigation rail to its actual width at reduced/enlarged UI
  scales. Center the existing branding image, search, navigation icons and footer;
  stop cropping expanded-width rows or reserving hidden group/quick-action space.
- Keep expanded/mobile navigation rows and search within the drawer. Hidden
  search controls no longer remain keyboard-focusable; opening search retains
  input focus. Preserve navigation scrolling and visible keyboard focus.
- Add a production-build cross-browser sidebar regression covering interface
  scales, themes, long module names, many modules and narrow drawers.

### Release

- Start fresh deployments without a generic starter module, business fields or
  records. Add first-module onboarding; retain existing/default/legacy modules
  and their data on upgrades.
- Advance the matching native/frontend candidate to `1.0.0-rc.2`. Retain the
  existing publisher public key and signed-update format for RC.1 installations.
  No database schema, N1 storage, credentials or branding asset changes.

## [1.0.0-rc.1] - 2026-10-16

### Release candidate and version reset

- Start the official 1.0 candidate line at **1.0.0-rc.1**. Align the Rust and
  frontend manifests and lockfiles; preserve all earlier release tags and
  changelog entries as development history. This is not the final stable 1.0.0.
- Document the one-time transition from 4.x development builds. The lower
  semantic version is intentional; it does not reset database migrations,
  records, N1 objects, configuration or the `/mx/v1` API namespace.
- Add a build-time release-metadata consistency check and a side-effect-free
  `litiaina-mx --version` / `-V` command. No dependency versions are changed.
- Include the previously unreleased features and fixes below. Initial adoption
  remains a manual upgrade; the new opt-in updater requires explicit deployment
  setup. See [candidate notes](docs/releases/1.0.0-rc.1.md) for scope,
  acceptance gates and the distinction between historical and current evidence.

### Signed update management

- Add trusted-key release discovery, streamed/verifiable packages, signed offline
  upload and matching binary/frontend release slots. Keep deployment secrets,
  SQLite and N1 outside packages; reject unsafe paths, wrong platforms/channels,
  historical development releases and downgrades.
- Add an independent systemd worker with operator-driven or automatic daily
  maintenance-window installation, pre-restart warnings and active-call deferral.
  Preserve a verified final local SQLite snapshot, gate new requests through
  activation and restore both the previous application and database on failure.
  Persist commit/rollback decisions across worker interruption; never rewind
  acknowledged writes after a successful activation.
- Replace the Updates placeholder with job progress, local errors, reconnecting
  status, release notes, policy controls and inspectable history. Add release
  key/package CLI tools and deployment/service templates; automatic updates remain
  disabled until explicitly provisioned and selected by the operator.

### Backup activity feedback — 2026-10-15

- Show immediate backup-button activity, an honest indeterminate indicator and
  elapsed time. Keep completion, verification and failure feedback beside the
  backup controls, with automatic active-status checks and manual refresh.
- Reconcile lost acknowledgements against verified history without replaying
  backup creation. Allow long backup requests instead of the normal 30-second
  form timeout; stop polling when idle and clean up timers/reads on navigation.

### Bounded record browsing and Office recovery — 2026-10-15

- Add bidirectional, query-bound cursor windows and a bounded, virtualized records
  view with configurable batches, record/offset scroll restoration, stable columns,
  incremental loading/retry and a numbered-page fallback. Keep edits independent
  of rendered rows and surface live updates without unexpected list reordering.
- Remove the continuous-view bottom control bar and use compact controls in the
  existing results toolbar. Let the table use the remaining screen height, show
  full wrapped long text, and avoid stale/off-screen anchors after fast scrolling.
- Overlap Office engine startup with exact-version document download and defer
  offline asset preparation. Reuse unchanged native snapshots, avoid duplicate
  save/checkpoint exports and remove redundant full-file buffer copies.
- Add automatic per-editor recovery copies and idle server saves; persist exact
  retry intent before uploading, recover interrupted operations without duplicate
  versions, and distinguish preparation, local protection, upload and confirmed
  server storage. Surface storage failures before unprotected uploads.
- Keep MX + SQLite + N1 deployment; no mandatory server Office service, new
  server editing sessions or live document co-editing is introduced. Recovery
  checkpoints are not a zero-loss power-failure guarantee.

### Office launch prompt alignment — 2026-10-15

- Center the Office prompt's icon, text and actions as one responsive group.
  Wrapped action rows remain centered on small screens; simplify the message.
  Scope the layout to this prompt without changing Drive's shared control styles.

### Media playback recovery — 2026-10-15

- Preserve N1 streaming 429/503/504/404 responses and `Retry-After` across Drive,
  record attachments and collaboration instead of masking them as generic 502s.
  Do not add an MX streaming rate limiter or server-side retry loop.
- Show video/audio connection failures with bounded automatic recovery and a
  manual retry action. Retain playback position/settings, stop on access/codec
  failures and cancel pending recovery when the preview is closed or unmounted.
  Connection checks request a single byte and cancel the body, not a full video.

### Import results layout and safe field widening — 2026-10-15

- Give skipped-row errors and correction actions separate responsive columns.
  Long values wrap; narrow screens stack the action below the issue. Preserve
  row inspection, pagination and the ordinary correction form.
- Allow Text → Long text on the existing field, without changing its ID/key,
  stored values, record revisions or history. Confirm the change, retain
  uniqueness/required settings, reject unsafe conversions and stale datatype
  changes atomically, and clear a saved sort when its field is no longer sortable.

### Visible field configuration errors — 2026-10-15

- Show Modules & Fields failures immediately in an accessible error popup,
  instead of only at the top of the page. Keep the new-field draft and show its
  error beside the form after dismissal.
- Name the active or archived field reserving a duplicate key, including its
  module and datatype. Offer an available key or a direct jump to the existing
  field. Recheck uniqueness atomically; archived history is never overwritten.
- Explain field keys and distinguish them from duplicate record-value checks.

### Office opening and preview reliability — 2026-10-15

- Open supported Drive Office files directly in the on-device editor. Shared
  links offer an explicit editor/download choice rather than automatically
  starting an expensive server PDF conversion.
- Allow longer document opening/export operations; show large-document guidance
  and Cancel/Retry/Download/Return recovery. Keep the loading overlay until
  native input focus is acknowledged. Cancellation aborts downloads and stops
  editor workers; late responses cannot restart a cancelled opening.
- Prime Calc's initial input after final layout/focus, without resetting pending
  input during normal refocus. Preserve real MX save acknowledgements and version
  conflict checks.
- Inspect only bounded ZIP directory metadata. Reject impossible allocations
  with original-download recovery; editor safety budgets do not change Drive
  upload limits. Keep the pinned Office vendor binary unchanged.
- Bound optional PDF conversions to one worker, share identical requests,
  cool down failures, terminate process groups on deadline and protect temporary
  files. Check file size before downloading bytes for server conversion, and
  enforce expanded-input/output budgets across Drive, records and collaboration.

### Module actions menu recovery — 2026-10-15

- Keep Module actions after Refresh, with matching control sizing and a floating
  import menu that stays above the sidebar and within the visible screen. Keep
  Refresh and actions together on narrow screens; support keyboard navigation,
  click-away dismissal and reliable focus on repeated browser openings.

### Module spreadsheet importer — 2026-10-14

- Add Module actions (⋯) → Import records / Import history for XLSX, ODS, UTF-8
  CSV and TSV. Configure per-sheet mappings, header/data rows, date formats and
  date fill-down; download stable field-key tagged templates.
- Use normal validation, form rules, relationship access and version history.
  Skip invalid rows atomically while continuing valid rows.
- Show committed progress, paginated source/errors/results, downloadable failure
  logs and correction through the ordinary record form.
- Persist jobs and commit success markers with the record/version transaction;
  pause, restart and request replay do not duplicate successfully imported rows.
- Bound parser resources; avoid dense formatting grids and isolate unusable XLSX
  sheets. Source stays in MX metadata, not N1. Removing logs preserves records.
- Add an operator walkthrough. No live MX/N1 reset, version bump or release
  publication is part of this addition.

### Drive styling recovery and scoped Deployment layout — 2026-10-14

- Withdraw the broad UI polish and restore the original component style
  precedence. Moving component CSS into a named layer let shared styles override
  Drive's row grid and other component layouts in the production bundle.
- Remove the application-wide polish overrides; preserve the preceding feature
  changes. No record, attachment, permission or N1 data changes are needed.
- Fix the Branding-to-Terminology gap using independent Deployment columns, with
  control sizing scoped to Deployment rather than changing Drive or other screens.
- Replace the inadequate control-only polish checks with rendered Drive grid,
  column-alignment and Deployment-gap regression tests; retain file-manager tests.

### Documentation organization and operator instructions — 2026-10-13

- Replace the 2,578-line README with a concise project overview, build instructions
  and documentation entry points. Preserve detailed reference material under `docs/`.
- Add an indexed documentation collection, deployment/upgrade guide and first-module
  operator walkthrough using the current UI labels and explicit verification steps.
- Explain Relationship versus Lookup versus editable auto-fill with complete
  employee/issuer/classification setup recipes. Explain single/multiple linked
  record selection and why another Relationship is not an auto-fill destination.
- Add conditional-form instructions, troubleshooting tables, access requirements
  and cautions about changing existing schemas, uncertain saves and release limits.
- This documentation pass does not convert existing fields, wipe data, change
  runtime configuration, or fix the remaining field-settings save-error placement.

### Modular conditional forms — 2026-10-13

- Add a focused **Form rules** configuration dialog per module field: nested
  All/Any conditions for visibility, editability and requirements; dependent
  select options; dropdown/radio/exclusive-checkbox presentation; exclusive
  Boolean groups and option-to-field automatic values with stable stored choices.
- Apply immediate local selection behavior, retain editable copies and require
  confirmation before clearing old branch details or queued files. Keep stored
  attachments and normal version history. Preserve the selected radio button
  when cancelling a switch, and warn on save-time numeric/text branch resets.
- Show inactive table cells as Not applicable; inline changes needing required
  detail fields open the record form with the proposed selection already applied.
- Enforce typed conditions and automatic values on the server, serialize schema
  graph checks, reject named cycles/missing controls/competing mappings, and add
  named form-rule archive blockers with navigation to the rules dialog.
- Expand/reset ordinary fields in the existing atomic record save. Preserve
  independent-field merging; guard changed controllers and copied/cleared fields.
  Keeping latest branch details also restores the current controls while retaining
  unrelated local edits. Recheck attachment admission after transfer and reject
  version restores that violate current rules with an actionable conflict response.
- No editing sessions, collaboration locks, cell-version tables, new storage
  services or attendance-specific hardcoding. Attachments/calculated fields support
  visibility only; rules do not introduce field-level read permissions or atomic
  conditional file-byte requirements. Existing rows are not migrated by rule saves.

### Explain field archive dependencies in a dialog — 2026-10-13

- Replace the off-screen archive error with a focused dialog showing the field
  and module, every named dependency, why it blocks archiving and how to fix it.
  Link directly to the dependent field's settings, storage layout or module records.
- Check relationships, lookups, rollups, auto-fill mappings, formulas, stored
  active/trashed links, owned attachments and storage folder/prefix usage. Keep
  Archive disabled until clear; preserve historical values and files.
- Add an administrator-only read-only archive-check endpoint and structured
  blockers in rejected archive responses. Check again in the archive write
  transaction, so a stale preflight cannot bypass newly committed dependencies.
- Keep check/save failures and retry within the dialog, trap keyboard focus,
  restore focus on dismissal and highlight the exact destination settings.
  Provide balanced mobile margins and light/dark styling. Warn before cross-module
  navigation discards unsaved field settings as well as module/storage edits.

### Configurable relationship auto-fill — 2026-10-13

- Add **Fill fields automatically** mappings to single-record relationships.
  Select an employee name once to populate existing compatible fields such as
  Classification and Department immediately in forms and inline table editors.
- Keep copied values editable and historical; save them through normal record
  revisions and version history. Refresh dependent read-only lookups/rollups
  immediately without creating editing sessions, locks or separate cell versions.
- Provide an access-checked read-only selection preview. Cancel obsolete
  requests, preserve destination typing during loading, block partial saves on
  errors and offer retry. Reset/closing cannot apply an obsolete response.
- Fill omitted destinations on create/PATCH atomically, preserve explicit
  overrides and include copied fields in optimistic conflict detection. Allow
  separate decisions for every conflicting inline field. Validate compatible
  types, single selection, unique destinations and mapped-field dependencies.

### Configurable record-column summaries — 2026-10-13

- Add optional sum, average, minimum, maximum and non-empty numeric-value count
  below integer, decimal and formula columns. Auto-number columns support count.
  Configure calculations and display precision in Administration → Modules &
  fields → Column summaries; disabled by default for existing modules.
- Calculate summaries over every matching active record, not just the current
  page. Reuse record search/filter/module-access checks and exclude trash. Read
  rows, count and summaries in one SQLite snapshot; use read-only revision/file
  helpers so concurrent readers do not attempt migration writes inside it.
- Aggregate stored numeric fields in SQL. Recalculate formula summaries from
  current definitions in bounded 128-record batches; do not trust stale stored
  formula results or transfer all matching records to the browser.
- Refresh totals after edits and live deletion/restoration, invalidate obsolete
  totals on refresh/deletion, and retain aligned desktop and readable mobile
  footers. Distinguish empty averages/min/max from genuine zero sums/counts.

### Office loading feedback — 2026-10-13

- Replace the single Office loading message with document preparation, download,
  bundled-resource loading, device initialization and document-rendering stages.
  Show the filename, measured received bytes, per-download percentages when a
  valid total is known, elapsed time and a completed/current/waiting checklist.
- Use actual streamed document bytes and bundled-engine progress callbacks, not
  simulated overall percentages or estimated startup times. Stop animation on
  failures and keep the stopped state after dismissing an error message.
- Keep feedback within the loading area; remove it once the document opens.
  Preserve the compact 38-pixel toolbar and full editing area, light/dark and
  narrow-screen layouts, offline operation and MX-owned file versioning.
- Leave module relationships and lookup behavior unchanged.

## [4.1.1] - 2026-10-11

### Patch release

- Align Rust/frontend manifests and lockfiles on 4.1.1 for the Drive sharing
  delivery and upload recovery fixes below. N1 compatibility remains v4.0.0.
- This release does not clear the remaining Office, native WebKit upload,
  live-storage or call-media qualification gates. See `TESTING.md` for measured
  results and limits; a version bump is not production certification.

### Drive sharing delivery — 2026-10-11

- Post permission-aware Drive cards to private chats and spaces/groups when
  owners share files or folders. Commit grants, messages, activity and recipient
  notifications atomically; avoid duplicate delivery on unchanged-access retries.
- Confirm sharing in the dialog with a conversation shortcut. Reuse the existing
  notification center, toasts and configured alerts; respect file preferences,
  muted conversations and group mentions-only settings.
- Add a searchable, 50-item paginated conversation Drive list, including older
  grants. Open cards directly in Drive; resolve current permissions/names and
  show unavailable cards after access is removed or an item is trashed.
- Prevent shared videos/files from reopening when switching modules and
  returning to Drive. Synchronize route targets before mounting a destination,
  clear dismissed preview URL intents and cancel superseded metadata requests.
  Keep explicit shared-file clicks and intentional browser history navigation working.
- Keep sharing metadata-only: no duplicate N1 object or file-byte transfer.

### Drive upload recovery and throughput — 2026-10-11

- Automatically reconnect/reconcile transient Drive upload failures with bounded
  backoff, immediate offline/online handling and explicit retry/waiting states.
  Respect manual pauses and terminal authentication/permission/quota/conflict errors.
- Persist account-scoped, per-transfer metadata without storing file bytes or
  credentials. Restore unfinished uploads after reload with an original-file
  picker, preserve stable operation IDs and manual pause, and keep other tabs'
  unrelated recovery entries. Offer explicit cancellation of unfinished uploads,
  acknowledging cancellation only after its metadata transaction commits.
- Keep acknowledged progress separate from in-flight bytes. Hide stale speed/ETA
  during retries and show Confirming until the MX commit is acknowledged.
- Raise default inactive upload retention from one hour to 72 hours; configure
  `[drive] upload_retention_hours` between 1 and 168. Quota reservation, session
  matching and cleanup use the same window, including retryable failures. Valid
  part activity renews the window so slow active uploads are not expired mid-transfer.
- Grow files above 1 GiB from console's four workers toward eight after successful
  parts, sharing an eight-part browser budget. Preserve console chunk sizes and
  the existing eight-transfer server cap; apply bounded admission backpressure
  before body buffering instead of immediately rejecting brief contention.
- Distinguish interrupted request bodies (retryable) from oversized parts and
  unreadable source-file handles. Preserve stored parts and request the original
  file when browser-held source bytes are no longer readable; never accept empty
  or truncated chunks as successful.
- Validate recovery in Chromium/Firefox and eight concurrent HTTP clients against
  real isolated MX/SQLite and the N1 contract fixture. WebKit/WPE native source-file
  loss remains unresolved; only its safe-failure guard passes. Scheduling gains
  are measured with virtual-file latency tests, not live-network MB/s. See
  `TESTING.md`; this is not full production/browser qualification.

## [4.1.0] - 2026-10-10

### Release and UI notes

- Align Rust/frontend manifests and lockfiles on 4.1.0 for the standalone
  Office and module-relationship additions. N1 compatibility remains v4.0.0;
  no external Office server or deployment dependency is introduced.
- Restore visible dropdown arrows across MX. Component background overrides
  previously erased the custom arrow while native select styling was disabled,
  making Drive permissions, guest-link expiry, sorting and page-size controls
  look like plain text boxes. Preserve native HTML select popups and keyboard/touch
  controls. Use one non-repeating chevron inset 12px in light/dark themes, with
  reserved text space and native arrows restored in high-contrast mode.
- Keep Share open when Escape dismisses a native dropdown (including Firefox);
  Escape from other Share controls and the Close button still dismiss the dialog.
- Remove the remaining dark/system-theme SVG arrow overrides and obsolete
  custom-arrow spacing. Those overrides tiled behind native arrows outside
  Drive, including Reporting period and Navigation icon. Expand regression
  coverage to dashboard, module settings, report builder and account controls
  in explicit light/dark and system-light/system-dark themes, plus shared-style
  tests to prevent duplicated or tiled arrow overlays from returning.
- Give the module editor's Navigation icon control more desktop grid space so
  its selected text and inset arrow fit comfortably; preserve the mobile layout.
- This version designation does not clear the Office native-input/rendering,
  browser/device, live-storage or call qualification gates below. Consult
  `TESTING.md` before deciding whether to deploy.

### Added

- Keep horizontal scrolling within reach for wide/tall record tables with a
  slim synchronized scrollbar at the visible bottom edge. Show it only while
  the native bottom scrollbar is off-screen; hide it for fitting columns, mobile
  cards and open editors. Support thumb dragging and keyboard navigation without
  changing the table's styling, records or pagination.
- Relationship fields between modules, with single/multiple stable record-ID
  links, searchable 50-item selectors, inline/form editing and paged inbound
  related-record navigation. Lookup and count/sum/average/min/max rollup fields
  calculate from the selected linked records on read.
- Persistent indexed relationships and referential-integrity checks: reject
  cross-module/missing/trashed targets, prevent deletion of actively referenced
  records and modules, and guard dependent field archival. Relationship values
  use normal record revisions/history and optimistic conflicts, not edit locks.
- Check both source and target module access and redact inaccessible linked IDs,
  labels and calculated values, including history/conflict responses. These
  fields cannot be raw search/sort/storage-path inputs or formula dependencies.
- Bundled client-side Office editing from Drive, without an external Office
  server or runtime CDN. An ACL-checked snapshot pins the current MX version;
  Word/spreadsheet exports save through existing multipart uploads
  as new immutable N1 objects and normal, revision-checked MX versions.
- Editor-only public-asset offline caching and explicit per-document local
  retention. Prepared documents can reopen/edit with MX unreachable; reconnect
  and explicitly save after access/quota/revision checks. Local drafts survive
  tab closure, and uncertain saves retain their exact retry operation/payload.
- Route native Save/Ctrl+S through MX. Whole-document conflicts offer deliberate
  next-version/separate-file/latest-version choices; there is no automatic Office
  co-editing merge. Disable macros and automatic linked-data updates, and keep
  the editor's isolation/security headers separate from calls and the workspace.
- Queue Save/Download behind local checkpoints rather than losing a click;
  retain separate-file save operations across uncertain responses and offline
  reopening. Ctrl/Cmd+Shift+S opens the MX separate-file save flow.
- Check real worker WebGL support before engine startup and retain the original
  document download on unsupported graphics runtimes. Generate gzip alternatives
  for browsers/connections that do not advertise Brotli.
- Keep narrow-screen Office controls at their initialized width, with scroll
  buttons instead of distorting the native canvas; show a real failure status
  when startup fails, retain the loaded download, and observe later runtime errors.
- Show explicit acknowledged MX save feedback with revision/time, separate from
  offline drafts and preparation errors. Accept current Calc cell input before
  exporting; preserve native shortcut key state and resize native pointer targets
  when the viewport changes. Establish native focus after layout and retain the
  actual local spreadsheet selection for accessibility.
- Replace the stacked Office headers/status/offline rows and MX footer with one
  fixed 38-pixel bar. Keep filename, compact save feedback, Save and Document
  options; move Drive/download/copy/offline/details/scroll controls into that
  menu. Errors, recovery prompts and the options panel overlay the native editor
  without changing its canvas dimensions. Preserve the native editing controls.
- Show Office sign-in only for missing/expired MX authentication; preserve edits
  during recovery and reject switching the document to a different account.
  Guard the module-specific native File/Open commands and route users to Drive
  instead of the unsupported native file picker. Keep each tab bound to its file.
- Replace the raw offline-preparation SSL error banner with compact expandable
  guidance, technical details and retry. This does not bypass certificate checks
  or change deployment certificates/system trust.
- Change the served offline worker script for each bundled asset generation,
  allowing browsers with an existing offline cache to install editor updates
  instead of retaining the old shell indefinitely.

### Validation and deployment notes

- Dropdown regression passes in Chromium, Firefox and WebKit with real
  isolated MX/SQLite: keyboard role/expiry selection, persisted ACLs and expiry,
  all sorting choices, bounded pages, Escape behaviour and light/dark/mobile
  screenshots, shared 12px arrow insets and forced-colors fallback. Packaging/build
  and unit tests were rerun on 4.1.0; storage is
  the explicit contract fixture, not live N1. See `TESTING.md` for evidence/limits.
- Added an isolated eight-account relationship/API and three-engine UI runner.
  Relationship reads use already-initialized schema/permission readers within
  a database snapshot, avoiding migration writes and WAL read-to-write lock
  failures during concurrent record creation. See `TESTING.md` for exact results.
- Rollups operate
  on explicitly selected outgoing links, not automatic inverse SQL aggregates.
- Preserve standalone MX + SQLite + N1 deployment. The external Collabora/WOPI
  direction was replaced with checksum-pinned local WebAssembly assets. Office
  offline caching requires browser-trusted HTTPS and sufficient client storage;
  retained documents are private browser-profile copies, not server backups.
  The older pinned engine still needs a current security/support and complete
  distribution-notice audit before production qualification. Real device,
  low-memory and complex-document fidelity remain separate release gates.
- PPT/PPTX/ODP are explicitly read-only: presentation rendering succeeds, but
  native edit-input tests failed. Do not treat format/filter availability as a
  passing editing test. The tested WebKit/WPE build lacks worker WebGL; graceful
  fallback is separate from Chromium/Firefox Office editing qualification.
- Expanded real-browser save-UX checks validate delayed acknowledgement,
  current-cell saves and newer input during an upload. Repeated full runs still
  catch intermittent leading-character loss in fresh spreadsheets and a Firefox
  native-canvas rendering failure. These are explicit Office release blockers,
  not hidden behind green frontend unit tests or successful server commits.
- Patch the build-only `source-map-js` lockfile entry to 1.2.2 for
  [GHSA-68fv-2mgg-jv7q](https://github.com/advisories/GHSA-68fv-2mgg-jv7q).
  Full npm audit reports zero advisories; the Office binary audit remains separate.

## [4.0.0] - 2026-10-09

### Product identity and upgrade notes

- Position MX as a **self-hosted modular digital workplace platform**, unifying
  configurable records/reporting, collaboration, calls, screen sharing, and
  personal/shared file storage. Information management remains its core rather
  than its entire product category.
- Align Rust/frontend manifests, lockfiles, default branding, production frontend,
  and documentation on MX 4.0.0. Existing saved deployment branding is preserved;
  Restore MX defaults offers the new subtitle without publishing it until Save.
- Target N1 v4.0.0 / storage format 4: MX owns file names, folders, ownership,
  sharing, versions and trash metadata; N1 owns immutable file content and
  retention-based physical cleanup. Deploy the matching frontend/backend together
  and back up SQLite before automatic schema migration.
- Full-record replacement requires `base_revision` (428 if absent, 409 if stale);
  field-based PATCH retains stateless optimistic merges. Retire the legacy Drive
  single-request upload route (410); use durable multipart initialization, part
  PUTs, and finish. Drive listing clients should use returned page limits/offsets,
  not assume a fixed 100-item page.

### Drive file-manager improvements

- Add bounded server-side pages (25/50/100 items), counts, page numbers/jump,
  name/date sorting, and empty-last-page reconciliation. Apply pagination to
  folder pickers and public guest folders too; tighten the unused file-list gap.
- Navigate previews across page boundaries with adjacent-metadata seek queries,
  Left/Right shortcuts, and bounded 256-KiB text range previews rather than reading
  a complete text file. Preserve native image/video range streaming.
- Share with collaboration spaces/groups as well as accounts and guest links.
  Folder inheritance checks live membership; leaving/removal/archival revokes that
  source of access. Show direct and inherited grants separately; refreshing a
  removed/revoked open folder clears stale rows and returns to its view's root.
- Fix the Share dialog's one-character names/emails with constrained role controls,
  responsive identity rows, a fixed header and a scrollable content body.
- Align file-list columns with their headers and replace the crowded action row
  with quick actions and a viewport-clamped More/right-click menu, scoped to Drive.
- Add checkbox/Ctrl/Cmd/Shift/rubber-band multi-selection, select-all, copy/cut/paste,
  F2 rename, and confirmed Delete-to-trash without intercepting text inputs.
- Add external clipboard image/file uploads and file/folder drops, preserving
  nested and empty folders where browser directory APIs are available.
- Make Drive-to-Drive copies and version restoration metadata-only: independent
  ownership/history, immutable N1 content references, quota accounting, payload-bound
  replay receipts, and reference-safe cleanup. Multi-item moves commit atomically.
- Replace linear upload progress with a ring around the file icon; retain actual
  transferred bytes, smoothed MB/s, ETA, pause/resume, and module-independent uploads.
- Separate Versions and Activity into clear tabs with explicit preview/restore
  controls, current-version cards, plain-language events and timestamps.
- Keep local Copy/Cut responsive during unrelated metadata writes, and accept
  external clipboard uploads while those writes finish. Retain active-document
  authentication if browser storage disappears during process recovery, without
  weakening explicit sign-out, refresh rejection or server authorization.

### Drive upload and storage allowance fixes

- Removed Drive's 100-MiB per-file cap; account storage quotas remain separate.
- Matched console's default multipart part sizes and concurrency, including
  50-MiB parts above 1 GiB. Existing sessions retain their negotiated size.
- Follow console's metadata-first upload flow: remove whole-file pre-hashing
  and the complete N1 read-back after finalization. Check quota before file bytes;
  confirm exact committed N1 version/size through metadata. Retain per-part
  replay checks without recording an uncomputed whole-file checksum.
- Move transfers out of the Drive page into an account-scoped workspace queue;
  switching modules no longer cancels uploads. Add a collapsible lower-right
  upload panel with real in-flight byte progress, smoothed MB/s, estimated time
  remaining, pause/resume, and individual errors in neutral MX light/dark colors.
- Recover pending sessions on file reselection using metadata instead of browser
  localStorage, retry immutable binary parts with bounded backoff, and warn before
  closing/reloading during uploads. Signing out clears the account's queue.
- Correct the legacy raw-fragment/secret N1 authentication configuration in
  memory without modifying N1 or exposing credentials.
- Added administrator-assigned per-user quotas (10-GB starting default), audited
  revision checks, commit-time enforcement, a visible storage meter, and a
  neutral light/dark Drive sidebar and content layout.

### Added

- MX Drive: personal folders/files, shared/recent/starred/trash views, search,
  list/grid layouts, thumbnails, navigable previews, versions, and activity.
- MX-owned metadata and immutable unique N1 keys preserving each version's
  original file extension/casing. Rename/move never mutate N1 objects.
- Private-by-default owner/viewer/editor grants, inherited folder access,
  revocable/expiring read-only public guest links, and ACL-checked media ranges.
- Durable resumable N1 multipart uploads, stored-part reconciliation after MX
  restart, bounded file/folder queues, and N1 batches for files up to 1 MiB.
  Lost batch/finalize acknowledgements reconcile existing objects, not new copies.
- Independent Drive-file copies into collaboration messages and record
  attachments. MX-only trash and atomic durable background N1 soft-delete jobs;
  N1 TTL/GC retains responsibility for physical deletion.
- Optional `[drive]` default-quota/public-link configuration, with neutral
  light/dark UI, quota reservations, revision conflicts, and permission auditing.

### Fixed

- Deleted records disappear immediately on live deletion and stay hidden through
  delayed list responses and module updates. Explicit restores and reconnect
  reconciliation lift that barrier; old lifecycle events cannot undo a newer
  restore, including after server sequence counters restart.
- Trash and restore now atomically advance the record revision and invalidate
  pre-delete drafts for every field, including fields that were empty. Existing
  version history still records the lifecycle operations.
- Full-record `PUT` now requires `base_revision` (HTTP 428 when missing) and
  atomically rejects stale replacements (HTTP 409). This closes an overwrite
  path that could bring back cleared cells. Collaborative partial changes still
  use `PATCH`, allowing unrelated edits to merge.
- Peers discovered while microphone/camera permission is pending now acquire
  the granted tracks and renegotiate instead of remaining receive-only.
- Added a disposable real-server eight-account concurrency/upload/call runner,
  plus browser regressions for deleted-row refresh races and missed restores.
  Upload runners support both an explicit N1 v4 HTTP contract fixture and
  opt-in disposable live N1 storage with fault injection and test-only cleanup.
- Browser test runners now refuse physical/default audio outputs and require
  an explicitly verified test-only null sink, preventing synthetic call tones
  from being sent to the developer workstation's speakers.
- Preserved signed-in sessions through temporary network/server failures and
  added a reconnect screen instead of treating an outage as expired credentials.
- Added bounded API deadlines, safe-read retries, upload progress deadlines,
  jittered live reconnects, and a watchdog for silent/half-open connections.
  Reconnection refreshes workspace state from REST without remounting the record
  table or discarding typed entries.
- Added response-body stall recovery for buffered downloads, document previews,
  profile photos, and custom sounds without imposing a total playback deadline
  on native streaming video.
- Added durable, account-scoped operation identifiers for record/message
  creation. A retried create returns its existing resource instead of allocating
  another record, message, sequence, or history entry. Pending record creates
  retain their identifier with the tab's draft, including after reload.
- Kept inline cell saves tied to the revision captured when editing began,
  even if a background refresh delivers a newer revision while typing.
- Moved record action menus outside table overflow clipping, with viewport-aware
  placement, mobile/zoom resizing, keyboard focus, and Escape/outside dismissal.
- Preserved separate chat drafts and original message operations across send
  retries; attachment validation errors no longer cause duplicate base messages.
  Invalid upload acknowledgements retain an explicit uncertain outcome.
- Added session-scoped offer replay, duplicate-answer handling, candidate replay,
  and staggered ICE recovery for interrupted calls. Call heartbeats no longer
  overlap, and delayed requests cannot restart a call after the user leaves it.
  Departed peer sessions are retired before rejoin so delayed signaling or
  stale participant discovery cannot replace a fresh connection.
- N1 transport now bounds connection/read/authentication and mutation waits.
  Standard multipart parts retry their exact session/index/body on transient
  failures; uncertain finalization checks the exact committed version before
  reporting success. Streaming downloads use a read-idle timeout rather than a
  total playback deadline.
- N1 durable multipart accepts successful plain-text part acknowledgements
  instead of incorrectly reporting a JSON parsing failure. Drive quota commits
  recheck usage after paused reservations expire; canceled in-flight initialization
  queues its eventual N1 session for cleanup. Private previews pin their version,
  and guest media refuses stale revisions to prevent mixed-version range playback.
- Record save/close/reset and message sending wait for in-progress Drive copies,
  preventing an early save from silently dropping the selected attachment.
- Unhealthy connected call transports now attempt ICE recovery, then a fresh
  revisioned peer transport when needed. Stale transport descriptions/candidates
  cannot replace the recovered pair. Microphone and shared audio use independent
  stable playback elements. See TESTING.md for remaining eight-user call gaps.
- Added Chromium, Firefox, and WebKit browser regressions for outage recovery,
  retained drafts, chat/attachment retries, clipped record menus, inline-edit revisions, and native
  WebRTC signaling loss/rejoin. Mixed-engine group scenarios cover delayed,
  reordered signaling and camera/screen transitions with synthetic capture.
- Updated MX's N1 integration for v4.0.0 / storage format 4. All upload paths
  write logical object keys directly, without the rejected empty-directory
  creation calls, including records, collaboration, photos, sounds, logos,
  and backups.
- N1 object-key URL encoding preserves reserved characters. Rename, trash,
  and recovery inspect per-key conflict/failure reports rather than assuming
  HTTP 200 means every operation completed. Multipart finalization transport
  failures also trigger best-effort session cancellation.
- Fixed avoidable black flashes in camera and screen-share tiles caused by
  resetting video playback during participant heartbeats and microphone updates.
  Source transitions use one playing video and a bounded still-frame overlay
  until the replacement frame is available, avoiding overlapping video layers
  and fullscreen opacity compositing.
- Unchanged call heartbeat responses preserve participant object identity,
  avoiding unnecessary media and audio-meter effect restarts.
- The device-panel video preview no longer rebinds its source when only the
  microphone or stream wrapper changes.
- Fixed group screen-share tiles remaining on an avatar when sharing state
  arrives before the video track. Track additions, removals, and replacements
  now notify the view immediately while unchanged tracks retain their decoder.
- Group media updates run independently for each viewer. New video tracks
  trigger negotiation immediately, and changes queued during an outstanding
  offer resume after its answer rather than waiting for a participant refresh.
- Static screen shares become visible when their first frame is decoded,
  without waiting for another captured frame. Participant refreshes also
  discover missed joins without repeatedly renegotiating healthy connections.
- Self screen-share preview is hidden by default with explicit show/hide
  controls, and is paused in native fullscreen to prevent recursive capture.
  Starting a share no longer automatically enlarges the sharer's own tile, and
  the device panel avoids rendering a second copy of the shared screen.
- The capture picker requests exclusion of the current MX tab where supported.
  Remote viewers retain their fullscreen video controls.
- Remote audio playback binds only audio tracks and keeps the same stream when
  video changes, preventing microphone interruptions and unnecessary video work
  in audio elements.

### Validation and known limitations

- 105 Rust tests and 164 frontend tests passed; frontend checks/build passed.
  Drive polish and file-manager production UI passed in Chromium, Firefox and
  WebKit. Pagination tests include a 10,123-file-per-owner namespace and 135-item
  browser folders, group membership revocation, and bounded cross-page previews.
- These results are not blanket production certification. The latest large-file
  combined stress run still hit a Linux WebKit network-process crash; full
  eight-person call validation remains incomplete. Current live N1 revalidation
  was blocked by a missing test fragment; new polish checks used an explicit N1
  contract fixture. See [TESTING.md](TESTING.md) for retained evidence and limits.

## [3.0.0] - 2026-10-05

### Added

- Added server-evaluated `Formula / Calculated` fields with field-key
  references, arithmetic operators, constants, configurable decimal places,
  and common aggregate, rounding, logarithmic, trigonometric, and utility
  functions.
- Added inline record-table editing for text, long text, integer, decimal,
  date, boolean, and select fields, with the existing field-revision conflict
  protection retained.
- Added deliberate read-only and edit states to the record dialog so opening a
  record no longer immediately exposes destructive controls.
- Added session-scoped record drafts for typed field values. Queued local files
  must still be saved or removed before closing because browsers cannot safely
  reconstruct a selected local file.
- Added image thumbnails, icon-based attachment actions, a persistent attachment
  navigator, previous/next buttons, and Left/Right keyboard navigation to the
  record file viewer.
- Added short-lived preview tickets for record and collaboration media so native
  browser image, audio, and video requests do not need a session token in the
  resource URL.
- Added configurable multipart N1 upload planning through
  `n1.multipart_part_size_mb` with a supported range of 5-100 MiB and a 16 MiB
  default.
- Added visible collaboration system messages when a member is added, removed,
  or leaves a space.
- Added membership-scoped voice and video calls with microphone, camera, and
  screen-share controls, a persistent call dock, joinable active-call status,
  private WebRTC signaling, reconnect recovery, and automatic stale-member
  cleanup.
- Added incoming direct-call and group-call notifications, active-call badges,
  refresh-safe ongoing-call discovery, deafen and device controls, focused
  screen-share layout, and participant-level native full-screen viewing.
- Incoming private calls now use a bounded repeating ringtone and persistent
  operating-system alert, while group calls use one softer chime and a
  non-blocking availability alert. Answer, decline, remote join, expiry, and
  call end all stop and clean up the alert lifecycle.
- Added Discord-style speaking rings backed by live microphone levels, plus a
  structured device-check panel with outgoing preview, microphone meter,
  input/camera/output selection, speaker testing, and device refresh status.
- Screen sharing now requests supported tab/system audio and transports it on
  an independent sender, allowing shared audio and the microphone to operate
  and stop independently.
- Call joins now carry a unique browser-session identity. Delayed leave events,
  old SDP/ICE messages, and retained live-event history can no longer remove or
  attach to a user after they quit and rejoin; peers are rebuilt with the active
  microphone, camera, or screen track for the new session.
- Added an exact `font_size_px` preference from 10-24 px while retaining legacy
  percentage preference migration.
- Added focused interaction tests for message-composer keyboard behavior and
  record notification navigation.
- Added default-deny module isolation for non-administrator accounts. System
  roles now act as capability ceilings over explicit per-account module grants
  for view, create, edit, archive, configure, reporting, and attachments.
- Added optimistic revision keys to account module-access changes, so competing
  Administrator saves produce one accepted update and a reviewable `409`
  conflict instead of silently overwriting each other.
- Replaced record cell edit leases with stateless optimistic concurrency.
  Editors now open and type without network claims or heartbeats, submit only
  changed fields with the loaded record revision, automatically merge unrelated
  updates, and resolve same-field conflicts from stored/proposed values.
- Attachment additions and deletions now advance the same record revision;
  independent additions merge while stale or duplicate same-attachment actions
  return an explicit conflict.

### Changed

- Record attachments, collaboration shares, profile photos, deployment logos,
  and database backups now use one N1 uploader; any accepted object larger than
  the configured part size is transferred through N1 multipart upload.
- Record and collaboration previews now stream N1 response bodies and forward
  byte-range metadata, allowing supported video and audio to start before the
  complete object is downloaded.
- Chat now follows the standard Enter-to-send and Shift+Enter-for-new-line
  interaction. IME composition and the mention chooser retain their own Enter
  handling.
- Hardened camera-to-screen-to-camera switching by retaining a stable WebRTC
  video sender, recovering unusable senders through renegotiation, rebinding
  receiver media on source changes, and keeping local media labels authoritative.
- Remote microphone playback now uses a dedicated audio path, so rebinding a
  camera or screen-share view cannot interrupt voice audio. Unmuting a remote
  track also retries playback without rebuilding the peer connection.
- Record tables now expose a persistent **Edit** action on every writable row;
  users no longer need to open view mode or find an edit command in an overflow
  menu. Destructive actions remain separated behind the row menu.
- The call panel can now be dragged and safely clamped inside the viewport when
  it is not expanded.
- Notification clicks now open the requested record even when it is already in
  the active module; attachment notifications can focus the attachment area.
- Record activity notifications created by an edit and its attachment operation
  are grouped within a five-minute actor/record window.
- Live record refreshes keep the existing table rendered until replacement data
  arrives, preventing the page from jumping back to the top during active work.
- The record viewer now opens files through the same secure preview system used
  by collaboration and keeps Office conversion previews as generated PDF data.
- Record tables and lifecycle/reporting paths now understand calculated fields.
- Recent lifecycle rows missing a matching request audit entry are repaired
  immediately, improving actor attribution in User performance reports.
- Module, record, preview, notification, and compact/mobile layouts were tightened
  to avoid clipped controls, oversized cards, and narrow-screen overflow.
- Call panels and in-conversation call banners now use content-driven flex rows,
  preventing status and control areas from stretching into empty viewport space.
- Module navigation, schema, records, search, dashboards, attachments,
  notifications, and collaboration-linked records now use the same effective
  server-side access decision. Revocation also invalidates previously issued
  record preview tickets.

### Fixed

- Fixed minimizing an expanded call leaving both layout states active and
  stretching the compact call dock to nearly the full viewport height.
- Fixed quit/rejoin being blocked by slow leave cleanup. The dock now closes
  immediately while the session-scoped leave completes safely in the background.
- Fixed late or rolled-back ICE candidates surfacing as fatal call-negotiation
  errors after rejoining; obsolete candidates are ignored and failed connections
  use the existing ICE-restart path.
- Fixed simultaneous initial WebRTC offers reaching `have-remote-offer` before
  `setLocalDescription`. Each peer pair now has one deterministic initial
  offerer and uses the browser's atomic perfect-negotiation operation.
- Fixed incoming direct calls showing the database label "Direct message" in
  the call dock instead of the other participant's name.
- Fixed the notification popover becoming a thin, unusable column beside wide
  record tables.
- Fixed notification navigation being ignored when the selected record route did
  not otherwise change.
- Fixed duplicate record-update and attachment notifications for one user action.
- Fixed large media previews waiting for the entire N1 object instead of using
  HTTP partial content.
- Fixed missing thumbnails and broken full-size record image previews caused by
  authenticated image resources being used directly by browser media elements.
- Fixed long-text cells being unavailable to spreadsheet-style editing.
- Fixed audit performance summaries temporarily omitting newly created or edited
  records when request-level audit capture was absent.
- Fixed the call device panel being clipped or allowing clicks to pass through
  on short windows and mobile layouts.
- Fixed concurrent non-administrator module refreshes intermittently returning
  HTTP 500 while account permissions were being evaluated.
- Fixed fresh installations attempting to write request audit entries before
  the record-lifecycle schema had been initialized.

### Upgrade notes

- Deploy the updated Rust binary and the committed `frontend/dist` bundle
  together, then restart MX. The new preview routes and automatic SQLite schema
  upgrades are registered at process startup.
- Existing `mx.config` files may omit `multipart_part_size_mb`; MX uses 16 MiB.
  If it is configured explicitly, N1's maximum allowed part size must be at least
  the same value.
- Existing `mx.config` files may omit `[webrtc]`. HTTPS is required outside
  localhost, and production calls across NAT or restrictive firewalls should
  configure deployment-owned STUN/TURN URLs and credentials. Calls use a bounded
  peer mesh with a configurable 2-12 participant limit.
- No manual database migration is required. MX applies the additive collaboration
  schema changes and preference compatibility handling automatically.
- Existing non-administrator accounts receive their compatible active-module
  grants once during the startup migration. Accounts created after that marker
  are intentionally default-denied until an Administrator assigns access.

## [2.1.0] - 2026-09-29

- Standardized dashboard charts and collaboration surfaces on MX light/dark
  tokens independently of deployment accent customization.
- Reworked dashboard chart presentation and module configuration workflows.
- Increased the configurable collaboration upload limit and improved responsive
  administration layouts.

## [2.0.0]

- Introduced independent modules, collaboration, durable notifications, record
  history and Trash, deployment identity, and the second-generation Svelte
  workspace.
