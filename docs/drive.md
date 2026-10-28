# MX Drive reference

[Documentation index](README.md) · [Project overview](../README.md)

## MX Drive

**MX Drive** in the workspace navigation is personal file storage backed by N1.
My Drive, Shared with me, Recent, Starred, and Trash provide folder browsing,
search, list/grid layouts, file-type icons, image thumbnails, preview navigation,
downloads, rename/move, file versions, and activity. The interface follows MX's
neutral light/dark surfaces independently of the custom accent color.

RC.11's Office appearance update also applies MX's configured logo,
theme, accent and personal text size to the separate document window's loading
screen, header and options. The document itself retains its own formatting and
native zoom controls; changing the workspace theme does not recolor document
contents or change saved file bytes. First-time engine startup and complex
workbook import are separate loading stages. Do not treat an open loader or
local prepared copy as an acknowledged server save.

Use **Upload files** or **Upload folder** to queue files; folder uploads preserve
the selected directory structure. You can also drop files or folders into Drive
(or directly onto an owned folder), or paste an external clipboard image/file
while Drive is open. Directory drops preserve nested and empty folders when the
browser exposes directory entries; use **Upload folder** as a fallback.
The queue admits two transfers at a time:
multipart files use console's default 1–4 concurrent parts each; groups of up to 16 files
of at most 1 MiB use N1's small-file batch endpoint. A failed batch entry does not
roll back successful files. Pausing a grouped small-file batch pauses that group.

Drive uploads use N1's restart-persistent **durable multipart** protocol. MX keeps
the N1 session and unique object identity in SQLite, reconciles N1's stored part
indices on resume, and sends only missing parts. Drive follows the console's
default part sizing: below 16 MiB, one part; 16–<256 MiB, 4 MiB parts;
256 MiB–1 GiB inclusive, 8 MiB parts; above 1 GiB, 50 MiB parts. N1 must
permit 50 MiB parts. There is **no Drive per-file size cap**; available account
quota and storage capacity still apply. Existing sessions retain their negotiated
part size when resumed. Lost finalization acknowledgements are reconciled
against the exact committed N1 version and size using N1's metadata endpoint.
MX does **not** download the whole object again before publishing it.

Like console, resumable uploading starts with a metadata-only session request and then
bounded native Blob parts—**no whole-file hashing or pre-scan**, regardless of
file size. Multipart quota is checked before reading/sending file bytes. N1 owns multipart
validation; MX retains immutable per-part SHA-256 receipts to reject different
bytes replayed into the same slot. Multipart versions do not claim a computed
whole-file checksum. Small files (up to 1 MiB each) use bounded batches with
per-entry quota enforcement. Scheduling does not allocate a queue entry for every part.
The old buffered
`POST /mx/v1/drive/upload` endpoint is retired (410); clients must use
`POST /mx/v1/drive/uploads`, part PUTs, and finish.

The upload panel is global to the authenticated workspace, fixed at the lower
right, collapsible, and independent of custom accent colors. Switching modules
or collaboration views does not abort its queue. It shows in-flight transferred
bytes, smoothed **MB/s**, estimated time remaining, pause/resume, and per-file
errors. Speed measures browser-to-MX transfer (decimal MB/s); N1 processing is
shown separately as finalization, not falsely counted as a finished upload.
Two files run at once, sharing an eight-part budget per browser document. Files
above 1 GiB start with four workers and can grow to eight after acknowledged
parts; chunk sizes still follow console. MX keeps its server-wide eight-transfer
cap and briefly queues contention before reading bodies, avoiding unnecessary
whole-part retransmissions during short bursts. This improves latency-limited
transfers, not the physical capacity of a link or N1's disks. Transient part
failures retry the identical slot/body with bounded backoff.
Each file's progress is a circular ring around its icon, not a linear bar.

### File-manager controls

Folders use server-side metadata pagination (50 items by default, selectable
25/50/100), a result range, numbered pages, next/previous and a page jump for long
lists. Name/date sorting and search run before SQL LIMIT/OFFSET, not on a loaded
whole folder. Empty last pages clamp after concurrent moves or deletions. Pickers
and public guest folders use bounded pages too. Selection is intentionally scoped
to the displayed page; changing pages clears that selection.

File previews have previous/next buttons and Left/Right shortcuts across page
boundaries. Neighbor queries seek by sort key and stable item ID, return only the
adjacent metadata, and load bytes only for the selected file. Text preview reads
at most 256 KiB with HTTP Range; larger text stays available through Download.
Images/video retain native streaming and range behavior. Visible image thumbnails
remain original-image previews, not generated small thumbnails.

