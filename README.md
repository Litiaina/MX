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
  <strong>Current release: MX 1.0.0</strong>
</p>

---

## Overview

**MX** is a schema-driven information-system platform built for deployments that should not be locked to one predefined record format.

A fresh MX deployment starts with **no business fields and no predefined dashboard statistics**. An Administrator defines the information model through the application itself. Forms, record tables, search, attachment handling, N1 storage paths, dashboard widgets, statistics, and exports adapt to that configuration.

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
├── fields
├── field order
├── validation rules
├── file-attachment fields
├── N1 folder layout
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

The browser provides the unified MX interface:

- Dashboard
- Records
- Administration
- attachment preview
- search and filtering
- account management
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
- field-level concurrency checks.

### SQLite

SQLite stores structured and queryable state.

Binary attachment data is deliberately kept out of SQLite.

### N1

N1 stores:

- original uploaded attachment bytes;
- attachment versions;
- verified SQLite backup objects.

The N1 object is authoritative for file content. Preview files are derived data and may be regenerated.

---


## Real-Time Collaboration

MX 1.0 includes authenticated real-time synchronization for multi-user deployments.

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

Record editing uses partial field updates and field-level revision checks. A
conflict is raised only when two users concurrently modify the same logical
field from incompatible base revisions.

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

## Fully Dynamic Record Structure

The **Record Structure** is the source of truth for the business model of a deployment.

A fresh installation can contain zero fields.

Administrators add only what that deployment requires.

### Supported field types

```text
Text
Long Text
Integer
Decimal
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

## Dynamic Search

Search is generated from the active Record Structure rather than hard-coded business columns.

### Universal Search

Universal Search can search active fields marked `Searchable` and attachment filenames.

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

Administrators can select Record Structure fields to build the base N1 hierarchy.

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

Existing N1 object keys are not silently moved just because a label or global layout changes.

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
Record Structure
N1 Storage
Dashboard
Backups
Audit Log
```

### Accounts

Manage users and access levels.

### Record Structure

Build and maintain the deployment's dynamic schema.

### N1 Storage

Choose the dynamic fields used in the storage hierarchy and optional filename-prefix behavior.

### Dashboard

Create and maintain dashboard/statistics widgets.

### Backups

Create, verify, list, and download SQLite backup snapshots stored in N1.

### Audit Log

Review authenticated system activity.

These configuration areas are intended for Administrators.

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
| Administrator | Full record access plus accounts, schema, storage layout, dashboard configuration, database administration, audit, and backups |
| Manager | Read, create, edit, upload, download, and delete records/attachments |
| Editor | Read, create, edit, upload, and download; no delete |
| Viewer | Read, search, preview, and download |

Authorization is enforced by the backend rather than only by hiding interface controls.

---

## Authentication

MX supports:

```text
JWT access tokens
Refresh tokens
Optional TOTP two-factor authentication
First-run Administrator bootstrap
```

A fresh deployment can create its first Administrator only while the user table is empty and the bootstrap authorization requirement is satisfied.

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
account.2fa.enable
account.2fa.disable

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
mx_schema_meta
mx_fields
mx_record_values
mx_unique_values
mx_field_sequences
mx_attachments
mx_record_storage
mx_storage_layout
mx_dashboard_config
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

POST /mx/v1/auth/enable_2fa
POST /mx/v1/auth/disable_2fa
POST /mx/v1/auth/check_2fa

POST   /mx/v1/user/create
PATCH  /mx/v1/user/modify
DELETE /mx/v1/user/delete
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
```

The upload route includes the File Attachment field UID so a record can contain multiple independent attachment fields.

### Revision tracking

```http
GET /mx/v1/status/revision
```

The lightweight revision endpoint remains useful for refresh/fallback logic.

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
```

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
Audio        -> browser audio player
Video        -> browser video player
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

Run development build:

```bash
cargo run
```

Production build:

```bash
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
├── Cargo.toml
├── mx.config
├── mx.env
├── web/
│   └── index.html
│
└── src/
    ├── api/
    │   ├── admin/
    │   ├── mx/
    │   │   ├── attachment_fields.rs
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
    │   ├── dashboard.rs
    │   ├── live.rs
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
records.rs            dynamic values, search, sorting, pagination
attachment_fields.rs  File Attachment field behavior and metadata
storage.rs            configurable and frozen N1 namespaces
handler.rs            record/attachment operations and N1 coordination
reports.rs            statistics, dashboard reporting, CSV export
dashboard.rs          dashboard support
live.rs               WebSocket sessions, live events, and presence
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
- protect same-field concurrent edits with field-level revision checks;
- avoid recompilation when a deployment changes its business record structure.

---


## MX 1.0

MX 1.0 is the first public stable release of the MX general-purpose information
system platform.

The 1.0 line establishes the current platform contract around:

```text
dynamic Record Structure
File Attachment fields
configurable N1 storage layout
Universal and Advanced Search
server-side pagination and filtering
custom dashboard/statistics builder
CSV reporting and exports
deployment Appearance & Identity
role-based access
audit logging
verified SQLite backups to N1
real-time WebSocket synchronization
online/offline account presence
parallel field-level collaboration
responsive light/dark interface
```

Cargo package version:

```text
1.0.0
```

Product/release name:

```text
MX 1.0
```

Future additions can evolve the platform without redefining the 1.0 identity:
MX remains a schema-driven, self-hosted general-purpose information system.

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