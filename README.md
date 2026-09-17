<div align="center">
  <img
    src="https://github.com/Litiaina/litiaina-admin-webpage/blob/master/images/litiaina_icon.png?raw=true"
    alt="Litiaina Icon"
    width="150"
    height="150"
  />
  <h1>Litiaina Aris</h1>
  <p>
    Atomic Record Information System — Litiaina's programmable platform for
    structured records, rich attachments, search, routing, and custom
    information systems.
  </p>
</div>

---

ARIS, the **Atomic Record Information System**, is Litiaina's foundation for
building custom information systems around structured records and rich data.

ARIS treats every business record as an atomic unit containing structured
information, metadata, searchable fields, routing information, remarks, and
zero or more attachments. A record can represent a document transaction,
procurement request, administrative communication, incident report, equipment
record, referral, inventory item, workflow request, or another domain-specific
business object.

The goal of ARIS is not to provide one fixed document-tracking application.
It provides a reusable information-system core that can be customized
programmatically for different organizations and workflows.

SQLite owns ARIS record metadata, queries, control numbers, relationships, and
attachment metadata. N1 owns the durable attachment bytes. This separation keeps
business information independently queryable while allowing large or rich files
to remain under N1's storage lifecycle and durability model.

Attachments are first-class parts of a record. ARIS can store and serve images,
PDF documents, text files, audio, video, Microsoft Office documents,
OpenDocument files, and arbitrary binary data. Supported Office documents can
be converted into cached PDF previews through a local headless LibreOffice
installation while the original uploaded object remains unchanged in N1.

ARIS is designed for environments where users need more than simple file
storage. Records can be filtered and searched across multiple business fields,
sorted, paginated, associated with attachments, moved logically when identifying
record information changes, and protected through role-based authorization.

## Core Model

```text
Atomic Record
  |
  +-- UID
  +-- Control Number
  +-- Date
  +-- Office
  +-- Requestor
  +-- Subject
  +-- Routing
  +-- Remarks
  |
  +-- Attachments
        |
        +-- File Name
        +-- MIME Type
        +-- Size
        +-- N1 Object Key
        +-- N1 Version ID
```

Each record has an immutable UID used internally by ARIS.

Human-facing control numbers are generated server-side and are not supplied by
clients. Control numbers are unique within their configured yearly sequence,
increase atomically, are never reused after committed creation, and may reset
when a new year begins.

Attachments use their own immutable UIDs and remain independently addressable
through the ARIS API.

## Architecture

```text
                         ARIS
                          |
          +---------------+---------------+
          |                               |
          v                               v
       SQLite                             N1
          |                               |
  Record metadata                  Attachment bytes
  Control numbers                  Object versions
  Search/filter data               Durable storage
  User accounts                    Storage recovery
  Attachment metadata              Object lifecycle
          |
          v
     ARIS API
          |
          v
      ARIS Web UI
```

ARIS does not place large object data inside SQLite.

SQLite is responsible for information that must be queried efficiently. N1 is
responsible for the actual attachment bytes.

A stored attachment therefore resembles:

```text
SQLite
  file_name
  mime_type
  size
  object_key
  version_id
       |
       v
      N1
       |
       v
Original immutable object bytes
```

This lets ARIS provide rich business queries without forcing the object-storage
engine to behave like a relational database.

## N1 Attachment Layout

ARIS stores attachments in a human-readable N1 namespace derived from their
record:

```text
<office>/<date>/<control-number>__<file-name>
```

Example:

```text
MISO/2026-09-17/000125__purchase_request.docx
```

The control number is padded in the object key to preserve natural lexical
ordering.

If identifying record information such as office or date changes, ARIS can move
the corresponding attachment objects while preserving the record and attachment
identities.

The original attachment stored in N1 remains authoritative.

Derived data such as previews or indexes may be discarded and regenerated.

## Rich Attachment Preview

ARIS provides native browser previews where possible:

