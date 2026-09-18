<div align="center">

  <img
    src="https://github.com/Litiaina/litiaina-admin-webpage/blob/master/images/litiaina_icon.png?raw=true"
    alt="Litiaina Icon"
    width="150"
    height="150"
  />

  <h1>Litiaina ARIS</h1>

  <p>
    <b>Atomic Record Information System</b><br>
    A dynamic records platform for building structured, searchable, auditable information systems
    without hard-coding one organization’s record format into the server.
  </p>

</div>

---

## Overview

ARIS is a reusable information-system core for organizations that need more than fixed forms or simple file storage.

Instead of defining one permanent record structure in Rust, ARIS stores the record schema as data. Administrators can define the fields that make sense for a deployment, reorder them, control which fields are searchable or sortable, and choose how records are represented in the interface.

A record can represent a document transaction, procurement request, incident report, equipment record, referral, workflow request, inventory item, correspondence item, or another domain-specific business object.

The application remains the same while the record structure changes.

```text
ARIS
├── Dynamic Record Schema
│   ├── user-defined fields
│   ├── field types
│   ├── validation rules
│   ├── searchable/sortable flags
│   ├── table visibility
│   └── field ordering
│
├── SQLite
│   ├── record identities
│   ├── dynamic field values
│   ├── uniqueness indexes
│   ├── auto-number sequences
│   ├── users and permissions
│   ├── attachment metadata
│   ├── frozen storage namespaces
│   ├── audit history
│   └── backup metadata
│
└── N1
    ├── original attachment bytes
    └── verified database backups
```

SQLite handles structured information and transactional state. N1 handles durable binary objects.

## Why Atomic

ARIS treats each record as a stable atomic business object with its own identity, values, attachments, audit history, and storage namespace.

The record format is configurable, but the internal system model remains stable.

This separation allows ARIS to support different organizational workflows without rebuilding the storage, authentication, search, attachment, audit, and backup layers for every deployment.

## Dynamic Record Structure

A fresh deployment does not require a predefined business schema.

Administrators create the fields required by the organization from the Administration interface.

Supported field types include:

```text
Text
Long Text
Integer
Decimal
Date
Boolean
Select
Auto Number
```

Each field may define behavior such as:

```text
Required
Unique
Searchable
Sortable
Default table column
Table priority
Field order
Default value
Type-specific configuration
```

Fields have stable internal UIDs. Their labels, ordering, and presentation may change without changing the identity used by stored records.

Archived fields are retained instead of being destructively removed from historical data.

### Auto Number

`Auto Number` is a dynamic field type rather than a hard-coded control-number property.

A deployment may use one or more auto-number fields, or none at all.

Sequences are allocated transactionally by the server and can be configured independently from other fields.

This allows deployments to define identifiers such as:

```text
000001
2026-000001
REQ-000001
INC-2026-000042
```

without requiring ARIS itself to understand the organization-specific meaning of the number.

## Record Model

ARIS keeps a small stable internal record identity and stores business values separately.

Conceptually:

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
└── Attachments
    ├── UID
    ├── File Name
    ├── MIME Type
    ├── Size
    ├── N1 Object Key
    └── N1 Version ID
```

There is no required hard-coded `Office`, `Subject`, `Date`, `Requestor`, `Routing`, or `Control Number` field in the engine.

Those concepts are created only when a deployment needs them.

## Built-in System Fields

Some capabilities belong to ARIS itself rather than to the user-defined business schema.

`Attached Files` is currently a built-in system field.

Attachments are therefore always available to records without pretending that uploaded files are ordinary text or numeric values.

The built-in field can participate in table-column ordering and visibility, but it cannot be archived or removed like a custom field.

## Field Ordering

Record Structure supports drag-and-drop ordering.

Administrators can reorder custom fields and built-in fields without manually editing numeric positions.

ARIS normalizes positions internally:

```text
0
10
20
30
40
...
```

The order is saved transactionally and becomes the source for generated forms and record-table presentation.

New fields append to the end of the current structure.

## Search

Search is generated from the active dynamic schema.

### Universal Search

Universal Search searches all active fields marked `Searchable`.

It also searches attachment filenames.

The server supports:

```text
Contains
Starts with
Exact
```

Search safely escapes SQL `LIKE` metacharacters so literal `%`, `_`, and `!` values can be queried correctly.

### Advanced Search

Advanced Search operates on dynamic field UIDs rather than hard-coded column names.

It supports combinations of:

```text
field filters
match mode
attachment presence
sortable fields
sort direction
pagination
```

Fields not marked `Searchable` are excluded from search filters.

Fields not marked `Sortable` cannot be used as sort keys.

Records are paginated by the backend instead of loading the complete database into the browser.

## Storage

ARIS uses a normalized SQLite schema for the dynamic record engine.

Important tables include:

```text
aris_records
aris_schema_meta
aris_fields
aris_system_fields
aris_record_values
aris_unique_values
aris_field_sequences
aris_attachments
aris_record_storage
aris_audit_log
aris_backups
```

### Dynamic Values

Business values are stored separately from record identity.

The schema determines how a value is interpreted and validated.

This allows new fields to be introduced without adding new SQLite columns or changing the Rust record struct.

### Unique Values

Fields marked `Unique` are enforced through dedicated uniqueness state rather than relying on UI validation.

### Field Sequences

Auto-number fields maintain server-side sequence state in `aris_field_sequences`.

The client does not allocate authoritative sequence values.

## Configurable N1 Storage Layout

Attachment storage is also schema-aware.

Administrators can choose dynamic fields to build the visible N1 folder hierarchy and may optionally select a field as the filename prefix.

Example configuration:

```text
Folder 1       OFFICE
Folder 2       DATE
Filename prefix CONTROL NO
```

may produce:

```text
records/
└── MISO/
    └── 2026-09-18/
        └── 000125__purchase_request.docx
