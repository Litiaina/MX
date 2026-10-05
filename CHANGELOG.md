# Changelog

This file records user-visible changes to Litiaina MX. Release dates use the
deployment maintainer's local date.

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
