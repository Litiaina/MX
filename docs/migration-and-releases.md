# Migration and release notes

[Documentation index](README.md) · [Project overview](../README.md)

## Current candidate: MX 1.0.0-rc.10

RC.10 hardens concurrent saves, notification delivery and uploads. A legacy
global control-number mirror no longer rejects repeated values in independent
modules or in a field that is not configured as Unique. A missing or stale
internal sequence reconciles against stored identifiers inside the save
transaction; existing records and visible field values are not renumbered.
Explicit unique fields still reject duplicates and identify the field.

Startup adds a durable notification-outbox table before serving requests.
Current access and notification preferences are rechecked before delivery.
Existing data, publisher trust, worker/package protocol and N1 format remain
unchanged. No database reset, re-import or N1 upgrade is required. Use the normal
signed updater, retain its verified final SQLite snapshot and the previous
application pair, and follow the [RC.10 instructions](releases/1.0.0-rc.10.md).

## Previous candidate: MX 1.0.0-rc.9

RC.9 adds opt-in module workspaces, controlled processes and accounting/stock
books. Administrators choose the opening view and configure pipelines,
directories, operations overviews, document libraries, full-page books and
record-detail sections. Existing modules keep Records; new installations remain
blank. No business workflow or opening balance is created automatically.

The application adds process, workspace-layout, journal, stock and audit tables
at startup before accepting requests. Preserve the verified final pre-update
SQLite snapshot. Rollback must restore both the previous application and that
snapshot, not run an older binary alone against an upgraded database. Publisher
trust, package/worker protocol and N1 encoding are unchanged. See
[RC.9 instructions](releases/1.0.0-rc.9.md) and the [workspace guide](module-workspaces.md).

## Previous candidate: MX 1.0.0-rc.8

RC.8 fixes duplicate clipboard attachments, replaces the long module sidebar with
a searchable grouped Modules menu/library and retains administrator-configured
navigation order. It also repairs stale Office offline-manifest references and
checks final packaged assets. Existing modules, fields, account grants, records,
files, histories, upload settings and publisher trust remain unchanged. No new
database schema or N1 format migration is introduced. Follow the normal signed
update procedure; see [RC.8 instructions](releases/1.0.0-rc.8.md).

## Previous candidate: MX 1.0.0-rc.7

RC.7 removes the module form's hard-coded 50 MiB attachment ceiling and adds
revision-checked administrator upload settings for records and Collaboration.
Each supports a custom cap or No file-size limit. Files stage on temporary disk
and transfer to N1 in bounded parts. New requests use saved limits immediately; `mx.config` remains the startup
fallback. Existing files, modules, Drive quotas, publisher trust and the signed
update/backup/rollback protocol are unchanged. No new schema or N1 format
migration is introduced. See [RC.7 instructions](releases/1.0.0-rc.7.md).

## Previous candidate: MX 1.0.0-rc.6

RC.6 fixes file paste/drop in Collaboration and opens supported chat/record
Office attachments in the bundled read-only browser viewer, without a server
converter. Shared Drive cards open the existing versioned editor directly.
Publisher trust and the update/backup/rollback protocol are unchanged. No new
database schema or N1 storage-format migration is introduced. See the
[RC.6 changes and acceptance limits](releases/1.0.0-rc.6.md).

## Previous candidate: MX 1.0.0-rc.5

RC.5 adds calculations, creator-based privacy, administration/reading improvements,
background-only notification handling and the fresh-directory selection fix.
It is a release candidate, not stable 1.0.0. See the
[RC.5 changes and rollback boundaries](releases/1.0.0-rc.5.md).

## Earlier candidate: MX 1.0.0-rc.4

RC.4 is a database-contention hotfix. Schema initialization and compatibility
backfills run before serving requests, not during ordinary read/write handlers.
Existing imported records and histories remain; do not reset or re-import them.
The signing key and managed update/backup/rollback protocol are unchanged. See
[RC.4 instructions and limits](releases/1.0.0-rc.4.md).

On 2026-10-18, an initial documentation-only RC.4 refresh qualified N1
`1.0.0-RC.1` without changing runtime bytes. A subsequent, explicitly authorized
**importer hotfix replacement** changes the binary/frontend and signed package
while retaining version `1.0.0-rc.4` and publisher trust. RC.5 was not yet published
at the time of that replacement.
Original and intermediate artifacts remain archived for traceability.
Equal-version protection stays enabled: an earlier RC.4 requires a verified
operator replacement with backup, not an ordinary same-version UI update.
The local test replacement is explicitly authorized; older RC versions can use
the normal signed updater. See the current [RC.4 notes](releases/1.0.0-rc.4.md).