```text
Images       -> browser image viewer
PDF          -> embedded PDF viewer
Text         -> text viewer
Audio        -> browser audio player
Video        -> browser video player
Office       -> LibreOffice -> PDF preview
Other files  -> download / browser-supported fallback
```

Office preview currently supports common Microsoft Office and OpenDocument
formats:

```text
Word
  .doc
  .docx
  .docm
  .rtf
  .odt

Excel
  .xls
  .xlsx
  .xlsm
  .ods

PowerPoint
  .ppt
  .pptx
  .pptm
  .odp
```

Office files are downloaded internally from N1, converted through headless
LibreOffice, and returned to the browser as PDF previews.

The original Office file is never replaced.

Generated previews are cached using the attachment identity and N1 version so
repeated views do not require repeated conversion.

```text
N1 original
    |
    v
ARIS Preview Engine
    |
    +-- cached preview exists -> return PDF
    |
    +-- no cached preview
            |
            v
       LibreOffice
            |
            v
           PDF
```

## Search and Query

ARIS is designed around information retrieval rather than simple filename
browsing.

Current record queries can combine fields such as:

```text
Control Number
Date Range
Office
Requestor
Subject
Routing
Remarks
Attachment File Name
Attachment Presence
Universal Search
```

Results are paginated and can be sorted without loading the entire record
collection into the browser.

ARIS is intended to grow toward full attachment-content indexing as well.
Extracted document text can eventually be indexed independently from the
original N1 objects so searches can locate information inside Word documents,
PDFs, spreadsheets, presentations, and other supported formats.

## Access Levels

ARIS uses four access levels:

```text
0 = Administrator
1 = Manager
2 = Editor
3 = Viewer
```

Their intended capabilities are:

```text
Administrator
  - manage user accounts
  - read records
  - create records
  - edit records
  - upload attachments
  - download and preview attachments
  - delete records and attachments
  - database administration

Manager
  - read and search records
  - create records
  - edit records
  - upload attachments
  - download and preview attachments
  - delete records and attachments

Editor
  - read and search records
  - create records
  - edit records
  - upload attachments
  - download and preview attachments
  - cannot delete

Viewer
  - read and search records
  - download and preview attachments
  - cannot create, edit, upload, or delete
```

Authorization is enforced by the backend through the authenticated JWT claims,
not only by hiding controls in the browser.

## Components

```text
src/
  api/
    admin/             Administrator account and database operations
    aris/              Atomic record, attachment, search, and N1 operations
    hosting/           Hosted UI support
    user/              User self-service operations
    api_error.rs       Shared API error definitions
    query_handler.rs   SQLite account/query helpers
    route.rs           ARIS HTTP route definitions

  config/
    init_config.rs     Configuration initialization
    init_env.rs        Environment initialization
    load_config.rs     Runtime configuration loading

  db/
    connector.rs       SQLite connection handling
    init_db.rs         Database and ARIS schema initialization

  middleware/
    auth.rs            JWT authentication and access-level authorization
    totp.rs            TOTP verification

  macros/
    error.rs           Error-reporting helpers

  util/                Shared utilities
  main.rs              ARIS application entry point

aris.html              Main ARIS record interface
admin.html             Administrator and account-management interface
aris.config            Runtime configuration
aris.env               Local secrets
```

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

### User Administration

```http
POST   /aris/v1/auth/get
PATCH  /aris/v1/auth/modify
DELETE /aris/v1/auth/delete

POST /aris/v1/auth/enable_2fa
POST /aris/v1/auth/disable_2fa
POST /aris/v1/auth/check_2fa

POST   /aris/v1/user/create
PATCH  /aris/v1/user/modify
DELETE /aris/v1/user/delete
```

### Records

```http
GET    /aris/v1/records
POST   /aris/v1/records
PUT    /aris/v1/records/{uid}
DELETE /aris/v1/records/{uid}
```

### Attachments

