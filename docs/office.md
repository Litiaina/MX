# Office editor reference

[Documentation index](README.md) · [Project overview](../README.md)

## Office Preview

MX previews supported files directly where possible.

```text
Images       -> browser image viewer
PDF          -> embedded PDF viewer
Text         -> text viewer
Audio        -> range-streamed browser audio player
Video        -> range-streamed browser video player
Office in Drive -> locally bundled browser Office editor
Office-to-PDF   -> optional server-side LibreOffice conversion
Other files  -> download
```

### Optional server-side PDF conversion

The bundled browser Office editor does **not** require installing LibreOffice
on the MX server. The older Office-to-PDF attachment-preview route is a separate
capability: it invokes a host LibreOffice converter and can fail if that converter
is unavailable. Downloading the original or using the supported Drive Office
action is independent of installing that converter.

Only deployments deliberately using server-side Office-to-PDF conversion need
the extra components. On Debian, that optional path can use:

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

### Standalone Office editing

MX deployment must remain MX + SQLite + N1. No Collabora/ONLYOFFICE server,
Docker stack, database service, public Office API or runtime CDN is required by
the bundled editor. In Drive, use the Office action / **Edit in MX Office** on a
supported file. Viewers open a read-only editor. This opens a separate browser tab
running a locally bundled LibreOffice/ZetaOffice WebAssembly engine; MX owns
authorization, metadata and versions, while N1 stores the bytes.

Double-clicking a supported Office file in Drive now opens this editor directly.
Opening an Office file from a shared-file link first offers **Open in MX Office**
and **Download original**; guest links offer downloading, not authenticated editor
access. These paths do not automatically request a server PDF conversion.

The editor fills the window beneath a single 38-pixel MX bar: filename, compact
save status, Save and **⋯ Document options**. There are no additional status,
offline or information rows and no MX footer. Open from Drive, Download copy,
Save as a copy, offline settings and detailed save/certificate information are
in Document options. The menu and actionable error/recovery messages overlay
the editor rather than resizing it while you type. LibreOffice's own editing
menus, formatting controls and formula bar are retained.

While opening a file, Office shows five loading stages: prepare the document,
download it, load bundled editor resources, initialize the editor on your device
and render the document. The loading card includes the filename, elapsed time,
received bytes and measured download percentages when a valid byte total is
available. Initialization/rendering stay indeterminate: there is no invented
overall percentage or startup ETA. Failures stop the loading animation. This
card disappears once the editor opens; it does not add rows to the editing UI.

Large/format-heavy spreadsheets may take several minutes even when downloading
the original is fast. Opening and export have a ten-minute deadline; runtime
startup is bounded too. **Cancel opening** stops document downloads and editor
workers without changing the original or a retained local copy. A failed/cancelled
opening offers **Retry opening**, **Download loaded copy** (or streamed **Download
original** if it was not loaded), and **Return to Drive**. Readiness includes native
input focus, not merely loading the engine.

Calc's first input selection is initialized again after the final layout/focus
acknowledgement; ordinary refocus does not reset unfinished cell input.

The pinned engine has a fixed 1 GiB WebAssembly heap; increasing a JavaScript
setting does not increase its compiled memory limit. MX reads at most a 2 MiB ZIP
directory plus its small trailing header to identify complex documents, without
decompressing/scanning the whole file or removing cells/styles. The editor rejects
inputs over 128 MiB before buffering, and archives reporting over 256 MiB expanded
content before opening it in the engine. Engine startup can overlap downloading;
the document is not opened until inspection succeeds. These are editor safety budgets, not Drive
upload/file-size limits. Download the original for a document outside these budgets.

Optional server PDF previews have separate limits: one conversion at a time,
16 MiB original input, 32 MiB archive expansion, 32 MiB PDF output and 45 seconds
runtime. Identical inputs share an in-flight conversion; failures are cooled down
for 30 seconds. Unix process-group cleanup stops the launcher and its descendants;
Linux additionally bounds converter address space and generated file size. Size
rejections happen before the preview download when metadata already exceeds the
budget. A client disconnect does not leave an unbounded conversion running.

Editing is offered for DOC/DOCX/ODT/RTF and XLS/XLSX/ODS, using explicit matching
export filters. PPT/PPTX/ODP open **read-only**: native presentation editing
failed acceptance tests and is not enabled in this build. Macro execution and automatic linked-data
updates are disabled; macro-enabled DOCM/XLSM/PPTM files are not offered for
editing. Format availability is not a guarantee of perfect fidelity for every
document. See [TESTING.md](../TESTING.md) for the formats and browser engines actually tested.
Earlier expanded acceptance runs caught intermittent missing leading input in
fresh spreadsheets and a Firefox native-canvas rendering failure. The scoped
opening/input fix has passed current focused Chromium/Firefox XLSX and DOCX checks,
but does not requalify every format or recovery scenario. Keep separate copies of important
documents and do not confuse confirmed MX snapshot storage with validation of
every native editing interaction.

