# Changelog

This file records user-visible changes to Litiaina MX. Release dates use the
deployment maintainer's local date.

## [2.2.0] - 2026-10-03

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
- Added an exact `font_size_px` preference from 10-24 px while retaining legacy
  percentage preference migration.
- Added focused interaction tests for message-composer keyboard behavior and
  record notification navigation.

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

### Fixed

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

### Upgrade notes

- Deploy the updated Rust binary and the committed `frontend/dist` bundle
  together, then restart MX. The new preview routes and automatic SQLite schema
  upgrades are registered at process startup.
- Existing `mx.config` files may omit `multipart_part_size_mb`; MX uses 16 MiB.
  If it is configured explicitly, N1's maximum allowed part size must be at least
  the same value.
- No manual database migration is required. MX applies the additive collaboration
  schema changes and preference compatibility handling automatically.

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
