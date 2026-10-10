<div align="center">

  <img
    src="./assets/litiaina_icon.png"
    alt="Litiaina"
    width="150"
    height="150"
  />

  <h1>Litiaina MX</h1>

  <p>
    <b>Self-hosted, modular digital workplace platform</b><br>
    Bring configurable records, dashboards, collaboration, voice/video calls,
    screen sharing, and personal file storage into one workspace.
  </p>

</div>

<p align="center">
  <strong>Current version: MX 4.1.0</strong>
</p>

---

## Overview

**MX** is a self-hosted, modular digital workplace platform. It brings structured
information management, team collaboration, real-time calls, and personal/shared
file storage into one authenticated workspace.

Its information-management core remains schema-driven: deployments are not locked
to one predefined record format. Calling MX only a general-purpose information
system no longer describes the full product; that is one capability within the
broader workplace platform. MX is not a predefined ERP suite or a claim of Google
Drive/Discord feature parity.

A fresh MX deployment starts with a default Records module but **no mandatory business fields and no predefined dashboard statistics**. Administrators can add more modules, give each module its own fields and permissions, and build the information model through the application itself. Forms, record tables, search, attachment handling, N1 storage paths, dashboard widgets, statistics, and exports adapt to that configuration.

MX is therefore not a hard-coded document tracker, correspondence registry, inventory system, ticketing system, or workflow application.

It is the reusable system underneath those possibilities.

A deployment can use MX for:

- document and correspondence tracking;
- procurement and request records;
- inventory and asset information;
- incident and maintenance records;
- referrals and operational workflows;
- research or administrative datasets;
- office-specific registries;
- other structured information systems that can be represented as records and fields.

The core application stays the same while the deployment-defined schema changes.
People also use the same workspace to message their teams, meet through private
or group calls, share screens, and organize/share files through MX Drive.

---

## Core Principle

MX separates **platform responsibilities** from **business structure**.

```text
MX platform
├── identity
├── authentication
├── authorization
├── persistence
├── validation
├── search
├── attachment handling
├── N1 object storage integration
├── personal Drive and shared-file access
├── team collaboration and membership
├── call signaling and media controls
├── reporting
├── dashboard rendering
├── audit logging
└── backup safety

Deployment configuration
├── modules and module identities
├── module-specific fields and field order
├── module access matrices
├── validation and file-attachment fields
├── module-specific N1 folder layouts
├── table presentation
├── dashboard widgets
├── statistics
└── reporting dimensions
```

MX does not need to know what **Office**, **Department**, **Subject**, **Status**, **Action Taken**, **Control Number**, or any other organization-specific concept means.

If a deployment needs one of those concepts, the Administrator creates it as a field.

---

## Architecture

```text
Browser
   │
   │ HTTPS / JSON
   ▼
MX Server
Rust + Axum + Tokio
   │
   ├──────────────► SQLite
   │                structured state
   │                schema
   │                records
   │                users
   │                audit
   │                reports
   │                Drive metadata, versions, quotas and sharing
   │                backup metadata
   │
   └──────────────► N1
                    attachment bytes
                    immutable Drive file objects
                    verified database backups
```

### Browser

The browser interface is a Svelte 5 + TypeScript application built to static
assets in `frontend/dist` and served by the Rust process. It provides:

- Dashboard
- independently configured modules
- Collaboration channels and direct messages
- Notifications
- Administration
- attachment preview
- search and filtering
- account management
- personal theme, accent color, exact font size, interface scale, density, refresh, and notification preferences
- schema configuration
- reporting and exports
- real-time synchronization
- online/offline account presence
- parallel multi-user record editing
- personal/shared Drive, paginated folders, file versions, and upload queues
- collaboration spaces, messaging, and file sharing
- voice/video calls, screen sharing, and device controls
- linked-record selectors, lookups and rollups between modules
- bundled client-side Office editing with opt-in offline document copies

### MX Server

The Rust server owns:

- API routing;
- authentication and authorization;
- schema validation;
- dynamic record validation;
- transactional database operations;
- attachment coordination;
- N1 integration;
- search construction;
- dashboard/report queries;
- audit events;
- database backup operations;
- authenticated WebSocket sessions;
- live change broadcasting;
- account presence tracking;
- stateless optimistic record concurrency;
- Drive ownership, sharing, quotas, versions, and durable cleanup jobs;
- membership-scoped call state and signaling (browser WebRTC carries media).

### SQLite

SQLite stores structured and queryable state.

Binary attachment data is deliberately kept out of SQLite.

### N1

N1 stores:

- original uploaded attachment bytes;
- attachment versions;
- collaboration file shares;
- immutable MX Drive file objects and versions;
- the uploaded deployment logo;
- verified SQLite backup objects.

The N1 object is authoritative for file content. Preview files are derived data and may be regenerated.

---


## Real-Time Collaboration

MX includes authenticated real-time synchronization for multi-user deployments.

Connected browsers maintain a WebSocket session with the MX server. When records,
attachments, schema configuration, dashboard configuration, deployment settings,
or account presence change, MX can notify connected clients immediately instead
of waiting for manual refreshes.

### Reliable live connection

The browser reconnects indefinitely if the WebSocket is interrupted.

Reconnect behavior uses bounded exponential backoff: the delay can increase during
an outage, but MX does not permanently give up after an arbitrary retry count.
After a successful reconnect, the client resynchronizes authoritative state from
the normal HTTP API so events missed during the outage do not leave the interface
stale.

MX also uses heartbeat/liveness handling so long-running browser sessions can
detect dead connections and recover.

### Network failure and save recovery

Temporary connection errors and server outages preserve session credentials.
At startup, a failed session check shows a reconnect screen; only a definitive
authentication rejection signs the user out. Live connections have a silent-link
watchdog, an opening deadline, jittered reconnect backoff, and recovery triggers
when the browser returns online or visible. Reconnection refreshes records,
dashboards, modules, notifications, collaboration, preferences, and call discovery
from the HTTP API. Existing record rows remain mounted during refresh.

JSON API requests have finite deadlines covering headers and the response body.
Reads retry transient failures up to three attempts. Other writes are not
automatically replayed unless their endpoint explicitly supports it. Record and
message creation carry a client operation UUID, bound to the account, target,
and submitted payload in SQLite in the same transaction as the write. Replaying
that operation returns the existing resource. Deploy the updated frontend and
backend together; the replay guarantee requires the updated backend.

Buffered downloads, document previews, profile photos, and custom sounds also
detect a stalled response body (60 seconds without a chunk). Failed reads retry
from the beginning, never treating a partial buffer as a complete file. Buffered
transfers have a 30-minute total deadline; native video/range playback does not
use this buffer or total deadline.

A pending new-record operation is retained with its draft in the current tab's
session storage. A lost save response does not mean the server failed to save:
retry the retained operation to reconcile it. Inline cell editing keeps the
original revision loaded when editing began, even after a background refresh.
Concurrent edits still use the normal field-conflict decisions.

Chat composers retain separate in-memory drafts per conversation. After an
uncertain send, **Retry send** reconciles the original message operation;
an attachment error does not create another base message. Switching conversations
is paused only while a send is actively running. Chat drafts and local file
selections are not a persistent outbox and do not survive a full page reload.

Uploads have progress/stall and processing deadlines. MX retries transient N1
standard-multipart failures at the same part index with identical bytes, and
checks the committed version after an uncertain finalize response. This is not
browser-to-MX resumable upload: after an interrupted attachment request, inspect
the attachment list before retrying. Local file selections cannot be restored
after a page reload. One-shot uploads and arbitrary mutations are not blindly
replayed. N1 streams use a 60-second read-idle timeout, not a total video duration
limit; connect/authentication (including waiting for a shared token refresh) and
non-streaming mutations have separate bounds. Invalid HTML upload acknowledgements
are reported as uncertain writes, never as a successful attachment.

Call signaling retries lost offers and answers without rebuilding healthy media
connections. Duplicate descriptions and obsolete sessions are ignored; candidate
replay and staggered ICE restart handle interrupted negotiation/connectivity.
This cannot replace a TURN relay on restrictive networks or guarantee usable
audio/video while the underlying network is disconnected.

### Voice, video, and screen sharing

Conversation members can start or join a voice call, enable a camera, and share
a screen without leaving MX. The call dock remains available while the user
navigates between records, dashboards, and collaboration. Media is transported
between browsers with WebRTC; authenticated MX HTTP and private WebSocket events
provide membership enforcement, room presence, and offer/answer/ICE signaling.

Direct conversations produce an incoming private-call card; spaces produce a
group-call card with the caller, space, participant count, and voice/video join
actions. A private call rings for at most 30 seconds and uses a persistent
operating-system alert when enabled; a group call uses one softer chime and a
non-blocking alert. Answering, declining, joining from another signed-in tab, or
the call ending stops the active alert. Ongoing calls are restored after page refresh or live reconnection and
remain visible as call badges in the conversation list. The call dock includes
mute, deafen, camera, screen sharing, input/output device selection, elapsed
time, focused-video layout, and native full-screen viewing for shared screens.
When the browser and selected capture source support it, screen sharing also
transports tab/system audio on a sender independent from the microphone; users
must enable the browser's **Share audio** option in the capture picker.
Active microphone tracks drive live speaking rings on participant tiles, and the
local device panel includes an outgoing preview, input-level meter, microphone
and camera selection, browser-supported speaker routing, and an audible output
test. These indicators are calculated locally from call media and do not send
recorded audio to the MX server.