#### Offline operation

There are two independent cases:

- **No public internet, MX/N1 reachable on the LAN:** open, edit and save normally.
  Every engine/SDK/font asset is served from MX. Deployment needs only the Rust
  binary, the complete `frontend/dist` package, SQLite and N1; Node.js and a host
  LibreOffice installation are not needed for this editor.
- **MX/LAN itself unreachable:** previously prepared documents can reopen and
  remain editable. Visit Office once while connected, open **Document options**
  and wait for **Editor
  available offline**. Enable **Keep offline copy on this device** for each
  document, then bookmark `/office/index.html` for the offline document list.
  Recovery snapshots are retained in IndexedDB after an idle pause. Automatic
  saving to MX is enabled by default; reconnecting can resume a pending save with
  the same operation ID. Disable **Automatically save to MX when idle** in
  Document options for manual server saves. Both modes check the original
  revision: reconnecting never silently overwrites another editor's version.

The editor-only service worker caches public application/engine assets, **not**
API responses, credentials or documents. Long-term offline retention is a separate,
explicit opt-in. Automatic recovery copies are created for edited documents even
when that option is off. They expire after seven days and are cleaned at the next
recovery scan; a confirmed MX save removes its automatic copy when there are no
newer edits. Separate editor tabs have separate recovery keys. Local copies are
readable by anyone using the same browser profile: use dedicated profiles on
shared workstations and remove local copies before handing over the profile. Offline copies
are not backups or cloud saves. Browser storage limits, eviction/private mode,
device failure or clearing site data can remove them. Download a separate copy
when needed. Changes not yet checkpointed can be lost in a process/power crash.
Removing a local copy does not delete MX versions, and revoking server access
cannot retract a copy already downloaded to a user's machine.

Office requires a secure context, WebAssembly threads/SharedArrayBuffer, worker
WebGL and
browser-trusted HTTPS. LAN deployments can use an internally trusted CA; a public
internet connection is not necessary. Merely accepting a self-signed certificate
warning may prevent service-worker installation. Preserve MX's isolation headers
through any reverse proxy. They apply only to `/office/`, not the workspace/calls.
Offline caching failures have a concise, expandable explanation in **Document
options**, technical
details and a retry action. They are separate from MX save errors: an expired or
untrusted certificate can prevent offline preparation even when the browser
lets you open MX after bypassing its certificate warning. Renew the certificate
and establish browser trust; bypassing the warning is not an offline-mode fix.
If the engine cannot
start, an already loaded document/local copy remains downloadable. A disposable
worker checks actual graphics support before the large runtime starts: merely
having `OffscreenCanvas` or main-page WebGL is not enough. The tested WebKit/WPE
runtime lacks worker WebGL; its download fallback is tested, not Office editing.
Chromium/Firefox editing results are listed separately in `TESTING.md`.

Each frontend build changes the offline worker's generation as well as its asset
manifest, so already-cached browsers can receive editor updates. Before closing
an old editor tab, save successfully to MX or download a copy of unsaved edits;
then reopen the document to use the updated interface.

#### Saving and concurrency

Use **Save to MX**, Ctrl/Cmd+S, or the native File → Save/Save toolbar action.
The compact bar distinguishes **Unsaved**, **Preparing**, **Saving locally**,
**Uploading**, **Confirming**, **Saved locally** and **Saved to MX**, with server success only after MX
acknowledges the commit. Hover its status or open **Document options** for
**Saved to MX · revision … · time** and detailed progress; the full status is
also announced to assistive technology. Unsaved/newer edits and failures stay marked
unsaved; a local draft or native temporary file is not proof of an MX save.
Saving an unchanged document explains that there are no unsaved changes rather
than exporting the entire file again or creating another version. **Sign in to save** is a recovery action shown
only when authentication is missing/expired, not an extra Office account.
Recovery keeps the open edits and requires the original MX account.
Save also accepts the spreadsheet cell currently being typed, without requiring
Enter first. The native spreadsheet address box identifies the active cell; its
local-only accessible selection is also retained. It does not create server edit
sessions or presence tracking.

Native File → Open, the folder toolbar action and Ctrl/Cmd+O guide you to
**Open from Drive** in **Document options**. Each editor tab remains bound to its original MX file.
Opening a different local file through the engine's unsupported native picker
is not enabled; upload/open it through Drive in a separate editor tab. This also
prevents accidentally saving a different displayed document over the original.