If video/audio playback loses its connection, the preview shows a recovery
message and makes at most three automatic reconnect attempts. It retains the
playback position, volume, mute state and speed, and resumes only if playback was
active. **Retry playback** is available after recoverable failures. A diagnostic
request asks for one byte and immediately cancels its response; MX never buffers
the whole video just to check its connection. Upstream `Retry-After` is respected
for automatic retries. Long waits require a manual retry rather than an indefinite
background loop. Closing the preview or leaving the module cancels recovery.

Expired links or revoked access require reopening/signing in, not repeated
background requests. Unsupported codecs explain that the original can be
downloaded. Browser format support still applies; reconnecting cannot make an
unsupported codec playable. MX forwards storage 429/503 statuses and retry
information instead of presenting every storage rejection as a 502. This adds no
MX streaming rate limit or server-side retry loop.

Click once to select; double-click to open a folder or preview a file. Use the
checkboxes, Ctrl/Cmd-click to toggle items, Shift-click for a range, or drag a
rectangle from the blank area below the file rows. Ctrl/Cmd+A selects the current
page (up to 100 visible items). Shortcuts do not intercept text fields or dialogs.

Use **Copy** / **Cut**, or Ctrl/Cmd+C / Ctrl/Cmd+X, open the destination folder,
then **Paste** or Ctrl/Cmd+V. The account-scoped clipboard survives switching MX
modules but is cleared at signout. External files on the clipboard upload instead
of pasting the internal selection. A move is owner-only and atomic across the
selection; collisions or stale revisions reject the whole move. Copies receive
new MX IDs and ownership, preserve the current file version (not the original's
history/shares), and get a `(copy)` name on collisions, keeping the extension.
At most 100 selected roots / 5,000 nested items may be copied per request.

Right-click an item for contextual actions; right-click blank space to upload,
create a folder, or paste. These menus exist only inside the Drive file area;
other MX views retain their normal right-click behavior. The row's **More actions**
button provides the same controls on touch devices. Escape closes the menu;
arrow keys navigate it. F2 renames a single selection; Delete offers trash confirmation.

**More actions → Versions and activity** separates version cards from a dated
activity timeline. Versions have explicit Preview and Restore actions. Restoring
creates a new normal version, keeps all previous history, checks quota and the
loaded revision, and does not download the file through the browser.

Temporary upload interruptions show retry/reconnection state and automatically
resume with bounded backoff. Browser "online" is only a wake-up hint: real
MX/N1 reconciliation determines which parts to send. Waiting/paused states
retain acknowledged bytes and hide stale speed/ETA. Manual pauses, authentication,
permission, quota and revision failures do not trigger automatic resumption.
An interrupted request body is retryable; a genuinely oversized part is not.

Closing/reloading warns while transfers are active; sign-out stops that account's
jobs and releases its in-memory files. The account-scoped browser queue retains
only small metadata entries (no file bytes, credentials or N1 secrets). On
reopening MX, unfinished files show **Select original file to resume**. Reselecting
matches name, size, type and modification time, preserving manual pause and the
exact operation ID; N1 remains the stored-part authority. Separate entries avoid
one tab overwriting another file's recovery metadata. This metadata identity is
not a full-content checksum. No whole-file scan is performed.
If browser storage is blocked, uploads still work; reselect originals in their
Drive destination for server-owned session recovery. Inactive reservations and
their quota expire after **72 hours** by default, configurable with
`[drive] upload_retention_hours` from 1 to 168 hours. Expired/cleaned sessions may
require a fresh upload. Cancelling an unfinished upload releases its reservation
and queues N1 cleanup without deleting a completed file. This is not an offline
file-byte outbox or a service that keeps uploading after the browser closes.
Active-document credentials are retained in memory if browser storage is lost;
explicit signout and rejected authentication clear them. A browser process crash
can also invalidate selected file handles: reselect the original file to resume
the acknowledged parts. MX never accepts a truncated part as a successful upload.

### Metadata, ownership, and sharing

MX owns names, folders, ownership, grants, revisions, and trash. N1 receives only
immutable unique keys such as `__mx/drive/<owner-uuid>/<version-uuid>.MP4`.
Each version preserves its uploaded filename's original extension and casing.
Renaming/moving changes MX metadata only, never that key or the N1 object.
Drive-to-Drive copies and version restores reuse immutable N1 content through
independent MX version records. They never transfer the file's bytes or rename
its original storage key. Logical copies/versions each count toward their owner's
quota. Cleanup excludes every key still referenced by any Drive version, so
purging one copy cannot delete another copy's content. A copied object's N1 key
continues to identify its original upload, not the copy's new MX owner.
Names must be unique within the same owner's folder, case-insensitively; use
**New version** in the Versions tab when replacing an existing file.