Camera and screen-share playback stays attached to the current video track
during unchanged participant heartbeats and microphone-only updates. When the
video source changes, a bounded still frame covers the transition until the
replacement is decoded. Each tile uses one playing video, avoiding overlapping
video layers and fullscreen crossfade effects. The microphone uses a separate
audio-only playback stream that is preserved when the video source changes.
The device-panel video preview also avoids restarting for microphone-only changes.
In group calls, each viewer receives source updates independently. A newly
received video track updates its tile even if the sharing status arrived first;
adding video to a voice call starts negotiation immediately, and queued changes
resume when the current offer is answered. Static screen shares can be displayed
from their first decoded frame without waiting for another screen change.

Your own shared-screen preview is hidden by default; **Show my preview** and
**Hide my preview** control local viewing without stopping the stream sent to
others. Starting a share keeps the current call layout instead of automatically
enlarging your own screen. Native fullscreen of your own share displays sharing
status with the local preview paused, preventing the shared screen from
recursively capturing its fullscreen video. Remote viewers can still watch the
share in fullscreen. The capture picker requests exclusion of the current MX
tab where the browser supports it, and the device panel hides shared-screen
self-preview as well.

Call access follows conversation membership. A removed member is immediately
evicted from an active call, signaling is addressed only to joined members, and
inactive participants expire automatically. MX reconnects call signaling and
rejoins a participant after long background-tab timer throttling.

Each browser join uses a separate call-session identifier. Quitting and
rejoining creates fresh peer connections, while delayed leave or WebRTC
signaling from the previous join is ignored. This prevents new microphone,
camera, and screen-share tracks from attaching to a stale peer. Obsolete ICE
candidates from an offer rollback or restart are discarded without failing the
call; a genuinely failed connection triggers ICE restart and renegotiation.
Peer pairs choose one deterministic initial offerer and use atomic browser
offer/answer application so simultaneous discovery cannot produce negotiation
glare during join or rejoin.