```

Another deployment can choose an entirely different layout without changing the backend.

The storage layout references field UIDs, not hard-coded names.

### Frozen Record Namespace

The resolved N1 namespace is frozen per record when storage is first established.

This prevents later edits to fields such as Office or Date from splitting one record's attachments across multiple paths.

Conceptually:

```text
Global storage layout
        ↓
Resolve dynamic field values
        ↓
Freeze record namespace
        ↓
Store attachment objects
```

Changing the global storage layout affects records whose storage namespace has not yet been established.

If a required storage-path value is missing, ARIS rejects the upload instead of creating an ambiguous path.

Existing attachment object keys remain valid and are not silently migrated.

## Attachments

Attachment bytes are stored in N1 rather than inside SQLite.

SQLite stores only the attachment metadata required to associate an object with its record.

The original object stored in N1 remains authoritative.

Derived previews may be regenerated.

## Attachment Preview

ARIS previews supported files directly where possible:

```text
Images       -> browser image viewer
PDF          -> embedded PDF viewer
Text         -> text viewer
Audio        -> browser audio player
Video        -> browser video player
Office       -> LibreOffice -> PDF preview
Other files  -> download
```

Office preview supports common Word, Excel, PowerPoint, and OpenDocument formats while keeping the original uploaded object unchanged in N1.

## Access Levels

```text
0 = Administrator
1 = Manager
2 = Editor
3 = Viewer
```

| Role | Capabilities |
|---|---|
| Administrator | Full record access, account management, record-structure management, storage-layout management, database administration, audit, and backups |
| Manager | Read, create, edit, upload, download, and delete |
| Editor | Read, create, edit, upload, and download; no delete |
| Viewer | Read, search, preview, and download only |

Authorization is enforced by the backend, not only by the interface.

## Authentication

ARIS supports:

```text
JWT access tokens
Refresh tokens
Optional TOTP two-factor authentication
First-run administrator bootstrap
```

A fresh deployment may create its first Administrator through the bootstrap flow while the user table is empty.

## Audit Logging

ARIS records meaningful authenticated actions for administrator review.

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

Passwords, JWTs, authentication keys, request bodies, uploaded file bytes, and SQL text are not copied into the audit log.

The audit log is append-only at the database level.

## Database Backups

Administrators can create a live SQLite backup from the Administration interface.

Backup flow:

```text
SQLite
  ↓
VACUUM INTO
  ↓
standalone .db snapshot
  ↓
PRAGMA integrity_check
  ↓
N1
```

Verified backups are stored under:

```text
__aris/backups/database/YYYY/MM/DD/
```

Example:

```text
__aris/backups/database/2026/09/18/aris-20260918T020000Z.db
```

ARIS can download a stored backup and re-verify it by opening the downloaded database and running:

```sql
PRAGMA integrity_check;
```

### Recovery

Database recovery is intentionally an offline administrative operation.

Stop the service first:

```bash
sudo systemctl stop litiaina-aris
```

Preserve the current database files:

```bash
mkdir -p recovery-before-restore

cp -a aris.db recovery-before-restore/ 2>/dev/null || true
cp -a aris.db-wal recovery-before-restore/ 2>/dev/null || true
cp -a aris.db-shm recovery-before-restore/ 2>/dev/null || true
```

Verify the selected backup:

```bash
sqlite3 aris-backup.db "PRAGMA integrity_check;"
```

Expected result:

```text
ok
```

Restore it:

```bash
cp aris-backup.db aris.db
rm -f aris.db-wal aris.db-shm
```

Start the service again:

```bash
sudo systemctl start litiaina-aris
```

Do not replace the database or remove WAL/SHM files while ARIS is running.

## Dashboard

The Dashboard uses dedicated summary endpoints rather than loading the full records database.

ARIS also exposes a lightweight revision endpoint so the interface can determine when record data has changed and refresh only when necessary.

This keeps dashboard retrieval independent from the number of stored records.

## Unified Interface

`aris.html` contains the application workspaces in one shell:

```text
Dashboard
Records
Administration
```

The interface includes:

```text
dynamic forms
dynamic record columns
horizontal table scrolling
local display scaling
persistent light/dark theme
persistent column preferences
record-structure management
N1 storage-layout management
account administration
audit review
database backups
```

The frontend uses the current page origin for API requests when served by ARIS:

```javascript
const API_BASE =
  window.location.protocol === "file:"
    ? "https://localhost:21001"
    : window.location.origin;
