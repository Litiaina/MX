# Migration and release notes

[Documentation index](README.md) · [Project overview](../README.md)

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

Rust and frontend package versions are both **4.1.1**. Deploy the matching binary
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