```http
POST /aris/v1/records/{uid}/attachments

GET    /aris/v1/records/{record_uid}/attachments/{attachment_uid}/preview
GET    /aris/v1/records/{record_uid}/attachments/{attachment_uid}/download
DELETE /aris/v1/records/{record_uid}/attachments/{attachment_uid}
```

### Database Administration

```http
POST /aris/v1/db/query
```

Database administration requires an Administrator account.

## Configuration

ARIS reads its normal runtime settings from `aris.config` and sensitive values
from `aris.env`.

N1 integration resembles:

```ini
[n1]
base_url=https://127.0.0.1:50001
fragment=<ARIS_FRAGMENT_HASH>
insecure_tls=true
attachment_max_size_mb=50
```

The N1 fragment secret is kept separately in the environment file.

The configured attachment size limit is used by both the HTTP multipart layer
and the attachment handler. ARIS reserves a small amount of additional HTTP
body capacity for multipart framing while enforcing the configured value as the
actual attachment limit.

## Office Preview Requirements

Office preview requires LibreOffice on the ARIS server.

On Debian:

```bash
sudo apt update

sudo apt install -y \
  libreoffice-writer \
  libreoffice-calc \
  libreoffice-impress
```

Verify the headless converter:

```bash
libreoffice --headless --version
```

ARIS uses LibreOffice only as a conversion worker. Users do not interact with
the LibreOffice desktop application.

Generated previews are stored in the operating system temporary directory:

```text
/tmp/aris-preview-cache/
```

They are derived data and may be regenerated from the authoritative N1 object.

## Build

```bash
cargo build --release
```

For development:

```bash
cargo check
cargo run
```

For production:

```bash
cargo build --release
./target/release/litiaina-aris
```

## First Administrator

A fresh ARIS deployment begins without user accounts.

The first administrator is created through:

```http
POST /aris/v1/auth/create
```

The request requires one configured `AUTH_KEYS` value.

ARIS only allows this bootstrap operation while the user table is empty. The
first account is always created as:

```text
Access Level 0
Administrator
```

After bootstrap, additional accounts are created and managed through the
Administrator interface.

## Authentication

ARIS uses short-lived JWT access tokens and longer-lived refresh tokens.

```text
Login
  |
  +-- Access Token
  |     short-lived
  |
  +-- Refresh Token
        longer-lived
```

When an access token expires, the client may exchange its refresh token for a
new session.

Role information is reloaded from SQLite during the refresh process so account
changes can take effect without permanently embedding old permissions into
long-lived sessions.

Optional TOTP-based two-factor authentication is supported for accounts.

## Custom Information Systems

ARIS is intentionally domain-neutral.

The same core can be customized into systems such as:

```text
Document Tracking System
Procurement Information System
Administrative Routing System
Incident Reporting System
Equipment Registry
Inventory Information System
Referral Tracking System
Project Record System
Request Management System
Records Management System
```

A custom ARIS deployment can change its user interface, record fields, workflows,
queries, terminology, and reporting while keeping the same underlying model:

```text
Atomic Records
+
Structured Metadata
+
Rich Attachments
+
Search
+
Authorization
+
N1 Storage
```

ARIS therefore acts as the reusable information layer rather than a
single-purpose application.

## Design Principles

ARIS follows several core rules:

- records have stable immutable identities;
- human control numbers are generated by the server;
- authorization is enforced by backend handlers;
- SQLite stores queryable business state;
- N1 stores durable attachment bytes;
- original uploaded files remain authoritative;
- previews and indexes are derived and rebuildable;
- attachment storage paths remain understandable to administrators;
- large binary data is not placed inside the relational database;
- destructive permissions are separate from normal editing permissions;
- custom information systems can be built without redesigning the storage
  foundation.

ARIS is designed to make organizational information searchable, structured,
durable, and programmable without forcing every new internal system to rebuild
the same record, attachment, authentication, and storage infrastructure.
