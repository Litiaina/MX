# HTTP API reference

## Module workspace presentation (RC.9)

`GET /mx/v1/modules/{module}/workspace` requires module Read and returns
`{revision,layout:{default_view,views,sections}}`. Revision zero and no views means
the existing Records interface; reads never create a layout or business data.

`PUT` on the same path requires Configure and accepts the exact current revision
and complete layout. Each view has `id`, `name`, `kind`, `title_field`, `fields`,
`summary_fields`, `description`. Kinds: `records`, `board`, `cards`, `overview`,
`documents`, `ledger`, `inventory`. Sections have `name` and `fields` (field keys).
Limits: 12 views, 8 card fields, 4 summaries, 16 record sections. Shared titles,
card information and summaries must reference active module-visible fields.
Pipeline/document/book prerequisites are validated. Conflicting revisions return
409; permission failures return 403; invalid layouts return 400 with operator
instructions. Save and configuration history commit in one transaction.

Work-queue reads optionally accept `q` (title substring, max 200 characters),
`counts_only=true` and `include_total=true`. Count-only responses contain `total`
and `stage_totals:[{state,total}]`, respecting module, creator, title-field and
action permissions before aggregation. Count-only reads return no record cards.
Normal queue pages remain bounded to 40; totals are computed only when requested.

[Documentation index](README.md) · [Project overview](../README.md)

## HTTP API

### Signed server updates

All `/admin/updates` endpoints require administrator access. Trust keys, release
origin and systemd authority are deployment configuration, not API-settable keys.

```http
GET  /mx/v1/admin/updates
POST /mx/v1/admin/updates/check
POST /mx/v1/admin/updates/stage
POST /mx/v1/admin/updates/install
PUT  /mx/v1/admin/updates/policy
POST /mx/v1/admin/updates/package?operation_uid={uuid}
GET  /mx/v1/system/update-health
```

Actions take `{"operation_uid":"UUID","version":null}` (`check`) or the exact
selected version (`stage`/`install`). A successful submission returns `202`; the
independent worker owns execution, so clients poll status rather than keeping an
install HTTP request open or replaying a lost acknowledgement. Concurrent jobs
return `409`. Operation IDs are action/version-bound and deduplicated.

Package upload is streamed `application/octet-stream`, not JSON or multipart.
Its safety budget is separate from Drive upload policy. The response acknowledges
receipt/queueing, not successful signature verification or installation.

Policy takes `{"mode":"manual","start_hour":2,"end_hour":3,"utc_offset_minutes":480}`.
Mode is `manual` or `automatic`; starts/ends are distinct hours 0–23 and the offset
is fixed minutes from UTC, from -720 to 840. Overnight windows are supported.

Public readiness contains version/availability, aggregate active request/call
counts, maintenance state and the planned restart time—never user/file data,
tokens, trust keys, database paths or job history. During activation, normal APIs
return `503` with `Retry-After: 5`; administrator update status and readiness are
still allowed. See [update lifecycle and operator setup](updates.md).

### Operational books — RC.9

Unreleased, authenticated and opt-in. `GET /books` requires module Read and
returns settings plus `can_configure` / `can_operate`. Saving settings requires
Configure and an exact `revision`. Reading book contents or submitting commands
requires an explicitly allowed operator level **and** module Read, Edit and
Reports grants. That is whole-book access, not creator-only record access.
Creating account/item definitions additionally requires Configure.

```http
GET  /mx/v1/modules/{module}/books
PUT  /mx/v1/modules/{module}/books
POST /mx/v1/modules/{module}/books/commands
GET  /mx/v1/modules/{module}/books/list/{kind}?after={sequence}
GET  /mx/v1/modules/{module}/books/journals/{uid}
```

List kinds are `accounts`, `journals`, `items`, `reservations`, `stock-events`.
Pages contain at most 40 items, `has_next` and `next_cursor`; cursors are ascending
sequence numbers scoped to the module. They are live pages, not frozen exports.

Settings contain `accounting`, `inventory`, `currency`, `money_decimals`,
`stock_decimals`, `operator_levels`, `closed_through` and `required_stage`.
Precision is 0–6; currency is three uppercase ASCII letters. Administrator must
remain an allowed operator; Viewer cannot be an operator. Period cutoffs cannot
move backwards. Currency/precision freeze after relevant first use.

Every command includes a UUID `operation_uid`, `configuration_revision` and
discriminator `kind`:

- `account`: `code`, `name`, `category` (asset/liability/equity/income/expense).
- `item`: `code`, `name`, `unit`.
- `journal`: `date`, `description`, optional `source`, and 2–100 `lines` with
  `account_uid`, `debit`, `credit`. Amounts are **decimal strings**, one side
  positive and the other the empty string; repeated accounts are rejected.
- `reverse`: `journal_uid`, `date`, `reason`. Creates opposite lines once.
- `stock`: `item_uid`, `movement` (receive/issue/reserve), positive decimal-string
  `quantity`, `note`, optional `source`.
- `reservation`: `reservation_uid`, `action` (release/consume), `note`.

A source is `{record_uid,record_revision}`. Posting verifies its current module,
row access, exact revision and, if configured, completed unchanged process stage.
Its stored values/file references are snapshotted in the posting transaction.
Journal detail hides source links when current row visibility denies access.

Commands return `{uid,replayed}` only after committing. The UUID is bound to the
actor, module and exact submitted payload; matching retries return the same UID,
different payloads conflict. An authenticated actor can reconcile their own exact
already-committed receipt after book permissions change; this returns only the
original UID, not book contents or permission to post a fresh entry.
Monetary/quantity outputs are exact decimal strings,
never JSON floats. Accounts report debit-minus-credit net balances. Journal lines,
movements, audit evidence and retry receipts commit atomically. Evidence cannot
be updated/deleted; modules with book audit history must be archived, not purged.
There are no automatic record-to-journal mappings or tax/valuation guarantees.
See [operator instructions and limits](operational-books.md).