### Earlier candidates

RC.3 defers Office background preparation during active input, scrolling and
dragging, preserves unfinished spreadsheet cells until explicit Save or input
acceptance, and remembers an explicit save requested during an automatic upload.
It uses the existing publisher key and activation/backup/rollback protocol; no
new database schema or N1 format migration is introduced. See the
[RC.3 upgrade instructions and remaining limits](releases/1.0.0-rc.3.md).
Published RC.1/RC.2 remain unchanged. RC.3 is also a candidate, not stable 1.0.0.

RC.2 corrects sidebar alignment/clipping at different interface scales, defaults
record browsing to Pages with an optional bounded continuous view, and starts
new installations without a starter module. Existing modules/data remain. It uses
the same signed-update trust key and managed activation/backup/rollback protocol
as RC.1; no new schema or N1 format migration is introduced. See the
[RC.2 upgrade instructions](releases/1.0.0-rc.2.md). These remain candidates, not
stable releases.

On **2026-10-15**, MX starts a new official release-candidate line at
**1.0.0-rc.1**. The earlier 2.x/3.x/4.x numbering is retained as development
history, including previously published artifacts and tags. No historical tag
or changelog entry is rewritten or removed. This is a release-policy reset,
not a database reset or a claim of stable-release qualification.

The intended progression is `1.0.0-rc.1`, further numbered candidates as needed,
then `1.0.0` after acceptance. A published candidate is immutable: fixes require
a new candidate version. Subsequent stable releases use semantic versioning.

**Important:** `1.0.0-rc.1` sorts below `4.1.1` and `4.1.2`. Transition an existing
4.x installation explicitly with a matching candidate binary/frontend package;
do not expect the "higher version only" updater to select it. This one-time
transition stays operator-driven; do not disable downgrade protection generally.
After migration, the [signed updater](updates.md) accepts compatible newer 1.x
releases only. Historical 4.x development builds are not update targets.

Application version numbers are independent of SQLite migration state, record
revisions, N1 object keys/storage format and `/mx/v1`. Do not renumber, reset or
delete any of those to match 1.0. N1 remains independently versioned: current
compatibility is `1.0.0-RC.1`, NOA v1 and storage encoding 4. The earlier private
N1 `4.0.0` product label describes the same encoding-4 baseline, not a required
format migration.

Read the [candidate scope, transition procedure and acceptance gates](releases/1.0.0-rc.1.md).
The sections below describe the historical builds at their original versions;
their "current" wording and test limitations apply to those builds, not a new
qualification claim for this candidate.

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

Existing deployments upgraded from ARIS can migrate legacy internal SQLite objects to `mx_*`.

Existing database filenames and existing N1 attachment object keys do not need to be physically moved only for branding. Keeping durable object locations stable avoids unnecessary data movement and migration risk.

New MX backup objects use the `__mx` namespace.

---

## MX 4.1.1

MX 4.1.1 is a Drive reliability patch: personal/group sharing now delivers
conversation cards and recipient notifications, with a searchable, paginated
shared-item view. Uploads recover from transient interruptions, retain resumable
metadata across reloads and use bounded adaptive scheduling for large files.
Dismissed shared-file previews stay closed across module switches and reloads;
obsolete loading requests cannot reopen them.
See [CHANGELOG.md](../CHANGELOG.md) for the fixes and [TESTING.md](../TESTING.md) for
validation results and remaining limits.

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

The 4.1.1 version is not production certification. Office spreadsheet input and
Firefox rendering have unresolved acceptance failures; presentation files are
read-only and the tested WebKit/WPE editor uses a download fallback. Full call,
live-storage, security, complex-document and real-device qualification gates
remain documented in [TESTING.md](../TESTING.md). Do not infer clearance from a
successful build or a version bump.

### Upgrade and validation

For that historical build, Rust and frontend package versions were both **4.1.1**. Deploy the matching binary
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
part of the platform. See [CHANGELOG.md](../CHANGELOG.md) for the 4.0 changes and
the historical 3.0 release notes.

Existing saved deployment branding is preserved. New defaults use “Litiaina's
Digital Workplace Platform”; **Restore MX defaults** in Deployment identity
changes the editor only until the administrator saves it.

Version 4.1.1 is not a blanket reliability certification. The known Linux WebKit
large-upload crash, incomplete eight-person call validation, and current live-N1
test-fragment blocker remain documented in [TESTING.md](../TESTING.md).

---

