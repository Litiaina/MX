# Deployment configuration reference

[Documentation index](README.md) · [Project overview](../README.md)

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

`attachment_max_size_mb` and `collaboration_file_max_size_mb` set the default
per-file limits for module records and Collaboration respectively. `mx.config`
accepts **`0` to disable either per-file cap**; positive values are whole MiB.
It is read at startup: restart the correct MX service after editing it. A reverse
proxy can impose a smaller request limit too. `multipart_part_size_mb` controls
each part sent from MX to N1 and
must be between 5 and 100 MiB; it defaults to 16 MiB when omitted. The N1 server
must allow a part at least this large. Increasing an upload limit does not
require sending the complete object as one N1 part.

### Administrator upload settings (RC.7 onward)

RC.6 and earlier have a record-form bug that hard-codes 50 MiB even when
the server configuration is higher. Changing `mx.config` alone does not remove
that browser-side check; the matching fixed frontend/server must be deployed.

After updating to RC.7, open **Administration → Deployment → Upload limits**:

1. Under **Module record attachments**, select **Custom limit** and enter a
   positive whole MiB value, for example **100** for a 66 MB file, or choose
   **No file-size limit** to remove MX's per-file cap.
2. Configure **Collaboration attachments** separately if required.
3. Choose **Save upload limits**. The current enforced values are displayed;
   new uploads use them without a service restart. Existing files are untouched.
4. **Server default** removes that override and uses the value loaded from
   `mx.config [n1]`. File edits still require restarting MX to reload defaults.

Overrides are saved in MX's deployment settings database and survive restarts
and updates. A stale administrator save is rejected rather than silently
overwriting another change; **Refresh current limits** retains your draft for
review. Saving branding/appearance does not reset upload limits.

These are not Drive per-file limits or storage quotas. Raising them does not
increase the separate spreadsheet-import or browser Office budgets. Record/chat
uploads stage each file on private temporary disk (the MX service's `TMPDIR`, or
the operating system temporary directory) and send N1 parts with bounded memory.
Ensure this disk has space for simultaneous uploads. Temporary-disk failures
return an actionable HTTP 507 instead of claiming a successful save. Linux
spools are unlinked immediately and reclaimed on cancellation/process exit.
For very large files prefer Drive's resumable upload path. No file-size limit
does not remove quotas, file-count rules, permissions, or physical storage
capacity. Changing N1 part size does not bypass an MX or proxy limit.

The API represents **Server default** as `null`, **No file-size limit** as `0`,
and a custom cap as a positive whole MiB value. These are independent choices
for module and Collaboration attachments; Drive already has no per-file cap.

When a proxy returns HTTP 413, review its request-body setting (for example
nginx `client_max_body_size`) as well. Allow multipart overhead above the chosen
per-file size. Do not change a production proxy or restart MX without planning
for users' active work.

MX targets **N1 1.0.0-RC.1**, using the unchanged **NOA v1** (`/noa/v1`)
fragment-scoped API and **storage encoding 4**. N1's storage-format specification
is named `1.0.0-RC.1`; the earlier private `4.0.0` product label is historical,
not a different API or a request to rewrite stored data. Storage layouts remain
logical object-key prefixes: uploading an object creates its virtual parent
prefixes automatically. MX does not call `posix/mkdir`, which N1 format 4
rejects. This applies to record attachments, collaboration files, profile
photos, notification sounds, deployment logos, and database backups.
Multipart initiation, raw part uploads, finalization, and byte-range media
streaming use the NOA v1 contracts. Namespace mutations are checked for reported
per-key conflicts and failures even when N1 returns HTTP 200.

N1 RC.1 retains existing encoding-4 data and backups without migration. Encoding 4
does **not** automatically migrate older, unsupported development
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
upload_retention_hours=72
```

`quota_mb` is the default per-account allowance in MiB; administrator assignments
override it per user. Obsolete `max_file_size_mb` values are ignored: Drive has
no per-file limit. Record/chat attachment limits remain separate and their Drive
attachment copies are still buffered within those limits. Setting
`public_links=false` immediately disables all guest listing/media routes.
`upload_retention_hours` controls inactive pending-session cleanup and quota
reservation together; default 72 hours, supported range 1–168 hours. Active part
requests refresh the session; this is not a total upload-duration limit.

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