```

When the application is served from:

```text
https://192.168.0.12:21001
```

the interface automatically uses that same origin for API requests.

## API

ARIS uses the `/aris/v1` namespace.

### Authentication

```http
GET  /aris/v1/auth/bootstrap/status
POST /aris/v1/auth/create
POST /aris/v1/auth/authenticate
POST /aris/v1/auth/refresh
GET  /aris/v1/auth/session
```

### Schema

```http
GET  /aris/v1/schema

GET  /aris/v1/admin/schema
POST /aris/v1/admin/schema/fields
PUT  /aris/v1/admin/schema/fields/{uid}
PUT  /aris/v1/admin/schema/system/{key}
PUT  /aris/v1/admin/schema/order
```

### Storage Layout

```http
GET /aris/v1/admin/storage-layout
PUT /aris/v1/admin/storage-layout
```

### Records

```http
GET    /aris/v1/records
POST   /aris/v1/records
PUT    /aris/v1/records/{uid}
DELETE /aris/v1/records/{uid}
```

Record listing supports server-side search, filtering, sorting, and pagination.

### Attachments

```http
POST   /aris/v1/records/{uid}/attachments
GET    /aris/v1/records/{record_uid}/attachments/{attachment_uid}/preview
GET    /aris/v1/records/{record_uid}/attachments/{attachment_uid}/download
DELETE /aris/v1/records/{record_uid}/attachments/{attachment_uid}
```

### Dashboard

```http
GET /aris/v1/dashboard/summary
GET /aris/v1/status/revision
```

### Administration

```http
GET  /aris/v1/admin/audit

GET  /aris/v1/admin/backups
POST /aris/v1/admin/backups
POST /aris/v1/admin/backups/{uid}/verify
GET  /aris/v1/admin/backups/{uid}/download

POST /aris/v1/db/query
```

Administrator-only endpoints require an Administrator account.

## Configuration

ARIS reads runtime settings from `aris.config` and secrets from `aris.env`.

Example N1 configuration:

```ini
[n1]
base_url=https://127.0.0.1:50001
fragment=<ARIS_FRAGMENT_HASH>
insecure_tls=true
attachment_max_size_mb=50
```

N1 secret:

```env
N1_ARIS_SECRET=<ARIS_FRAGMENT_SECRET>
```

The ARIS server may bind to localhost for local-only use or to a LAN-facing address for network access.

Example:

```text
127.0.0.1  -> local machine only
0.0.0.0    -> listen on available interfaces
```

## Office Preview Requirements

On Debian:

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

## Build

Development:

```bash
cargo check
cargo run
```

Production:

```bash
cargo build --release
./target/release/litiaina-aris
```

## First Administrator

A fresh deployment starts without users.

The first Administrator is created through the bootstrap API:

```http
POST /aris/v1/auth/create
```

This operation requires a configured `AUTH_KEYS` value and is available only while the user table is empty.

The unified interface detects this state and presents the first-administrator setup flow automatically.

## Project Structure

```text
src/
├── api/
│   ├── admin/
│   ├── aris/
│   │   ├── handler.rs
│   │   ├── model.rs
│   │   ├── records.rs
│   │   ├── schema.rs
│   │   └── storage.rs
│   │
│   ├── user/
│   ├── audit.rs
│   ├── backup.rs
│   ├── dashboard.rs
│   ├── query_handler.rs
│   └── route.rs
│
├── config/
├── db/
├── middleware/
├── macros/
├── util/
└── main.rs

aris.html
aris.config
aris.env
```

The ARIS modules are separated by responsibility:

```text
schema.rs   -> dynamic record structure
records.rs  -> dynamic record values, search, filtering, sorting, pagination
storage.rs  -> configurable N1 namespace layout
handler.rs  -> attachments, preview, download, and N1 operations
```

## Design Principles

ARIS follows a small set of rules:

- stable internal record identities
- deployment-defined business fields
- dynamic field values instead of hard-coded record columns
- backend-enforced validation and authorization
- transactional auto-number allocation
- transactional field-order updates
- server-side search, filtering, sorting, and pagination
- SQLite for structured queryable state
- N1 for durable binary objects
- human-readable configurable attachment namespaces
- frozen per-record storage paths
- original uploads remain authoritative
- large binary data stays out of SQLite
- meaningful administrator actions are auditable
- backups are verified before being treated as valid
- schema evolution does not require recompiling the record model
- the same core can support different organizational workflows

## Philosophy

ARIS is not a single-purpose document tracker.

It is an **Atomic Record Information System**: a reusable engine where an organization defines what a record means while the platform provides the infrastructure required to manage it safely.

The record schema, field order, search behavior, uniqueness rules, numbering strategy, table presentation, and N1 storage hierarchy are configuration.

Identity, authorization, persistence, attachment integrity, auditing, search execution, and backup safety remain system responsibilities.

That boundary is what allows one ARIS deployment to behave like a correspondence registry while another can represent inventory, incidents, procurement requests, referrals, equipment, or another structured workflow without rewriting the core.