### Module processes — RC.9

These endpoints are **unreleased**; published RC.8 does not provide them.
All require authentication and current module permissions. Record endpoints
also enforce row visibility. Configuration writes require Configure permission;
actions additionally require module Edit and the action's account/creator policy.

```http
GET  /mx/v1/modules/{module}/workflow
PUT  /mx/v1/modules/{module}/workflow
GET  /mx/v1/modules/{module}/work-queue?state={stage_key}&actionable=true&after={record_uid}
GET  /mx/v1/modules/{module}/records/{record}/workflow
GET  /mx/v1/modules/{module}/records/{record}/workflow/history?before={sequence}
POST /mx/v1/modules/{module}/records/{record}/workflow/actions
```

Configuration is separate from ordinary module `config`. PUT takes
`{revision, definition, confirm_existing_records}`. `definition` contains
`enabled`, `name`, `kind` (`custom`, `crm`, `erp`, `accounting`), `initial_state`,
`title_field`, `restart_on_edit`, `states: [{key,name}]` and `actions`.
Each action contains `key`, `name`, `from`, `to`, `access_levels` (0–3),
`creator_policy` (`any`, `creator`, `not_creator`), `required_fields` (field keys)
and `note_required`. Unknown properties are rejected. No business data is seeded.

GET configuration includes a revision and an administration-only `existing_records`
impact count (null for readers without Configure). Record status includes the
current stage, all three loaded revision numbers and permitted-account-level
actions. `missing_fields` and `blocked_reason` explain why a listed action is
disabled; clients must not interpret its presence as permission to execute it.

POST action body:

```json
{
  "operation_uid": "a22a97a7-3b7e-47ac-a8f3-d30775dd3620",
  "action": "submit",
  "configuration_revision": 1,
  "workflow_revision": 0,
  "record_revision": 1,
  "note": "Ready for review"
}
```

All revision numbers come from the record workflow GET; do not hard-code them.
Success returns `{event_uid, replayed, status}`. Same-actor, same-record replay
of the same operation/payload returns the original event. A changed payload or
stale revision returns 409. Missing requirements return 400, denied actions 403,
and inaccessible/missing records 404. Decisions, saved snapshots, state and
preference-aware notifications are committed together. Normal record APIs cannot
set the process stage. History remains readable after disabling a process.

Queue/history responses have `items`, `has_next`, `next_cursor`, with at most
40 entries. Queue cursors are ordered record UIDs; history cursors are descending
event sequences. No arbitrary scripts or SQL predicates are accepted. Read-only
endpoints do not create process instances or initialize schemas on request.
See [operator setup and capability limits](module-processes.md).

### Module spreadsheet imports

All endpoints require authentication, current module read/create permission and
ownership of the import job. Imports are create-only. Results/history use 20-item pages.

```http
GET    /mx/v1/modules/{module}/imports?page=1
POST   /mx/v1/modules/{module}/imports
GET    /mx/v1/modules/{module}/imports/{job}
PUT    /mx/v1/modules/{module}/imports/{job}/configure
POST   /mx/v1/modules/{module}/imports/{job}/process
GET    /mx/v1/modules/{module}/imports/{job}/rows?state=failed&page=1
POST   /mx/v1/modules/{module}/imports/{job}/retry
DELETE /mx/v1/modules/{module}/imports/{job}
```

Upload multipart fields: `job_uid` (client UUID), `delimiter` (`comma`, `semicolon`,
`tab`), `file` (one XLSX/ODS/CSV/TSV). Replaying the same UUID/file/delimiter returns
the existing job. Source cells are staged in SQLite; no N1 object is created.

Configure body: `{"sheets":[{"index":0,"header_row":1,"first_data_row":2,
"columns":[{"column":1,"key":"name","fill_down":false,"date_format":"iso"}]}]}`.
Columns/rows are one-based; sheet indices are zero-based. Date formats:
`iso`, `dmy`, `mdy`, `mdy-short-dot`, `month-short`, `month-long`.

Process saves up to 20 pending rows per call. Success is committed atomically
with the ordinary record/version. Infrastructure failures pause rather than
marking rows invalid. Results states: `pending`, `failed`, `imported`, `source`;
optional `sheet` restricts a results page. Row errors include field/message and,
when conversion failed, source column/value. Corrections POST
`{"sheet":0,"row":2,"values":{"name":"Corrected"}}` to retry and receive
`record_uid`; successful-row replay returns its existing ID. Configuration cannot
be replaced after processing starts. DELETE removes the log/staged cells only.

See [operator instructions and safety budgets](importing-records.md).

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

`PUT /mx/v1/admin/schema/fields/{uid}` accepts optional `field_type` and
`base_field_type`. A changed datatype is currently supported only for
`text` → `long_text`; other changes return `400`. A stale `base_field_type`
returns `409` without saving settings. The check, field metadata change and
schema revision increment share an immediate transaction. Stored record values,
record revisions and historical versions are not rewritten. Long text disables
sorting while retaining the existing key, ID and required/unique settings.

Creating a schema field with a reserved key returns `409`,
`error: "field_key_conflict"`, and `field_key_conflict` containing `field`
(`uid`, `key`, `label`, `field_type`, `active`, `module_uid`, `module_name`) and
`suggested_key`. Archived fields keep their keys. Suggestions are advisory;
the key check and insert share one immediate transaction. Failed creation does
not advance the schema revision or change historical record values.

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