Browser media APIs require a secure context. Use HTTPS for deployed MX instances
(`localhost` is the browser's development exception). Direct/LAN candidates can
work without an ICE service, but reliable calls across NAT, VPN boundaries, or
restrictive firewalls require deployment-owned STUN/TURN configuration. This
release uses a bounded peer mesh rather than an SFU, so `max_participants` is
limited to 12 and deployments should choose a lower limit when client upload
bandwidth is constrained.

### Account presence

Authenticated sessions contribute to deployment-wide presence information.

MX can show whether an account is currently:

```text
Online
Offline
```

Multiple browser tabs for the same account are treated as one online account for
presence display.

Presence is informational. It is **not** a record lock.

### Parallel editing without record locks

MX deliberately does not lock an entire record when somebody opens or edits it.

For example, these operations can happen at the same time:

```text
User A -> changes Status
User B -> changes Routed To
User C -> uploads a supporting document
```

Independent changes can be committed without forcing the users to wait for one
another.

When a record opens, the client retains its loaded record revision and edits
locally without leases, presence heartbeats, or server editing sessions. Save
sends only changed fields plus that base revision. The server atomically merges
fields whose last change predates the base and returns a field conflict only
when the same logical field changed. Conflict responses contain the stored and
proposed values so the user can choose either one. Attachment additions merge;
same-attachment operations are conflict checked and advance the same revision.

This prevents silent lost updates while preserving parallel work on unrelated
fields and attachments.

### Live events

The real-time layer can publish compact events such as:

```text
record.created
record.fields.updated
record.deleted

attachment.created
attachment.deleted

schema.updated
storage.updated
dashboard.updated
deployment.updated

presence.user.online
presence.user.offline
sync.required
```

WebSocket events are notifications, not the durable source of truth.

```text
WebSocket -> something changed
HTTP API  -> fetch authoritative state
SQLite    -> authoritative structured state
N1        -> authoritative attachment bytes
```

This keeps reconnection and recovery deterministic.

---

## MX Drive

**MX Drive** in the workspace navigation is personal file storage backed by N1.
My Drive, Shared with me, Recent, Starred, and Trash provide folder browsing,
search, list/grid layouts, file-type icons, image thumbnails, preview navigation,
downloads, rename/move, file versions, and activity. The interface follows MX's
neutral light/dark surfaces independently of the custom accent color.

Use **Upload files** or **Upload folder** to queue files; folder uploads preserve
the selected directory structure. You can also drop files or folders into Drive
(or directly onto an owned folder), or paste an external clipboard image/file
while Drive is open. Directory drops preserve nested and empty folders when the
browser exposes directory entries; use **Upload folder** as a fallback.
The queue admits two transfers at a time:
multipart files use console's default 1–4 concurrent parts each; groups of up to 16 files
of at most 1 MiB use N1's small-file batch endpoint. A failed batch entry does not
roll back successful files. Pausing a grouped small-file batch pauses that group.

Drive uploads use N1's restart-persistent **durable multipart** protocol. MX keeps
the N1 session and unique object identity in SQLite, reconciles N1's stored part
indices on resume, and sends only missing parts. Drive follows the console's
default part sizing: below 16 MiB, one part; 16–<256 MiB, 4 MiB parts;
256 MiB–1 GiB inclusive, 8 MiB parts; above 1 GiB, 50 MiB parts. N1 must
permit 50 MiB parts. There is **no Drive per-file size cap**; available account
quota and storage capacity still apply. Existing sessions retain their negotiated
part size when resumed. Lost finalization acknowledgements are reconciled
against the exact committed N1 version and size using N1's metadata endpoint.
MX does **not** download the whole object again before publishing it.

Like console, resumable uploading starts with a metadata-only session request and then
bounded native Blob parts—**no whole-file hashing or pre-scan**, regardless of
file size. Multipart quota is checked before reading/sending file bytes. N1 owns multipart
validation; MX retains immutable per-part SHA-256 receipts to reject different
bytes replayed into the same slot. Multipart versions do not claim a computed
whole-file checksum. Small files (up to 1 MiB each) use bounded batches with
per-entry quota enforcement. Scheduling does not allocate a queue entry for every part.
The old buffered
`POST /mx/v1/drive/upload` endpoint is retired (410); clients must use
`POST /mx/v1/drive/uploads`, part PUTs, and finish.

The upload panel is global to the authenticated workspace, fixed at the lower
right, collapsible, and independent of custom accent colors. Switching modules
or collaboration views does not abort its queue. It shows in-flight transferred
bytes, smoothed **MB/s**, estimated time remaining, pause/resume, and per-file
errors. Speed measures browser-to-MX transfer (decimal MB/s); N1 processing is
shown separately as finalization, not falsely counted as a finished upload.
Two files run at once, with up to eight part workers overall. Transient part
failures retry the identical slot/body with bounded backoff.
Each file's progress is a circular ring around its icon, not a linear bar.

### File-manager controls

Folders use server-side metadata pagination (50 items by default, selectable
25/50/100), a result range, numbered pages, next/previous and a page jump for long
lists. Name/date sorting and search run before SQL LIMIT/OFFSET, not on a loaded
whole folder. Empty last pages clamp after concurrent moves or deletions. Pickers
and public guest folders use bounded pages too. Selection is intentionally scoped
to the displayed page; changing pages clears that selection.

File previews have previous/next buttons and Left/Right shortcuts across page
boundaries. Neighbor queries seek by sort key and stable item ID, return only the
adjacent metadata, and load bytes only for the selected file. Text preview reads
at most 256 KiB with HTTP Range; larger text stays available through Download.
Images/video retain native streaming and range behavior. Visible image thumbnails
remain original-image previews, not generated small thumbnails.

Click once to select; double-click to open a folder or preview a file. Use the
checkboxes, Ctrl/Cmd-click to toggle items, Shift-click for a range, or drag a
rectangle from the blank area below the file rows. Ctrl/Cmd+A selects the current
page (up to 100 visible items). Shortcuts do not intercept text fields or dialogs.

Use **Copy** / **Cut**, or Ctrl/Cmd+C / Ctrl/Cmd+X, open the destination folder,
then **Paste** or Ctrl/Cmd+V. The account-scoped clipboard survives switching MX
modules but is cleared at signout. External files on the clipboard upload instead
of pasting the internal selection. A move is owner-only and atomic across the
selection; collisions or stale revisions reject the whole move. Copies receive
new MX IDs and ownership, preserve the current file version (not the original's
history/shares), and get a `(copy)` name on collisions, keeping the extension.
At most 100 selected roots / 5,000 nested items may be copied per request.

Right-click an item for contextual actions; right-click blank space to upload,
create a folder, or paste. These menus exist only inside the Drive file area;
other MX views retain their normal right-click behavior. The row's **More actions**
button provides the same controls on touch devices. Escape closes the menu;
arrow keys navigate it. F2 renames a single selection; Delete offers trash confirmation.

**More actions → Versions and activity** separates version cards from a dated
activity timeline. Versions have explicit Preview and Restore actions. Restoring
creates a new normal version, keeps all previous history, checks quota and the
loaded revision, and does not download the file through the browser.

Pause/resume works while the file remains selected. Closing/reloading the page
warns while transfers are active; signing out aborts and clears that account's
in-memory queue. After reload, reselect the **original file** in the same
destination to resume. MX recovers pending sessions by account, destination,
name, size, type, and modification time, with N1 as the stored-part authority;
this metadata identity is not a full-content checksum. No browser storage of
durable session state or whole-file scan is needed. Inactive pending server
reservations expire after one hour and queue cleanup; resuming after expiration
may need a new upload. This is not an offline outbox or background browser service.
Active-document credentials are retained in memory if browser storage is lost;
explicit signout and rejected authentication clear them. A browser process crash
can also invalidate selected file handles: reselect the original file to resume
the acknowledged parts. MX never accepts a truncated part as a successful upload.

### Metadata, ownership, and sharing

MX owns names, folders, ownership, grants, revisions, and trash. N1 receives only
immutable unique keys such as `__mx/drive/<owner-uuid>/<version-uuid>.MP4`.
Each version preserves its uploaded filename's original extension and casing.
Renaming/moving changes MX metadata only, never that key or the N1 object.
Drive-to-Drive copies and version restores reuse immutable N1 content through
independent MX version records. They never transfer the file's bytes or rename
its original storage key. Logical copies/versions each count toward their owner's
quota. Cleanup excludes every key still referenced by any Drive version, so
purging one copy cannot delete another copy's content. A copied object's N1 key
continues to identify its original upload, not the copy's new MX owner.
Names must be unique within the same owner's folder, case-insensitively; use
**New version** in the Versions tab when replacing an existing file.

Files are private by default, including from other administrator accounts.
Owners can share with authenticated MX users or active collaboration spaces/groups
they belong to, as viewers or editors. Space grants follow current membership:
new members inherit access; leaving, removal, archival or revocation removes that
grant immediately, including already-issued media tickets. Independent user/space
grants can still provide access; the strongest inherited role applies. Direct
conversations are not group-share targets. Folder grants
inherit to descendants. Viewers can preview/download; editors can rename and
upload file versions. Moving, trashing, creating children, and managing sharing
remain owner actions. Record-module grants do not grant access to private Drive
files. Sharing changes are persisted in Drive activity and authenticated API audit.
The Share dialog separates people, spaces, inherited folder access and public links
and keeps identity/role/removal controls readable on desktop and mobile.
If an open folder is deleted or its access is revoked, the next refresh clears
the stale folder listing and returns to the current view's root with an explanation.

Owners can also create read-only guest links, optionally expiring after a selected
period. Anyone holding a link can read its shared file/folder subtree without
signing in. Links are stored hashed, never disclose N1 credentials/object keys,
and can be revoked. Trashing permanently revokes affected guest links; restoring
does not reactivate them. Every media request rechecks access/expiry. Private
preview tickets pin a version; guest requests reject a changed listing revision
instead of mixing byte ranges from different file versions.

**MX Drive** in the message composer and **Choose from MX Drive** on a record's
attachment field attach independent copies through the existing attachment
workflow. Trashing the original does not break those attachments. Destination
permissions and attachment limits still apply. Copying currently downloads the
file into the browser before uploading the new attachment; it is not zero-copy.

### Trash, quotas, and scope

Every account starts with **10 GB** (10 GiB). An administrator can assign a
different allowance under **Administration → Accounts → Storage** for any
account, including their own, or reset it to the deployment default. The panel
shows committed usage and pending reservations. Set zero to prevent new bytes;
lowering an allowance never deletes files. Quota changes are revision-checked
and audited; uploads recheck the owner's current allowance atomically at commit,
including versions uploaded by a shared editor. The Drive sidebar shows a storage
meter, usage, allowance, and full-storage guidance.

Moving to trash updates MX metadata only. **Delete permanently** / **Empty trash**
atomically remove MX metadata and enqueue durable background jobs. Those jobs retry
N1 **soft deletion**, including after restart/outage; only N1's TTL/GC physically
reclaims bytes. No permanent-delete N1 endpoint is assumed. Existing versions and
trash count toward the owner's quota; pending uploads reserve quota and commits
recheck it atomically. Backup both MX SQLite metadata and N1 storage.

Drive lists refresh quietly every 15 seconds while visible and no preview/dialog
is open, and when coming
online; this is not instant Drive-specific live synchronization. Native
video/audio previews use range-capable streaming. The existing Office PDF preview
uses optional server-side conversion. **Edit in MX Office** is a separate bundled
browser editor: see [Standalone Office editing](#standalone-office-editing).
This is not Google Docs-style real-time co-editing, desktop sync, OCR/full-text
indexing, or Google Drive feature parity.

---


## Modules and Dynamic Record Structure

Each **Module Structure** is the source of truth for one part of a deployment's business model.

A fresh installation can contain multiple independently configured modules, and each module can contain zero fields.

Administrators add only what that deployment requires.

### Account module isolation

Module authorization is the intersection of two server-side layers:

1. the account's system role defines the maximum capability ceiling; and
2. an explicit account grant selects which modules and capabilities that person
   actually receives.

Non-administrator accounts are default-denied. A module without an explicit
grant is absent from navigation and is rejected by its schema, record, search,
report, attachment, notification, and collaboration-linked-record APIs.
Administrators always retain full access to active modules so an accidental
grant edit cannot lock the deployment out of configuration.

Each account/module grant can independently allow view, create, edit,
archive/delete, structure configuration, reports, and attachments. A grant can
reduce its role's permissions but cannot exceed them. For example, an Editor
grant requesting delete access is still denied because the Editor role ceiling
does not allow record deletion.

Module-access saves include an optimistic revision key. If two Administrator
sessions edit the same account concurrently, exactly one save advances the
revision; the stale save receives HTTP `409 Conflict` and must reload before it
can overwrite the newer decision. Every successful access change is written to
the audit log and sent only to the affected account so its navigation refreshes
without exposing the assignment to other users.

### Supported field types

```text
Text
Long Text
Integer
Decimal
Formula / Calculated
Relationship / Linked record
Lookup from linked records
Rollup / Aggregate linked records
Date
Boolean
Select
Auto Number
File Attachment
```

### Relationships, lookups and rollups

In **Administration → Modules & fields**, select the source module and add a
**Relationship / linked record** field. Choose the target module, its display
field, and whether multiple records may be selected. For example, a Billing
record can link to a Patient without copying the patient's name into every bill.
Stored values are stable record IDs; display labels follow subsequent renames.
Forms and table-cell editing provide searchable, paginated selectors.

Add a **Lookup** to display another field from those selected records, or a
**Rollup** to count them or calculate sum, average, minimum or maximum of a
numeric linked field. These values are read-only and calculated when records are
loaded. Empty links produce count/sum zero; average/min/max are empty. Lookups
return a list of values, even for a single link. A record's **Related records**
section lists accessible records referencing it, with paged navigation.

Example API field configurations:

```json
{"field_type":"relationship","config":{"target_module_uid":"<module UUID>","label_field":"name","multiple":false}}
{"field_type":"lookup","config":{"relationship_field":"patient","target_field":"department"}}
{"field_type":"rollup","config":{"relationship_field":"selected_bills","target_field":"amount","function":"sum"}}
```

Send a record UUID for a single relationship, an array of UUIDs for a multiple
relationship (up to 100), or null/an empty array to clear an optional link.
Changes participate in the normal record `base_revision`/PATCH merge and version
history. Two edits to the same relationship field conflict; unrelated fields
still merge. Lookup/rollup fields cannot be submitted as editable values.

Both modules must be accessible. MX validates target membership and active state
inside the write transaction. Hidden target IDs, labels, lookups and rollups are
redacted, not merely hidden by the UI. Linked records cannot be trashed while
active records reference them. Clear links or trash their sources first; restore
targets before restoring sources. Reconfigure/archive dependent calculated
fields and clear stored links before archiving a relationship. Active relationship
definitions prevent deleting their target module.

Current scope: rollups aggregate **explicitly selected outgoing links**, not all
inverse child records automatically. Derived values are current-data views, not
historical computed snapshots. Relationships/lookups/rollups cannot be sorted,
raw-searched, used as N1 path components or referenced by formulas. Existing
dashboard/report SQL measures do not aggregate these on-demand computed fields;
CSV exports omit all three new field types rather than exposing raw linked IDs.
This does not turn MX into a SQL administration tool.

### Common field behavior

Depending on the field type, a field can define:

```text
Required
Unique
Searchable
Sortable
Show in records table
Table priority
Field order
Type-specific configuration
```

Fields use stable internal UIDs so labels and presentation can evolve without changing the identity used by stored records.

Archived fields can remain represented in historical data instead of forcing destructive schema changes.

---

## File Attachment Is a Normal Field Type

MX does **not** require one permanent built-in attachment area.

`File Attachment` is an ordinary Record Structure type.

An Administrator can create:

```text
Supporting Documents    File Attachment
Signed Copy             File Attachment
Photo Evidence          File Attachment
Action Documents        File Attachment
```

or create **no attachment fields at all**.

Each File Attachment field can define:

- its field name;
- its N1 storage-folder name;
- an optional user-facing description;
- whether multiple files are allowed;
- the maximum number of files;
- whether the field is required.

Large files are transferred to N1 in bounded multipart segments rather than as
one oversized N1 part. Images receive record-level thumbnails, and the record
viewer provides previous/next navigation across every attached file. Browser
media previews use short-lived access tickets and forward HTTP byte ranges to
N1 so video and audio can begin without downloading the complete object first.

Because attachment fields are part of Record Structure, the same schema drives:

- the record form;
- attachment upload controls;
- table presentation;
- search;
- reporting;
- exports;
- N1 storage organization.

---

## Record Model

MX keeps the internal identity of a record small and stable.

Business values are stored separately and interpreted using Record Structure.

```text
Record
├── UID
├── Created At
├── Updated At
│
├── Dynamic Values
│   ├── Field UID -> Value
│   ├── Field UID -> Value
│   └── ...
│
└── File Attachment Fields
    ├── Field UID
    │   ├── Attachment
    │   ├── Attachment
    │   └── ...
    └── Field UID
        └── Attachment
```

There is no required hard-coded `Office`, `Date`, `Subject`, `Requestor`, `Routing`, `Status`, or `Control Number` column in the business model.

---

## Auto Number

`Auto Number` is a dynamic field type.

A deployment may have one auto-number field, several, or none.

The server allocates authoritative sequence values transactionally.

Configuration can include:

```text
Scope     global | yearly
Prefix    optional text
Padding   0..12
```

Examples:

```text
1
000001
REQ-000001
2026-000001
```

The browser does not allocate the authoritative sequence value.

---

## Formula Fields

`Formula / Calculated` fields are server-computed numeric fields. A formula
references the stable keys of other numeric fields and is recalculated whenever
the record changes.

For example, fields with keys `value` and `khw` can produce a `total` field:

```text
value * khw
```

Expressions support parentheses, the `+`, `-`, `*`, `/`, `%`, and `^`
operators, the `pi` and `e` constants, and these functions:

```text
SUM       PRODUCT    AVERAGE / AVG
MIN       MAX        ABS
CEIL      FLOOR      SQRT
LN        LOG10      EXP
SIN       COS        TAN
POWER     MOD        ROUND
CLAMP
```

Formula fields are read-only in record forms and tables. The server validates
the expression and field references, stores the calculated value, and includes
it in search, sorting, reports, history, and exports.

---

## Dynamic Search

Search is generated from the active Record Structure rather than hard-coded business columns.

### Universal Search

Universal Search searches active fields marked `Searchable` and attachment filenames across every module the current account may read. Results are permission-filtered on the server and open the matching record directly.

### Field Search

Queries can use dynamic field UIDs for field-specific filtering.

MX supports server-side:

- search;
- filtering;
- sort selection;
- sort direction;
- pagination.

The browser does not need to download the complete database just to find or display records.

Wide record tables have a slim, synchronized horizontal scrollbar at the bottom
of the visible table area when their normal bottom scrollbar is off-screen.
Drag it or focus it and use Left/Right, Home/End or Page Up/Down. It disappears
when the normal scrollbar is reachable, columns fit, mobile cards are shown or
a record editor is open. Table styling and server-side pagination are unchanged.

---

## Configurable N1 Storage Layout

Administrators can select fields from each module to build that module's base N1 hierarchy. A field from one module cannot be reused accidentally by another module's layout.

A File Attachment field automatically contributes its own final folder segment.

Example:

```text
Record Structure

Office                 Select
Year                   Integer
Supporting Documents   File Attachment
Signed Copy             File Attachment
```

Storage layout:

```text
Base folder 1: Office
Base folder 2: Year
```

Result:

```text
records/
└── MISO/
    └── 2026/
        ├── Supporting Documents/
        │   ├── request.pdf
        │   └── quotation.xlsx
        │
        └── Signed Copy/
            └── approved.pdf
```

If no schema-derived base folders are configured, MX can use the record identity as the stable base:

```text
records/<record-uid>/<File Attachment field>/filename.ext
```

### Frozen record namespace

Once a record establishes its storage namespace, MX freezes that base namespace for the record.

This prevents later edits to values such as office, division, or date from scattering one record's attachments across unrelated paths.

Changing the global storage-layout configuration affects records whose storage namespace has not yet been established.

Existing N1 object keys are not silently moved just because a label or module layout changes.

Custom modules use separate `records/<module-slug>/` roots. The migrated default Records module keeps its existing `records/` namespace for compatibility.

---

## Custom Dashboard Builder

MX does not ship with organization-specific dashboard assumptions.

A new deployment starts with:

```text
Dashboard

No dashboard widgets configured.
```

Administrators build the dashboard from Record Structure.

### Widget types

MX supports:

```text
Summary number / KPI
Bar chart
Line chart / trend
Pie chart
Donut chart
Grouped progress
Detailed statistical table
```

### Measure rules

A widget can measure records using rules such as:

```text
Every record
Field equals a value
Field does not equal a value
Field is empty
Field is not empty
File Attachment field has files
```

The Administrator chooses the fields. MX does not hard-code business meaning into these rules.

### Grouping

Categorical widgets can group results by a deployment-defined field.

Example:

```text
Title: Records by Department
Display: Pie chart
Measure: Every record
Category: Department
```

### Completion / match statistics

Example:

```text
Title: Completion by Division
Display: Bar chart

Measure:
Action Taken is not empty

Group by:
Division
```

If a division has 100 records and 50 match the configured rule, the report can represent:

```text
Total          100
Matched         50
Not matched     50
Match rate      50%
```

`Action Taken` and `Division` are merely example field names. MX does not require them.

### Time-series statistics

Line charts can use any `Date` field as the time axis.

Supported aggregation intervals:

```text
Day
Week
Month
Quarter
Year
```

Example:

```text
Title: Monthly Completed Requests
Display: Line chart

Date field:
Date Received

Interval:
Month

Measure:
Status equals Completed
```

### Chart options

Categorical charts can limit the number of displayed categories.

Current category limits include:

```text
5
8
10
15
25
All
```

For count-based views, remaining categories can be combined into `Other`.

Pie and donut charts use count-based values so their slices represent a meaningful whole.

### Dashboard persistence

Dashboard configuration is stored by MX.

Administrators can:

- add widgets;
- edit widgets;
- reorder widgets;
- remove widgets;
- save the final dashboard configuration.

Only configured widgets appear on the dashboard.

---

## Reporting and CSV Export

MX performs reporting on the backend instead of fetching the entire records database into the browser.

The reporting engine supports:

- grouping by dynamic fields;
- match rules;
- empty/non-empty conditions;
- exact values;
- attachment presence;
- date ranges;
- time buckets;
- total counts;
- matched counts;
- not-matched counts;
- match rates.

### Detailed Records export

The detailed CSV export includes active Record Structure fields.

File Attachment fields receive their own attachment filename columns, together with attachment totals.

### Attachment inventory

Attachment inventory can include attachment metadata such as:

- record UID;
- attachment field UID;
- filename;
- size;
- N1 object key.

### User performance

Administrator reporting can derive activity metrics from the audit log, including:

```text
records created
records updated
attachments uploaded
records deleted
attachment downloads
successful actions
failed actions
unique records touched
last activity
```

These are audit-derived operational statistics. They are not treated as business completion metrics unless the deployment explicitly configures a business field/rule for that purpose.

---

## Administration

Administration is separated into function-specific tabs.

```text
Accounts
Modules & Fields
Dashboard
Trash
Deployment
Backups
Updates
Audit Log
```

### Accounts

Manage users, system-role ceilings, account recovery, and explicit module
grants. Newly created non-administrator accounts start with no module grants;
an Administrator deliberately assigns the modules and capabilities required for
that person's work.

### Modules & Fields

Create modules and maintain each module's identity, access matrix, dynamic schema, and N1 layout.

### N1 Storage

Choose the dynamic fields used in the storage hierarchy and optional filename-prefix behavior.

### Dashboard

Create and maintain dashboard/statistics widgets.

### Backups

Create, verify, list, and download SQLite backup snapshots stored in N1.

### Audit Log

Review authenticated system activity.

These configuration areas are intended for Administrators.

### Trash and record history

Record deletion is recoverable. Administrators can restore soft-deleted records from Trash, while users with edit access can inspect saved record versions and restore an earlier field state. Attachment metadata is included in version snapshots.

### Updates

Release discovery and installation remain deliberately disabled until the signed-artifact, update-channel, rollback, and maintenance-window contract is defined. The public repository alone is not treated as sufficient authority to replace a running production binary.

---

## Access Levels

```text
0 = Administrator
1 = Manager
2 = Editor
3 = Viewer
```

| Role | General capabilities |
|---|---|
| Administrator | Full platform access plus accounts, modules, schema, storage layouts, dashboard configuration, database administration, audit, and backups |
| Manager | Defaults to read, create, edit, upload, download, and delete; each module can override its matrix |
| Editor | Defaults to read, create, edit, upload, and download; each module can override its matrix |
| Viewer | Defaults to read, search, preview, and download; each module can override its matrix |

These role defaults are capability ceilings, not automatic module membership.
A non-administrator must also have an explicit account grant for the module,
and the effective permission is the intersection of both layers. Authorization
is enforced by the backend rather than only by hiding interface controls.

---

## Authentication

MX supports:

```text
JWT access tokens
Refresh tokens
Optional TOTP two-factor authentication
Single-use recovery codes
First-run Administrator bootstrap
```

A fresh deployment can create its first Administrator only while the user table is empty and the bootstrap authorization requirement is satisfied.

Passwords are stored as salted Argon2id hashes. TOTP secrets remain pending until
the user confirms a valid authenticator code. Password, email, authenticator, and
recovery-code changes revoke existing access and refresh tokens. Administrator
password and authenticator resets require the Administrator to re-enter their own
password and second factor.

---

## Audit Logging

MX records meaningful authenticated actions for Administrator review.

Examples include:

```text
record.create
record.update
record.delete

attachment.upload
attachment.delete
attachment.preview
attachment.download

schema.field.create
schema.field.update
schema.order.update

account.create
account.modify
account.delete
account.self.profile
account.self.password
account.self.2fa.enroll
account.self.2fa.disable
account.self.recovery-codes.regenerate
account.admin.password-reset
account.admin.security-reset

database.query
audit.view

backup.create
backup.verify
backup.download
```

Passwords, JWTs, authentication secrets, request bodies, uploaded file bytes, and SQL text are not copied into normal audit events.

---

## Database Backups

MX can create a consistent live SQLite snapshot and store the verified result in N1.

```text
SQLite
  │
  ▼
VACUUM INTO
  │
  ▼
standalone database snapshot
  │
  ▼
PRAGMA integrity_check
  │
  ▼
N1
```

New MX backups use:

```text
__mx/backups/database/YYYY/MM/DD/
```

Example:

```text
__mx/backups/database/2026/09/19/mx-20260919T120000Z.db
```

Backup verification can download the stored object, check its expected size, open it as SQLite, and run `PRAGMA integrity_check` again.

Database restore remains a deliberate offline administrative operation rather than a one-click browser action.

---

## Main SQLite State

Important MX tables include:

```text
mx_records
mx_modules
mx_module_permissions
mx_module_account_permissions
mx_module_access_revisions
mx_schema_meta
mx_fields
mx_record_values
mx_unique_values
mx_field_sequences
mx_attachments
mx_record_storage
mx_module_storage_layout_meta
mx_module_storage_layout_folders
mx_record_versions
mx_record_version_values
mx_record_version_attachments
mx_dashboard_config
mx_user_preferences
mx_notifications
mx_channels
mx_messages
mx_audit_log
mx_backups
```

The exact internal schema may evolve, but deployment-defined business fields remain data rather than Rust struct columns.

---

## HTTP API

MX uses the `/mx/v1` API namespace.

### Authentication

```http
GET  /mx/v1/auth/bootstrap/status
POST /mx/v1/auth/create
POST /mx/v1/auth/authenticate
POST /mx/v1/auth/refresh
GET  /mx/v1/auth/session
```

### Account administration

```http
POST   /mx/v1/auth/get
PATCH  /mx/v1/auth/modify
DELETE /mx/v1/auth/delete

POST   /mx/v1/user/create
PATCH  /mx/v1/user/modify
DELETE /mx/v1/user/delete

POST /mx/v1/admin/accounts/{uid}/password-reset
POST /mx/v1/admin/accounts/{uid}/security-reset
```

### My Account and security

```http
PATCH  /mx/v1/account/profile
POST   /mx/v1/account/password
POST   /mx/v1/account/totp/enroll
POST   /mx/v1/account/totp/confirm
DELETE /mx/v1/account/totp/enroll
DELETE /mx/v1/account/totp
POST   /mx/v1/account/recovery-codes/regenerate

GET /mx/v1/account/preferences
PUT /mx/v1/account/preferences
```

Authenticator enrollment uses a pending secret. Password-only login remains possible while setup is incomplete, but once confirmation activates the secret every password login requires a valid TOTP or single-use recovery code.

### Modules

```http
GET  /mx/v1/modules
POST /mx/v1/admin/modules
PUT  /mx/v1/admin/modules/{module_uid}

GET /mx/v1/admin/accounts/{account_uid}/modules
PUT /mx/v1/admin/accounts/{account_uid}/modules

GET  /mx/v1/modules/{module_uid}/schema
GET  /mx/v1/relationships/{field_uid}/options?q={query}&page=1
GET  /mx/v1/relationships/{module_uid}/{record_uid}/related?page=1
POST /mx/v1/admin/modules/{module_uid}/fields
PUT  /mx/v1/admin/modules/{module_uid}/schema/order

GET /mx/v1/search?q={query}
```

### Schema

```http
GET  /mx/v1/schema

GET  /mx/v1/admin/schema
POST /mx/v1/admin/schema/fields
PUT  /mx/v1/admin/schema/fields/{uid}
PUT  /mx/v1/admin/schema/order
```

### N1 storage layout

```http
GET /mx/v1/admin/storage-layout
PUT /mx/v1/admin/storage-layout

GET /mx/v1/admin/modules/{module_uid}/storage-layout
PUT /mx/v1/admin/modules/{module_uid}/storage-layout
```

### Dashboard

```http
GET /mx/v1/dashboard/config
PUT /mx/v1/admin/dashboard-config
```

### Reports

```http
GET /mx/v1/reports/action-rate
GET /mx/v1/reports/user-performance
GET /mx/v1/reports/export.csv
```

The `action-rate` endpoint is schema-driven: its action/group/date fields are supplied dynamically rather than representing hard-coded MX business fields.

### Records

```http
GET    /mx/v1/records
POST   /mx/v1/records

GET    /mx/v1/records/{uid}
PATCH  /mx/v1/records/{uid}
PUT    /mx/v1/records/{uid}
DELETE /mx/v1/records/{uid}

GET    /mx/v1/modules/{module_uid}/records
POST   /mx/v1/modules/{module_uid}/records
GET    /mx/v1/modules/{module_uid}/records/{uid}
PATCH  /mx/v1/modules/{module_uid}/records/{uid}
PUT    /mx/v1/modules/{module_uid}/records/{uid}
DELETE /mx/v1/modules/{module_uid}/records/{uid}

GET  /mx/v1/modules/{module_uid}/records/{uid}/versions
GET  /mx/v1/modules/{module_uid}/records/{uid}/versions/{version_uid}
POST /mx/v1/modules/{module_uid}/records/{uid}/versions/{version_uid}
```

`PATCH` is the preferred path for collaborative editing because it sends only
the fields actually changed by the client and participates in field-level
revision checks. `PUT` remains available for full-record update behavior.
It now requires the loaded `base_revision` alongside `values`: a missing base
returns **428 Precondition Required**, and any intervening record change returns
**409 Conflict** without modifying data. Fetch the record again before deliberately
replacing it; use `PATCH` for independent field changes that should merge.
Trash and restore also advance the record revision, so drafts opened before a
deletion cannot silently overwrite a restored record.

### File Attachment fields

```http
POST /mx/v1/records/{uid}/attachments/fields/{field_uid}

DELETE /mx/v1/records/{record_uid}/attachments/{attachment_uid}

GET /mx/v1/records/{record_uid}/attachments/{attachment_uid}/preview
GET /mx/v1/records/{record_uid}/attachments/{attachment_uid}/download
POST /mx/v1/records/{record_uid}/attachments/{attachment_uid}/preview-ticket

GET /mx/v1/records/media/{short_lived_ticket}
```

The upload route includes the File Attachment field UID so a record can contain
multiple independent attachment fields. Preview tickets permit native browser
media requests without exposing the user's session token; the media route
streams N1 responses and preserves `Range`, `Content-Range`, `Accept-Ranges`,
length, cache validator, and content metadata.

### Revision tracking

```http
GET /mx/v1/status/revision
```

The lightweight revision endpoint remains useful for refresh/fallback logic.

### Collaboration and notifications

```http
GET  /mx/v1/collaboration/people
GET  /mx/v1/collaboration/channels
GET  /mx/v1/collaboration/calls
POST /mx/v1/collaboration/channels
POST /mx/v1/collaboration/direct
GET  /mx/v1/collaboration/channels/{channel_uid}/messages
POST /mx/v1/collaboration/channels/{channel_uid}/messages
GET  /mx/v1/collaboration/channels/{channel_uid}/members
POST /mx/v1/collaboration/channels/{channel_uid}/members
DELETE /mx/v1/collaboration/channels/{channel_uid}/members/{user_uid}
GET  /mx/v1/collaboration/channels/{channel_uid}/call
POST /mx/v1/collaboration/channels/{channel_uid}/call
PATCH /mx/v1/collaboration/channels/{channel_uid}/call
DELETE /mx/v1/collaboration/channels/{channel_uid}/call
POST /mx/v1/collaboration/channels/{channel_uid}/call/signal
PUT  /mx/v1/collaboration/messages/{message_uid}
DELETE /mx/v1/collaboration/messages/{message_uid}
PUT  /mx/v1/collaboration/messages/{message_uid}/pin
DELETE /mx/v1/collaboration/messages/{message_uid}/pin
POST /mx/v1/collaboration/messages/{message_uid}/files
POST /mx/v1/collaboration/files/{file_uid}/preview-ticket
GET  /mx/v1/collaboration/media/{short_lived_ticket}

GET  /mx/v1/notifications
POST /mx/v1/notifications/read-all
POST /mx/v1/notifications/{uid}/read
```

Messages, mentions, pins, record links, channel membership, unread state,
shared-file metadata, and notifications are durable SQLite state. Adding,
removing, or leaving a group produces a visible system event. Messages use
administrator-sized cursor pages (256 by default), and older history loads
automatically as the conversation scrolls upward. Enter sends a message while
Shift+Enter inserts a new line. Edits are marked, while deletion keeps an
immutable database row and exposes only a non-editable tombstone. Shared image
files render authenticated in-conversation thumbnails with ghost loading;
shared bytes remain in N1 and media previews support HTTP byte ranges. Scoped
WebSocket events provide immediate delivery without exposing one user's private
events to other sockets.

### Real-time synchronization and presence

```http
POST /mx/v1/live/ticket
GET  /mx/v1/presence

WebSocket /mx/v1/live
```

The live ticket is short-lived and is used to establish an authenticated
WebSocket without placing the long-lived JWT directly in the WebSocket URL.

The WebSocket endpoint supports the HTTP/1.1 upgrade path and HTTP/2 extended
CONNECT when the server negotiates HTTP/2.

### Audit and backups

```http
GET  /mx/v1/admin/audit

GET  /mx/v1/admin/backups
POST /mx/v1/admin/backups

POST /mx/v1/admin/backups/{uid}/verify
GET  /mx/v1/admin/backups/{uid}/download
```

### Database administration

```http
POST /mx/v1/db/query
```

The database query endpoint is read-only and cannot access authentication tables.
Credential recovery and security changes must use their dedicated re-authenticated
workflows.

Administrator-only endpoints require Administrator authorization on the backend.

---

## Configuration

MX uses:

```text
mx.config
mx.env
```

Example N1 configuration:

```ini
[n1]
base_url=https://127.0.0.1:50001
fragment=<MX_FRAGMENT_HASH>
insecure_tls=true
attachment_max_size_mb=50
collaboration_file_max_size_mb=100
multipart_part_size_mb=16
```

`attachment_max_size_mb` and `collaboration_file_max_size_mb` set the accepted
file limits. `multipart_part_size_mb` controls each part sent from MX to N1 and
must be between 5 and 100 MiB; it defaults to 16 MiB when omitted. The N1 server
must allow a part at least this large. Increasing an upload limit does not
require sending the complete object as one N1 part.

MX uses the N1 v4.0.0 `/noa/v1` fragment-scoped API. Storage layouts remain
logical object-key prefixes: uploading an object creates its virtual parent
prefixes automatically. MX does not call `posix/mkdir`, which N1 format 4
rejects. This applies to record attachments, collaboration files, profile
photos, notification sounds, deployment logos, and database backups.
Multipart initiation, raw part uploads, finalization, and byte-range media
streaming use the v4 contracts. Namespace mutations are checked for reported
per-key conflicts and failures even when N1 returns HTTP 200.

N1 storage format 4 does **not** automatically migrate older development
roots. Updating MX's API integration does not migrate existing attachment
bytes or change their stored object keys. Before switching an existing
deployment, follow N1's storage-upgrade guidance and verify that the configured
fragment and existing objects are available on the new instance.

Optional Drive configuration (defaults shown; existing configuration files do
not need this section to start):

```ini
[drive]
quota_mb=10240
public_links=true
```

`quota_mb` is the default per-account allowance in MiB; administrator assignments
override it per user. Obsolete `max_file_size_mb` values are ignored: Drive has
no per-file limit. Record/chat attachment limits remain separate and their Drive
attachment copies are still buffered within those limits. Setting
`public_links=false` immediately disables all guest listing/media routes.

N1 authentication requires `[n1].fragment` to contain the SHA-256 identifier and
`N1_MX_SECRET` to contain the raw secret. For older configurations that copied
the same raw fragment into both, MX derives its identifier in memory and logs a
credential-free configuration warning. Already-hashed identifiers are not
hashed again, and other credential failures do not trigger fallback attempts.


Example WebRTC configuration:

```ini
[webrtc]
stun_urls=stun:stun.example.net:3478
turn_urls=turn:turn.example.net:3478,turns:turn.example.net:5349
turn_username=mx
turn_credential=<TURN_CREDENTIAL>
max_participants=12
```

The entire `[webrtc]` section is optional. URL lists are comma-separated and the
participant limit must be from 2 through 12. Empty STUN/TURN values allow direct
host candidates for reachable local networks. TURN credentials are delivered
only to authenticated members requesting the call state, but they remain
deployment secrets and should be scoped and rotated at the TURN service.

N1 secret:

```env
N1_MX_SECRET=<MX_FRAGMENT_SECRET>
```

Do not commit deployment secrets to source control.

---

## Office Preview

MX previews supported files directly where possible.

```text
Images       -> browser image viewer
PDF          -> embedded PDF viewer
Text         -> text viewer
Audio        -> range-streamed browser audio player
Video        -> range-streamed browser video player
Office       -> LibreOffice -> PDF preview
Other files  -> download
```

On Debian, Office/OpenDocument preview requires LibreOffice components:

```bash
sudo apt update

sudo apt install -y \
  libreoffice-writer \
  libreoffice-calc \
  libreoffice-impress
```

Verify:

```bash
libreoffice --headless --version
```

LibreOffice is used as a headless conversion worker. The original N1 object is not replaced by the generated preview.

### Standalone Office editing

MX deployment must remain MX + SQLite + N1. No Collabora/ONLYOFFICE server,
Docker stack, database service, public Office API or runtime CDN is required by
the bundled editor. In Drive, use the Office action / **Edit in MX Office** on a
supported file. Viewers open a read-only editor. This opens a separate browser tab
running a locally bundled LibreOffice/ZetaOffice WebAssembly engine; MX owns
authorization, metadata and versions, while N1 stores the bytes.

The editor fills the window beneath a single 38-pixel MX bar: filename, compact
save status, Save and **⋯ Document options**. There are no additional status,
offline or information rows and no MX footer. Open from Drive, Download copy,
Save as a copy, offline settings and detailed save/certificate information are
in Document options. The menu and actionable error/recovery messages overlay
the editor rather than resizing it while you type. LibreOffice's own editing
menus, formatting controls and formula bar are retained.

Editing is offered for DOC/DOCX/ODT/RTF and XLS/XLSX/ODS, using explicit matching
export filters. PPT/PPTX/ODP open **read-only**: native presentation editing
failed acceptance tests and is not enabled in this build. Macro execution and automatic linked-data
updates are disabled; macro-enabled DOCM/XLSM/PPTM files are not offered for
editing. Format availability is not a guarantee of perfect fidelity for every
document. See `TESTING.md` for the formats and browser engines actually tested.
Current acceptance testing still catches intermittent missing leading input in
freshly opened spreadsheets and a Firefox native-canvas rendering failure.
Office editing is not release-qualified yet; keep separate copies of important
documents and do not confuse confirmed MX snapshot storage with validation of
every native editing interaction.

#### Offline operation

There are two independent cases:

- **No public internet, MX/N1 reachable on the LAN:** open, edit and save normally.
  Every engine/SDK/font asset is served from MX. Deployment needs only the Rust
  binary, the complete `frontend/dist` package, SQLite and N1; Node.js and a host
  LibreOffice installation are not needed for this editor.
- **MX/LAN itself unreachable:** previously prepared documents can reopen and
  remain editable. Visit Office once while connected, open **Document options**
  and wait for **Editor
  available offline**. Enable **Keep offline copy on this device** for each
  document, then bookmark `/office/index.html` for the offline document list.
  Unsaved snapshots are retained in IndexedDB after a short typing pause and
  periodically while editing. Reconnect, sign in if necessary, and explicitly
  choose **Save to MX**. Reconnection never silently uploads or overwrites files.

The editor-only service worker caches public application/engine assets, **not**
API responses, credentials or documents. Document retention is a separate,
explicit opt-in. Local copies are readable by anyone using the same browser
profile; do not enable this on shared/untrusted workstations. Offline copies
are not backups or cloud saves. Browser storage limits, eviction/private mode,
device failure or clearing site data can remove them. Download a separate copy
when needed. Changes not yet checkpointed can be lost in a process/power crash.
Removing a local copy does not delete MX versions, and revoking server access
cannot retract a copy already downloaded to a user's machine.

Office requires a secure context, WebAssembly threads/SharedArrayBuffer, worker
WebGL and
browser-trusted HTTPS. LAN deployments can use an internally trusted CA; a public
internet connection is not necessary. Merely accepting a self-signed certificate
warning may prevent service-worker installation. Preserve MX's isolation headers
through any reverse proxy. They apply only to `/office/`, not the workspace/calls.
Offline caching failures have a concise, expandable explanation in **Document
options**, technical
details and a retry action. They are separate from MX save errors: an expired or
untrusted certificate can prevent offline preparation even when the browser
lets you open MX after bypassing its certificate warning. Renew the certificate
and establish browser trust; bypassing the warning is not an offline-mode fix.
If the engine cannot
start, an already loaded document/local copy remains downloadable. A disposable
worker checks actual graphics support before the large runtime starts: merely
having `OffscreenCanvas` or main-page WebGL is not enough. The tested WebKit/WPE
runtime lacks worker WebGL; its download fallback is tested, not Office editing.
Chromium/Firefox editing results are listed separately in `TESTING.md`.

Each frontend build changes the offline worker's generation as well as its asset
manifest, so already-cached browsers can receive editor updates. Before closing
an old editor tab, save successfully to MX or download a copy of unsaved edits;
then reopen the document to use the updated interface.

#### Saving and concurrency

Use **Save to MX**, Ctrl/Cmd+S, or the native File → Save/Save toolbar action.
The compact bar shows **Unsaved → Saving… → Saved**, with success only after MX
acknowledges the commit. Hover its status or open **Document options** for
**Saved to MX · revision … · time** and detailed progress; the full status is
also announced to assistive technology. Unsaved/newer edits and failures stay marked
unsaved; a local draft or native temporary file is not proof of an MX save.
Saving an unchanged document explains that there are no unsaved changes rather
than creating another version. **Sign in to save** is a recovery action shown
only when authentication is missing/expired, not an extra Office account.
Recovery keeps the open edits and requires the original MX account.
Save also accepts the spreadsheet cell currently being typed, without requiring
Enter first. The native spreadsheet address box identifies the active cell; its
local-only accessible selection is also retained. It does not create server edit
sessions or presence tracking.

Native File → Open, the folder toolbar action and Ctrl/Cmd+O guide you to
**Open from Drive** in **Document options**. Each editor tab remains bound to its original MX file.
Opening a different local file through the engine's unsupported native picker
is not enabled; upload/open it through Drive in a separate editor tab. This also
prevents accidentally saving a different displayed document over the original.

`GET /mx/v1/drive/items/{uid}/office` returns an ACL-checked snapshot with the
current MX revision, exact version UID and `editing_supported` flag; the editor downloads that pinned
version. Editing stays entirely local, with no edit locks, edit-presence heartbeat
or server Office sessions. A read-only connection check shows an offline icon
without tracking edits. MX's existing resumable multipart upload publishes saves:

1. Export the document in its original format in the browser.
2. Upload it as a **new uniquely keyed N1 object**, preserving the original
   extension. Never replace an earlier Office version's content.
3. Atomically recheck access, owner quota and the original MX revision, insert a
   normal MX file version and switch the current-version pointer.

An uncertain save or separate-file save retains its exact upload operation/payload
for idempotent retry. With local retention enabled, this intent also survives
closing/reopening the tab. Save and Download queue behind an in-flight local
checkpoint; the user does not need to click a second time.
Concurrent whole-document edits do **not** automatically merge paragraphs/cells:
a stale save asks the user to save a separate file, deliberately add a version
after the latest version, or discard/load the latest. Each explicit replacement
is checked again; previous versions remain in history. Native Office Save and
Ctrl/Cmd+S route through MX, not just the engine's temporary filesystem.

#### Packaging and current limits

`frontend/public/office/vendor/manifest.json` pins executable/data checksums;
`npm run build` verifies them without downloading assets. Brotli engine assets
include approximately 51 MiB of Brotli binaries plus approximately 80 MiB of
generated gzip alternatives for browsers/connections that do not advertise
Brotli. The engine expands to approximately 250 MiB before additional runtime/
document memory. Large documents require the
client to load/export the full file: Drive's unlimited-per-file multipart policy
does not imply unlimited browser editing memory. Editor tabs consume client RAM;
small server hardware still needs to serve assets and handle storage transfers.

The pinned upstream engine assets are dated 2025-05-13. This integration is **not
a completed current-engine security/support or third-party distribution-license
audit**; those are release gates. Provenance, source links and license notices are
in `frontend/public/office/vendor/THIRD-PARTY.md`. Physical low-memory devices,
mobile ergonomics and complex/legacy-format fidelity need target-device testing.
Direct Office editing currently starts from Drive, not record/message attachment
modals. The older optional headless LibreOffice PDF preview remains separate,
read-only and dependent on a host installation; it is not needed by this editor.
The native engine toolbar is desktop-oriented. Narrow windows scroll an
initialized-width canvas instead of distorting its text and hit targets. The MX
bar stays one row, with keyboard-operable scroll buttons available in Document
options. This is not a mobile-native Office interface.

---

## Development

Current project location:

```text
/home/altear/Development/mx
```

Enter the project:

```bash
cd /home/altear/Development/mx
```

Format and check:

```bash
cargo fmt
cargo check
```

Strict warning-free check:

```bash
RUSTFLAGS="-D warnings" cargo check
```

Backend regression tests, including the N1 v4 HTTP-contract tests:

```bash
cargo test --locked
```

The opt-in N1 round-trip test requires a **disposable, isolated N1 v4 instance**
and a fragment token in `MX_TEST_N1_TOKEN`, with its base URL in
`MX_TEST_N1_URL`. It checks real one-shot/multipart uploads, downloads, valid
byte ranges, rename/trash/recovery, and concurrent writes across MX storage
prefixes. It writes unique test keys and soft-deletes them afterward; trash
remains until N1 retention/GC or disposal of the test storage. The test allows
self-signed TLS for this isolated instance. Do not point it at production.

```bash
cargo test --locked api::mx::n1::tests::real_n1_v4_round_trip -- --ignored
```

Install and validate the Svelte frontend with Node.js 24 or newer:

```bash
cd frontend
npm ci
npm run check
npm test
npm run build
cd ..
```

Cross-browser UI and network-fault regressions (local mocks; no production data
or N1 writes):

Browser tests generate audio and substantial CPU load. Run them on a dedicated
test machine/container, not while using the workstation for normal audio.
They now refuse to launch without `MX_TEST_AUDIO_SINK` naming a verified
PulseAudio/PipeWire `module-null-sink` output with an `mx_test_` prefix.
Only test browser processes use that sink and its monitor; Chromium is also
muted, and an ALSA-null configuration prevents hardware fallback. The guard does
not change system volume, defaults, or restart audio services. For example,
on an isolated Linux test host:

```bash
mx_test_audio_module_id=$(pactl load-module module-null-sink sink_name=mx_test_silent)
# Prefix browser test commands below with MX_TEST_AUDIO_SINK=mx_test_silent.
```

```bash
cd frontend
npx playwright install --with-deps chromium firefox webkit
MX_TEST_AUDIO_SINK=mx_test_silent npm run test:network:browser
```

The suite opens the real Svelte record editor/table, chat composer, and startup UI, and uses
native browser WebRTC with synthetic camera/microphone/screen sources. It checks
retained create identifiers after lost responses, menu visibility/hit testing in
light/dark and narrow layouts, inline-edit revisions after refresh, chat retries
after lost responses/file rejection, native file-body cancellation, dropped
offers/answers/ICE, immediate rejoin, and mixed-engine three-person calls with
signaling jitter. Media assertions inspect received RTP and changing pixels in
the actual video elements. Artifacts and screenshots are saved to a temporary
directory printed by the runner. `MX_TEST_BROWSERS=firefox` selects one engine;
`MX_CHROMIUM_EXECUTABLE`, `MX_FIREFOX_EXECUTABLE`, and `MX_WEBKIT_EXECUTABLE`
allow custom engine paths.
Set `MX_TEST_UI_ONLY=1` to run the UI/file scenarios without media groups.
`MX_TEST_DELETION_ONLY=1` exercises immediate row removal, stale refresh/module
responses, explicit restores, out-of-order events, missed restores during an
outage, and restarted server event counters in each selected browser.

Eight-account real-server validation (build the backend first):

```bash
cargo build --locked
cd frontend
MX_TEST_AUDIO_SINK=mx_test_silent npm run test:eight-users:browser
```

This runner starts its own MX HTTPS server, SQLite database, generated credentials,
and N1 v4 HTTP contract fixture. It never loads deployment `mx.env`/`mx.config` or
modifies the N1 repository. It checks eight distinct authenticated editors,
atomic unrelated-field merges, same-field conflicts, retried creates/messages,
deleted records during module updates, stale drafts after restore, cleared cells,
archived fields, eight concurrent 18-MiB multipart uploads with injected 503s,
byte ranges, and independent record attachment additions. Its browser phase
uses the production call UI and real HTTP/WebSocket signaling with synthetic
capture: eight participants, 56 directed peer connections, every sender's screen
and camera on every viewer, audio, leave/rejoin, same-account session replacement,
device controls, fullscreen, reconnect, and final call cleanup. A failed assertion
keeps diagnostics and marks the run failed; connection state alone is not proof
of working media.

`MX_TEST_API_ONLY=1` skips the browser phase. `MX_TEST_BROWSERS=chromium,firefox`
selects that engine mix; omitted engines are **not** validated by that run.
`MX_TEST_ARTIFACTS` selects the parent artifact directory and
`MX_TEST_SOAK_SECONDS` changes the media soak duration (default 15 seconds).
Generated artifact directories contain test credentials/private keys and should
not be published. To run the real-server suites against disposable live N1,
set `MX_TEST_N1_URL` and `MX_TEST_N1_CREDENTIALS_FILE`. The private credentials
file contains `N1_MX_SECRET` and either `N1_FRAGMENT` (raw) or `N1_FRAGMENT_HASH`.
The adapter never reads the deployment's `mx.env`; it injects transient part
failures and cleans up only unique test objects by N1 soft deletion.

MX Drive production UI/API tests include eight accounts, durable resume across
MX restart and browser module switches, no file pre-scan/read-back, visible
MB/s/ETA, pause/resume, lost finish/batch acknowledgements, guest ACL/range checks,
folder uploads, and independent copies into collaboration/record attachments:

```bash
MX_TEST_N1_URL=https://127.0.0.1:50001 \
MX_TEST_N1_CREDENTIALS_FILE=/private/path/n1-test.env \
MX_TEST_N1_DROP_FINALIZE=1 \
MX_TEST_AUDIO_SINK=mx_test_silent \
npm run test:drive:browser
```

Run from `frontend` after `cargo build --locked` and `npm run build`.
Without the live-N1 variables the runner uses an explicit HTTP contract fixture,
not live storage. `MX_TEST_API_ONLY=1` skips browser checks; no UI validation is
claimed for that mode. See [TESTING.md](TESTING.md) for retained results and gaps.

The native relationship runner tests eight-account merges/conflicts, referential
integrity, paged selectors and three-engine form/inline/admin UI:
`npm run test:relationships:browser` (with the same isolated audio sink).

`npm run test:office:browser` runs the **real bundled engine**, keyboard edits,
exported-document inspection, new-object version saves, offline tab closure and
reopening, lost-response reconciliation and simultaneous-save conflicts. Public
internet requests are blocked throughout the browser phase. This suite needs no
audio devices/sink and never changes system certificate trust. It uses a test-only
localhost proxy for the isolated self-signed MX server. The default N1 backend is
the explicit contract fixture; opt-in live N1 variables retain the same meaning.
`MX_TEST_BROWSERS` selects the browser engines. Focused diagnostic runs using
`MX_TEST_OFFICE_FORMATS_ONLY=1` are not the complete offline/concurrency UI suite.
`MX_TEST_OFFICE_SHELL_ONLY=1` checks the compact layout, keyboard/menu actions,
real DOCX save/download, opt-in offline retention and fixed canvas geometry;
it does not clear the existing spreadsheet input/rendering release gates.
Use `MX_TEST_BROWSERS=chromium,firefox` for the currently validated editing engines.
The separate `MX_TEST_BROWSERS=webkit MX_TEST_OFFICE_GRAPHICS_FALLBACK=1` run
verifies the unsupported-runtime message and unchanged download, **not** WebKit
editing. Presentation checks assert the read-only gate, not a successful edit.
The suite's result file explicitly records these limits and does not claim
production qualification.

`npm run test:records:scroll` exercises wide/tall real record tables in Chromium,
Firefox and WebKit: pointer/keyboard scrolling, both-way synchronization,
resizing/themes, normal-scrollbar handoff, column visibility, dialogs, mobile
cards, pagination and cleanup when navigating away. It uses disposable MX data
and the explicit N1 contract fixture by default, without audio devices.

`npm run test:drive:dropdowns` checks dropdown arrows on dashboard, module settings,
report builder, account creation and Drive. It covers explicit light/dark and
system-light/system-dark themes at desktop/mobile widths; Reporting period
keyboard input and persisted Navigation icon changes are checked alongside
user/group access roles, every guest-link expiry, committed ACLs/timestamps,
all sort choices and 25/50/100-item pages. Computed-style checks verify one
non-repeating chevron inset 12px with reserved text space, plus the native-arrow
fallback in high-contrast mode. Shared-style unit checks reject legacy theme
overlays and guard the shared decoration against component background overrides.
It also checks that dismissing a dropdown with Escape keeps Share open, while
Escape from another dialog control still closes Share. The three-engine runner
uses real isolated MX/SQLite, light/dark desktop/mobile layouts and process-local
null audio backends; it does not require or change a desktop audio sink.

For focused pagination/share/preview and file-manager regressions, use
`npm run test:drive:polish` and `npm run test:drive:file-manager` with the same
explicit test-only null sink. The polish runner seeds 135 isolated metadata items
per browser over an HTTP-uploaded object, exercises page boundaries and group
membership, and saves desktop/mobile screenshots. It is not a live N1 test unless
the opt-in live credentials authenticate successfully.

After all test browsers have exited, remove only the null sink created for them:

```bash
pactl unload-module "$mx_test_audio_module_id"
```

This does not certify physical devices, Safari/iOS, browser capture-picker audio,
TURN deployments, or real UDP packet loss. Exercise those on the target devices
and network before production rollout. The separate Chromium capture/fullscreen
regression remains available with `npm run test:calls:browser`.

Validation caveat (2026-10-08): repeated mixed-engine stress testing on the
development host intermittently crashed the Linux Playwright WebKit page;
subsequent reruns passed. The crash's root cause remains unconfirmed. Keep failed
artifacts and do not treat a later green run as production/Safari certification.
Earlier runs used Node.js 20.19.2. The subsequent checks, production build,
cross-browser regression suites, and live N1 tests used Node.js 24.21.0.

Run MX with the production frontend from `frontend/dist`:

```bash
cargo run
```

For browser-interface development, run `npm run dev` inside `frontend`. Vite
proxies `/mx` and `/ping` to the Rust HTTPS server. Svelte is a build-time
dependency; production does not require a Node.js process.

Production build:

```bash
cd frontend && npm ci && npm run build && cd ..
cargo build --release
```

Run the production binary:

```bash
./target/release/litiaina-mx
```

---

## Project Structure

```text
mx/
├── CHANGELOG.md
├── Cargo.toml
├── mx.config
├── mx.env
├── frontend/
│   ├── src/                 # Svelte 5 and TypeScript application
│   ├── public/              # Static frontend assets
│   ├── dist/                # Shipped production frontend served by Rust
│   ├── package.json
│   └── vite.config.ts
│
└── src/
    ├── api/
    │   ├── admin/
    │   ├── mx/
    │   │   ├── attachment_fields.rs
    │   │   ├── formula.rs
    │   │   ├── handler.rs
    │   │   ├── migration.rs
    │   │   ├── model.rs
    │   │   ├── n1.rs
    │   │   ├── n1_tests.rs
    │   │   ├── records.rs
    │   │   ├── schema.rs
    │   │   └── storage.rs
    │   │
    │   ├── user/
    │   ├── audit.rs
    │   ├── backup.rs
    │   ├── calls.rs
    │   ├── collaboration.rs
    │   ├── dashboard.rs
    │   ├── lifecycle.rs
    │   ├── live.rs
    │   ├── modules.rs
    │   ├── notifications.rs
    │   ├── preferences.rs
    │   ├── reports.rs
    │   ├── query_handler.rs
    │   └── route.rs
    │
    ├── config/
    ├── db/
    ├── middleware/
    ├── macros/
    ├── util/
    └── main.rs
```

The main responsibilities are intentionally separated:

```text
schema.rs             dynamic Record Structure
modules.rs            module identities and access matrices
records.rs            dynamic values, search, sorting, pagination
attachment_fields.rs  File Attachment field behavior and metadata
storage.rs            configurable and frozen N1 namespaces
handler.rs            record/attachment operations and N1 coordination
n1.rs                 N1 v4 object-key, multipart, and streaming transport
reports.rs            statistics, dashboard reporting, CSV export
dashboard.rs          dashboard support
live.rs               WebSocket sessions, live events, and presence
calls.rs              authorized WebRTC room state and private signaling
collaboration.rs      channels, messages, N1 file shares, and record links
notifications.rs      durable per-user notification delivery
preferences.rs        per-user appearance and notification overrides
lifecycle.rs          record versions, soft delete, Trash, and recovery
audit.rs              accountability history
backup.rs             verified SQLite snapshots stored in N1
```

---

## ARIS to MX Migration

MX is the canonical product name.

The active API namespace is:

```text
/mx/v1
```

The Rust application module is:

```text
crate::api::mx
```

The Cargo package/binary is:

```text
litiaina-mx
```

The current repository directory is:

```text
/home/altear/Development/mx
```

Existing deployments upgraded from ARIS can migrate legacy internal SQLite objects to `mx_*`.

Existing database filenames and existing N1 attachment object keys do not need to be physically moved only for branding. Keeping durable object locations stable avoids unnecessary data movement and migration risk.

New MX backup objects use the `__mx` namespace.

---

## Design Principles

MX follows these principles:

- start with a blank business schema;
- make the deployment define the information model;
- treat File Attachment as a normal configurable field type;
- keep business fields out of hard-coded Rust structs;
- use stable internal record and field identities;
- enforce validation and authorization on the server;
- allocate automatic numbers transactionally;
- perform search, filtering, pagination, and reporting server-side;
- keep large binary data out of SQLite;
- use N1 as authoritative object storage for attachment bytes;
- keep storage namespaces deterministic and stable;
- allow dashboard/statistics behavior to be built from the same schema;
- keep operational audit history separate from business completion semantics;
- verify database backups before treating them as valid;
- keep the WebSocket layer as live invalidation/notification rather than durable state;
- reconnect live clients indefinitely and resynchronize after outages;
- allow parallel record work without whole-record locks;
- protect same-field concurrent edits with a record revision and per-field
  last-change metadata, without edit locks or collaboration state;
- avoid recompilation when a deployment changes its business record structure.

---


## MX 4.1.0

MX 4.1 adds native module relationships, lookups and rollups, plus locally
bundled, client-side Office editing in Drive. MX owns access and versioning;
each saved file version is a new immutable N1 object. Prepared, opted-in local
copies can reopen without MX connectivity; normal LAN use needs no Internet
or separately installed Office server. The Office shell is one compact bar,
and wide record tables keep horizontal scrolling within reach.

Dropdown arrows remain visible throughout MX, including Drive access
roles, guest-link expiry, sorting and page size. A single chevron sits 12px
inside the border with space reserved beside the selected text; high-contrast
mode uses the browser's native arrow. These remain real HTML dropdowns:
click/tap to open, or focus and use the browser's arrow-key selection controls.
Fixed labels such as the file's Owner role are not editable dropdowns.

The 4.1.0 version is not production certification. Office spreadsheet input and
Firefox rendering have unresolved acceptance failures; presentation files are
read-only and the tested WebKit/WPE editor uses a download fallback. Full call,
live-storage, security, complex-document and real-device qualification gates
remain documented in [TESTING.md](TESTING.md). Do not infer clearance from a
successful build or a version bump.

### Upgrade and validation

Rust and frontend package versions are both **4.1.0**. Deploy the matching binary
and rebuilt `frontend/dist` together, then restart MX so the new routes and
automatic SQLite schema migrations are active. Back up SQLite first; do not
replace the existing database, environment, or deployment configuration.
Compatibility remains **N1 v4.0.0 / storage format 4**; do not change N1's version
to match MX. Office offline reopening requires browser-trusted HTTPS and
sufficient client storage; a certificate-warning bypass does not qualify it.

API clients must send `base_revision` for full-record PUT, use durable multipart
instead of the retired Drive one-request upload endpoint, and honor returned
page limits/offsets rather than hard-coding 100-item pages.

### MX 4.0.0 baseline

MX 4.0 expands the product into a **self-hosted modular digital workplace
platform**. MX Drive joins the existing configurable records, dashboards,
collaboration spaces, and voice/video/screen sharing in the same workspace.
The release also brings N1 v4 integration, network recovery, safer record
lifecycle/concurrency behavior, and call-media refinements.

The 4.0 release includes:

- personal/shared Drive with folders, search, stars, trash, versions and activity;
- 25/50/100-item server-side pages, stable sorting and cross-page previews;
- user/space/group sharing and expiring/revocable read-only guest links;
- administrator-assigned storage allowances with a 10-GiB starting default;
- metadata-first, resumable/batched uploads without a Drive per-file size cap;
- a workspace-wide upload queue with circular progress, MB/s, ETA and pause/resume;
- checkbox, Ctrl/Cmd/Shift and rectangle selection, copy/cut/paste and file/folder drops;
- MX-owned metadata over immutable original-extension N1 objects, metadata-only
  Drive copies/version restoration, and durable background soft-delete cleanup;
- bounded network deadlines, replay-safe creates and reconnect reconciliation;
- stale-list/lifecycle protections that keep deleted records from reappearing;
- call transport recovery, independent microphone/shared audio and stable video presentation.

Schema-driven modules, formulas, inline editing, stateless optimistic record
merges, module isolation, reporting, attachment previews and team calls remain
part of the platform. See [CHANGELOG.md](CHANGELOG.md) for the 4.0 changes and
the historical 3.0 release notes.

Existing saved deployment branding is preserved. New defaults use “Litiaina's
Digital Workplace Platform”; **Restore MX defaults** in Deployment identity
changes the editor only until the administrator saves it.

Version 4.1.0 is not a blanket reliability certification. The known Linux WebKit
large-upload crash, incomplete eight-person call validation, and current live-N1
test-fragment blocker remain documented in [TESTING.md](TESTING.md).

---

## Philosophy

**MX is a configurable digital workplace, not a fixed business application.**

The platform provides shared mechanisms for managing information, communicating,
meeting, and storing/sharing files. Its record engine can store, search, secure,
audit, visualize, and preserve deployment-defined information.

The organization decides what that information means.

```text
Start blank.

Define the Record Structure.
Define File Attachment fields.
Define the N1 layout.
Define the dashboard.
Define the statistics.
Define the reports.

Connect people through collaboration spaces and calls.
Organize and share files through MX Drive.

MX becomes the digital workplace the deployment needs.
```

That boundary allows the same MX core to serve very different organizations and workflows without rewriting the application for every deployment.
