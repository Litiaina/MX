# Changelog

This file records user-visible changes to Litiaina MX. Release dates use the
deployment maintainer's local date.

## [Unreleased]

No additional changes recorded yet.

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
