# HTTP API reference

[Documentation index](README.md) · [Project overview](../README.md)

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

