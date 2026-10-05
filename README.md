<div align="center">

  <img
    src="./assets/litiaina_icon.png"
    alt="Litiaina"
    width="150"
    height="150"
  />

  <h1>Litiaina MX</h1>

  <p>
    <b>Self-hosted, fully customizable general-purpose information system</b><br>
    Build the record structure, attachments, storage layout, dashboard, statistics,
    and reporting model that each deployment actually needs.
  </p>

</div>

<p align="center">
  <strong>Current release: MX 3.0.0</strong>
</p>

---

## Overview

**MX** is a schema-driven information-system platform built for deployments that should not be locked to one predefined record format.

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
   │                backup metadata
   │
   └──────────────► N1
                    attachment bytes
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
- stateless optimistic record concurrency.

### SQLite

SQLite stores structured and queryable state.

Binary attachment data is deliberately kept out of SQLite.

### N1

N1 stores:

- original uploaded attachment bytes;
- attachment versions;
- collaboration file shares;
- the uploaded deployment logo;
- verified SQLite backup objects.

The N1 object is authoritative for file content. Preview files are derived data and may be regenerated.

---


## Real-Time Collaboration

MX 3.0.0 includes authenticated real-time synchronization for multi-user deployments.

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
video source changes, MX prepares the replacement in a second video layer and
crossfades after a frame is available. The previous frame stays visible while
the replacement starts, reducing black flashes during camera/screen switching.
The device-panel video preview also avoids restarting for microphone-only changes.

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
Date
Boolean
Select
Auto Number
File Attachment
```

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

Install and validate the Svelte frontend with Node.js 24 or newer:

```bash
cd frontend
npm ci
npm run check
npm test
npm run build
cd ..
```

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


## MX 3.0.0

MX 3.0 is a major expansion of the schema-driven MX platform, adding isolated
per-account module access, stateless multi-user record concurrency, calculated
data, high-volume media handling, and integrated collaboration calls.

The 3.0 release includes:

```text
Formula / Calculated record fields with server-side evaluation
inline table editing for every editable field type
deliberate view and edit modes in the record dialog
attachment thumbnails and keyboard-accessible file navigation
short-lived preview tickets and N1 byte-range streaming
configurable multipart N1 uploads for large objects
Enter-to-send collaboration messaging and durable membership events
joinable voice/video calls with camera, microphone, screen, and shared audio
record and attachment notification routing with duplicate suppression
immediate audit lifecycle repair for accurate actor performance reporting
non-destructive live table refreshes that retain the current scroll position
exact 10-24 px per-user font sizing independent of interface scale
responsive record, preview, notification, and administration layouts
default-deny per-account module isolation with role capability ceilings
stateless record revisions with automatic unrelated-field merging
same-field stored/proposed conflict resolution without editing locks
attachment concurrency tied to normal record revisions and history
robust private/group voice, video, screen sharing, and movable call controls
```

Existing SQLite databases are upgraded in place by MX. Existing deployments may
omit `multipart_part_size_mb` to use the 16 MiB default, but MX must be restarted
after the new binary and frontend bundle are deployed so the 3.0 routes and
schema migrations are active.

Cargo package version:

```text
3.0.0
```

Product/release name:

```text
MX 3.0.0
```

See [CHANGELOG.md](CHANGELOG.md) for the complete release notes. Fixes following
the released MX 3.0.0 are listed under **Unreleased**, including the camera and
screen-share playback improvements. MX remains a schema-driven, self-hosted
general-purpose information system.

---

## Philosophy

**MX is an information-system engine, not a predefined information system.**

The platform provides the mechanisms required to store, search, secure, audit, visualize, and preserve information.

The organization decides what that information means.

```text
Start blank.

Define the Record Structure.
Define File Attachment fields.
Define the N1 layout.
Define the dashboard.
Define the statistics.
Define the reports.

MX becomes the information system the deployment needs.
```

That boundary allows the same MX core to serve very different organizations and workflows without rewriting the application for every deployment.