Files are private by default, including from other administrator accounts.
Owners can share with authenticated MX users or active collaboration spaces/groups
they belong to, as viewers or editors. Space grants follow current membership:
new members inherit access; leaving, removal, archival or revocation removes that
grant immediately, including already-issued media tickets. Independent user/space
grants can still provide access; the strongest inherited role applies. Direct
conversations are not group-share targets. Folder grants
inherit to descendants. Viewers can preview/download; editors can rename and
upload file versions. Moving, trashing, creating children, and managing sharing
remain owner actions. Record-module grants do not grant access to private Drive
files. Sharing changes are persisted in Drive activity and authenticated API audit.
The Share dialog separates people, spaces, inherited folder access and public links
and keeps identity/role/removal controls readable on desktop and mobile.
Sharing with a person also posts a clickable Drive card in your private conversation;
sharing with a space/group posts it in that conversation. Access, the message,
activity and durable recipient notifications commit together. Retrying unchanged
access does not send duplicate messages or notifications. Role changes post an
access update; removing access makes the card unavailable unless another grant
still applies. Cards resolve current names and permissions rather than copying
file bytes into chat.
The Share dialog confirms the send and offers **View conversation**. In
**Conversation details → MX Drive**, a searchable, 50-item paginated list shows
current file/folder grants, including shares created before conversation delivery
was added. Clicking a card opens the file preview or shared folder in Drive.
Closing a preview clears its shared-file URL target. Switching modules and
returning to Drive does not reopen it; delayed metadata responses are cancelled
when you navigate away, choose another Drive view or open a newer file link.
An explicit click on the shared card can still open the same file again.
Personal notifications respect file-notification preferences and muted conversations;
group shares additionally respect mentions-only settings. Toasts, configured sounds
and browser alerts use MX's existing notification preferences. Public guest-link
creation does not send private/group notifications. Old grants remain visible in
the list; historical messages/notifications are not backfilled.
If an open folder is deleted or its access is revoked, the next refresh clears
the stale folder listing and returns to the current view's root with an explanation.

Owners can also create read-only guest links, optionally expiring after a selected
period. Anyone holding a link can read its shared file/folder subtree without
signing in. Links are stored hashed, never disclose N1 credentials/object keys,
and can be revoked. Trashing permanently revokes affected guest links; restoring
does not reactivate them. Every media request rechecks access/expiry. Private
preview tickets pin a version; guest requests reject a changed listing revision
instead of mixing byte ranges from different file versions.

**MX Drive** in the message composer and **Choose from MX Drive** on a record's
attachment field attach independent copies through the existing attachment
workflow. Trashing the original does not break those attachments. Destination
permissions and attachment limits still apply. Copying currently downloads the
file into the browser before uploading the new attachment; it is not zero-copy.

### Trash, quotas, and scope

Every account starts with **10 GB** (10 GiB). An administrator can assign a
different allowance under **Administration → Accounts → Storage** for any
account, including their own, or reset it to the deployment default. The panel
shows committed usage and pending reservations. Set zero to prevent new bytes;
lowering an allowance never deletes files. Quota changes are revision-checked
and audited; uploads recheck the owner's current allowance atomically at commit,
including versions uploaded by a shared editor. The Drive sidebar shows a storage
meter, usage, allowance, and full-storage guidance.

Moving to trash updates MX metadata only. **Delete permanently** / **Empty trash**
atomically remove MX metadata and enqueue durable background jobs. Those jobs retry
N1 **soft deletion**, including after restart/outage; only N1's TTL/GC physically
reclaims bytes. No permanent-delete N1 endpoint is assumed. Existing versions and
trash count toward the owner's quota; pending uploads reserve quota and commits
recheck it atomically. Backup both MX SQLite metadata and N1 storage.

Drive lists refresh quietly every 15 seconds while visible and no preview/dialog
is open, and when coming
online; this is not instant Drive-specific live synchronization. Native
video/audio previews use range-capable streaming. The existing Office PDF preview
uses optional server-side conversion. **Edit in MX Office** is a separate bundled
browser editor: see [Standalone Office editing](office.md#standalone-office-editing).
This is not Google Docs-style real-time co-editing, desktop sync, OCR/full-text
indexing, or Google Drive feature parity.

---

