# Administration and security reference

[Documentation index](README.md) · [Project overview](../README.md)

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