`GET /mx/v1/drive/items/{uid}/office` returns an ACL-checked snapshot with the
current MX revision, exact version UID and `editing_supported` flag; the editor downloads that pinned
version. Editing stays entirely local, with no edit locks, edit-presence heartbeat
or server Office sessions. A read-only connection check shows an offline icon
without tracking edits. MX's existing resumable multipart upload publishes saves:

1. Accept pending cell input and export the document in its original format in
   the browser. Reuse an already prepared snapshot if the engine generation is
   unchanged. The local checkpoint does not trigger a second full export.
2. Complete an IndexedDB recovery transaction containing the exact payload,
   original revision and upload operation ID before starting the upload. Request
   strict transaction durability where supported; this is not a physical
   power-loss or browser-storage guarantee.
3. Upload it as a **new uniquely keyed N1 object**, preserving the original
   extension. Never replace an earlier Office version's content.
4. Atomically recheck access, owner quota and the original MX revision, insert a
   normal MX file version and switch the current-version pointer.

An uncertain save or separate-file save retains its exact upload operation/payload
for idempotent retry in automatic recovery, without requiring offline retention.
Reopen the editor and choose **Open local copy**. The pending save checks its
original operation, including whether MX already committed it before the response
was lost. A tab cannot keep uploading after it closes or the client powers off.
Recovered payloads use fresh memory-backed browser files before upload while
preserving filenames, timestamps and operation IDs; normal saves do not take
this recovery-only materialization path.
Save and Download queue behind an in-flight local
checkpoint; the user does not need to click a second time.
If local recovery storage fails, show an actionable error and stop before upload.
The user can download a copy or explicitly choose **Save directly to MX** without
device recovery. Never label that path locally protected. Saving remains whole-
document work: large exports can still take time. Background preparation waits
for at least 15 seconds without typing, scrolling, pointer dragging, held keys
or text composition. Opening Document options also postpones it. Preparations
are spaced at least 30 seconds apart, with a cost-based cooldown up to one
minute after expensive exports. Activity is rechecked after waiting for local
work; a queued automatic operation does not override renewed interaction.

Automatic preparation never accepts an unfinished Calc cell or reselects its
range. Finish the cell with Enter/Tab (or select another cell), or use Ctrl/Cmd+S
to explicitly include its current input. Automatic feedback stays compact as
**Autosaving…**; detailed preparation/upload/confirmation remains in the status
tooltip and Document options. It is not called saved to MX until the server
acknowledges the new version. Explicit Save during an automatic upload is
remembered and runs afterward to include newer edits.

This is not per-keystroke journaling: unfinished cell input and changes since
the last completed checkpoint can be lost on a crash. Once native export has
begun, it cannot be safely preempted; a large document can still briefly delay
input even though preparation starts only during an idle pause. No save-speed
or zero-loss power-cut guarantee is implied.

Document download and engine startup overlap. Offline asset preparation begins
after opening, not in competition with initial loading. Public engine resources
can reuse the browser HTTP cache. These changes do not promise faster native
parsing/layout for every complex workbook or remove the compiled engine limits.
Concurrent whole-document edits do **not** automatically merge paragraphs/cells:
a stale save asks the user to save a separate file, deliberately add a version
after the latest version, or discard/load the latest. Each explicit replacement
is checked again; previous versions remain in history. Native Office Save and
Ctrl/Cmd+S route through MX, not just the engine's temporary filesystem.

#### Packaging and current limits

`frontend/public/office/vendor/manifest.json` pins executable/data checksums;
`npm run build` verifies them without downloading assets. Brotli engine assets
include approximately 51 MiB of Brotli binaries plus approximately 80 MiB of
generated gzip alternatives for browsers/connections that do not advertise
Brotli. The engine expands to approximately 250 MiB before additional runtime/
document memory. Large documents require the
client to load/export the full file: Drive's unlimited-per-file multipart policy
does not imply unlimited browser editing memory. Editor tabs consume client RAM;
small server hardware still needs to serve assets and handle storage transfers.

The pinned upstream engine assets are dated 2025-05-13. This integration is **not
a completed current-engine security/support or third-party distribution-license
audit**; those are release gates. Provenance, source links and license notices are
in `frontend/public/office/vendor/THIRD-PARTY.md`. Physical low-memory devices,
mobile ergonomics and complex/legacy-format fidelity need target-device testing.
Direct Office editing currently starts from Drive, not record/message attachment
modals. The older optional headless LibreOffice PDF preview remains separate,
read-only and dependent on a host installation; it is not needed by this editor.
The native engine toolbar is desktop-oriented. Narrow windows scroll an
initialized-width canvas instead of distorting its text and hit targets. The MX
bar stays one row, with keyboard-operable scroll buttons available in Document
options. This is not a mobile-native Office interface.

---

