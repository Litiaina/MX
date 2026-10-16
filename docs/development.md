# Development and testing

[Documentation index](README.md) · [Project overview](../README.md)

## Development

Run the commands below from your MX repository root.

### Interface consistency

Shared styles use the existing legacy/tokens/foundation/shell/workspaces layers.
Svelte component styles remain unlayered so component-specific layouts win over
shared defaults. Do not wrap them in a new named layer without checking the actual
production CSS order: Vite can emit component styles before the layer declaration,
silently putting them below shared styles. This broke Drive during the withdrawn
2026-10-13 polish pass even though the input-size tests passed.

Keep screen-specific changes scoped. Deployment uses independent section stacks
to prevent a tall Appearance section from creating a Branding-to-Terminology gap;
its control adjustments do not affect Drive. Test container/row layout and real
actions, not just the dimensions of inputs and selects.

Validate production-built layout and file-manager behavior with disposable data:

```bash
npm --prefix frontend run build
npm --prefix frontend run test:ui:cascade
npm --prefix frontend run test:ui:sidebar
npm --prefix frontend run test:drive:file-manager
```

The sidebar suite checks production-built collapsed/expanded geometry at 85%,
100% and 160% interface scales, light/dark themes, many modules and mobile
drawers. It checks logo/icon alignment, unclipped rows, search focus and footer
visibility. Use an explicit external `MX_TEST_ARTIFACTS` directory and matching
`MX_TEST_BINARY` / `MX_TEST_STATIC_SITE_PATH` when running on this workstation.
It uses disposable data and disconnected browser audio.

`npm --prefix frontend run test:records:navigation` checks blank-installation
onboarding, deleting the last test module, Pages as the default, stable columns
and horizontal position, schema reuse and cancellation during rapid view changes.
Pair it with `test:records:window` for bounded caching, variable-height anchors,
reverse loading and interrupted reads, and `test:records:scroll` for the reachable
horizontal control. Set the same explicit external artifact/build environment.

The cascade suite asserts Drive row grids and aligned columns, sharing-dialog
dimensions, Deployment section gaps and control alignment in light/dark themes
at desktop and narrow widths. The file-manager suite exercises selection,
copy/cut/paste, uploads and navigation against disposable MX/SQLite and an N1
fixture. Both disconnect browser processes from workstation audio. Browser
executable paths in the scripts may need adjustment on another machine.

Format and check:

```bash
cargo fmt
cargo check
```

Strict warning-free check:

```bash
RUSTFLAGS="-D warnings" cargo check
```

Backend regression tests, including the N1 v4 HTTP-contract tests:

```bash
cargo test --locked
```

The opt-in N1 round-trip test requires a **disposable, isolated N1 v4 instance**
and a fragment token in `MX_TEST_N1_TOKEN`, with its base URL in
`MX_TEST_N1_URL`. It checks real one-shot/multipart uploads, downloads, valid
byte ranges, rename/trash/recovery, and concurrent writes across MX storage
prefixes. It writes unique test keys and soft-deletes them afterward; trash
remains until N1 retention/GC or disposal of the test storage. The test allows
self-signed TLS for this isolated instance. Do not point it at production.

```bash
cargo test --locked api::mx::n1::tests::real_n1_v4_round_trip -- --ignored
```

Install and validate the Svelte frontend with Node.js 24 or newer:

```bash
cd frontend
npm ci
npm run check
npm test
npm run build
cd ..
```

Cross-browser UI and network-fault regressions (local mocks; no production data
or N1 writes):

Browser tests generate audio and substantial CPU load. Run them on a dedicated
test machine/container, not while using the workstation for normal audio.
They now refuse to launch without `MX_TEST_AUDIO_SINK` naming a verified
PulseAudio/PipeWire `module-null-sink` output with an `mx_test_` prefix.
Only test browser processes use that sink and its monitor; Chromium is also
muted, and an ALSA-null configuration prevents hardware fallback. The guard does
not change system volume, defaults, or restart audio services. For example,
on an isolated Linux test host:

```bash
mx_test_audio_module_id=$(pactl load-module module-null-sink sink_name=mx_test_silent)
# Prefix browser test commands below with MX_TEST_AUDIO_SINK=mx_test_silent.
```

```bash
cd frontend
npx playwright install --with-deps chromium firefox webkit
MX_TEST_AUDIO_SINK=mx_test_silent npm run test:network:browser
```

The suite opens the real Svelte record editor/table, chat composer, and startup UI, and uses
native browser WebRTC with synthetic camera/microphone/screen sources. It checks
retained create identifiers after lost responses, menu visibility/hit testing in
light/dark and narrow layouts, inline-edit revisions after refresh, chat retries
after lost responses/file rejection, native file-body cancellation, dropped
offers/answers/ICE, immediate rejoin, and mixed-engine three-person calls with
signaling jitter. Media assertions inspect received RTP and changing pixels in
the actual video elements. Artifacts and screenshots are saved to a temporary
directory printed by the runner. `MX_TEST_BROWSERS=firefox` selects one engine;
`MX_CHROMIUM_EXECUTABLE`, `MX_FIREFOX_EXECUTABLE`, and `MX_WEBKIT_EXECUTABLE`
allow custom engine paths.
Set `MX_TEST_UI_ONLY=1` to run the UI/file scenarios without media groups.
`MX_TEST_DELETION_ONLY=1` exercises immediate row removal, stale refresh/module
responses, explicit restores, out-of-order events, missed restores during an
outage, and restarted server event counters in each selected browser.

Eight-account real-server validation (build the backend first):

```bash
cargo build --locked
cd frontend
MX_TEST_AUDIO_SINK=mx_test_silent npm run test:eight-users:browser
```

This runner starts its own MX HTTPS server, SQLite database, generated credentials,
and N1 v4 HTTP contract fixture. It never loads deployment `mx.env`/`mx.config` or
modifies the N1 repository. It checks eight distinct authenticated editors,
atomic unrelated-field merges, same-field conflicts, retried creates/messages,
deleted records during module updates, stale drafts after restore, cleared cells,
archived fields, eight concurrent 18-MiB multipart uploads with injected 503s,
byte ranges, and independent record attachment additions. Its browser phase
uses the production call UI and real HTTP/WebSocket signaling with synthetic
capture: eight participants, 56 directed peer connections, every sender's screen
and camera on every viewer, audio, leave/rejoin, same-account session replacement,
device controls, fullscreen, reconnect, and final call cleanup. A failed assertion
keeps diagnostics and marks the run failed; connection state alone is not proof
of working media.

`MX_TEST_API_ONLY=1` skips the browser phase. `MX_TEST_BROWSERS=chromium,firefox`
selects that engine mix; omitted engines are **not** validated by that run.
`MX_TEST_ARTIFACTS` selects the parent artifact directory and
`MX_TEST_SOAK_SECONDS` changes the media soak duration (default 15 seconds).
Generated artifact directories contain test credentials/private keys and should
not be published. To run the real-server suites against disposable live N1,
set `MX_TEST_N1_URL` and `MX_TEST_N1_CREDENTIALS_FILE`. The private credentials
file contains `N1_MX_SECRET` and either `N1_FRAGMENT` (raw) or `N1_FRAGMENT_HASH`.
The adapter never reads the deployment's `mx.env`; it injects transient part
failures and cleans up only unique test objects by N1 soft deletion.

MX Drive production UI/API tests include eight accounts, durable resume across
MX restart and browser module switches, no file pre-scan/read-back, visible
MB/s/ETA, pause/resume, lost finish/batch acknowledgements, guest ACL/range checks,
folder uploads, and independent copies into collaboration/record attachments:

```bash
MX_TEST_N1_URL=https://127.0.0.1:50001 \
MX_TEST_N1_CREDENTIALS_FILE=/private/path/n1-test.env \
MX_TEST_N1_DROP_FINALIZE=1 \
MX_TEST_AUDIO_SINK=mx_test_silent \
npm run test:drive:browser
```

Run from `frontend` after `cargo build --locked` and `npm run build`.
Without the live-N1 variables the runner uses an explicit HTTP contract fixture,
not live storage. `MX_TEST_API_ONLY=1` skips browser checks; no UI validation is
claimed for that mode. See [TESTING.md](../TESTING.md) for retained results and gaps.

`npm run test:drive:recovery` uses real production UI/MX/SQLite and an isolated,
fault-injected N1 HTTP contract fixture. It checks native disk-file uploads,
module navigation, disconnect/reconnect, manual pause, reload/original-file
reselection, recovery of a two-hour-old reservation, real MX restart, lost
finish acknowledgements, exact stored bytes and eight concurrent HTTP users.
It needs no desktop audio sink and rejects live-N1 mode because its storage
fault gates are fixture-specific. Use `MX_TEST_BROWSERS=chromium,firefox` for
the currently passing full recovery scope. The default three-engine run also
includes WebKit, whose tested WPE runtime currently loses native source-file
access; that is an unresolved qualification gate, not a successful upload.
`MX_TEST_BROWSERS=webkit MX_TEST_RECOVERY_SOURCE_FALLBACK=1` checks only the
safe source-loss message/no-empty-file-publication guard, not full recovery.
The controlled scheduling benchmark in `npm test` uses a virtual 2-GiB file;
it does not measure actual N1/LAN MB/s. See [TESTING.md](../TESTING.md) for evidence.

The native relationship runner tests eight-account merges/conflicts, referential
integrity, paged selectors and three-engine form/inline/admin UI:
`npm run test:relationships:browser` (with the same isolated audio sink).

`npm run test:office:browser` runs the **real bundled engine**, keyboard edits,
exported-document inspection, new-object version saves, offline tab closure and
reopening, lost-response reconciliation and simultaneous-save conflicts. Public
internet requests are blocked throughout the browser phase. This suite needs no
audio devices/sink and never changes system certificate trust. It uses a test-only
localhost proxy for the isolated self-signed MX server. The default N1 backend is
the explicit contract fixture; opt-in live N1 variables retain the same meaning.
`MX_TEST_BROWSERS` selects the browser engines. Focused diagnostic runs using
`MX_TEST_OFFICE_FORMATS_ONLY=1` are not the complete offline/concurrency UI suite.
`MX_TEST_OFFICE_SHELL_ONLY=1` checks the compact layout, keyboard/menu actions,
real DOCX save/download, opt-in offline retention and fixed canvas geometry;
it does not clear the existing spreadsheet input/rendering release gates.
Use `MX_TEST_BROWSERS=chromium,firefox` for the currently validated editing engines.
The separate `MX_TEST_BROWSERS=webkit MX_TEST_OFFICE_GRAPHICS_FALLBACK=1` run
verifies the unsupported-runtime message and unchanged download, **not** WebKit
editing. Presentation checks assert the read-only gate, not a successful edit.
The suite's result file explicitly records these limits and does not claim
production qualification.

`npm run test:records:scroll` exercises wide/tall real record tables in Chromium,
Firefox and WebKit: pointer/keyboard scrolling, both-way synchronization,
resizing/themes, normal-scrollbar handoff, column visibility, dialogs, mobile
cards, pagination and cleanup when navigating away. It uses disposable MX data
and the explicit N1 contract fixture by default, without audio devices.

`npm run test:drive:dropdowns` checks dropdown arrows on dashboard, module settings,
report builder, account creation and Drive. It covers explicit light/dark and
system-light/system-dark themes at desktop/mobile widths; Reporting period
keyboard input and persisted Navigation icon changes are checked alongside
user/group access roles, every guest-link expiry, committed ACLs/timestamps,
all sort choices and 25/50/100-item pages. Computed-style checks verify one
non-repeating chevron inset 12px with reserved text space, plus the native-arrow
fallback in high-contrast mode. Shared-style unit checks reject legacy theme
overlays and guard the shared decoration against component background overrides.
It also checks that dismissing a dropdown with Escape keeps Share open, while
Escape from another dialog control still closes Share. The three-engine runner
uses real isolated MX/SQLite, light/dark desktop/mobile layouts and process-local
null audio backends; it does not require or change a desktop audio sink.

For focused pagination/share/preview and file-manager regressions, use
`npm run test:drive:polish` and `npm run test:drive:file-manager` with the same
explicit test-only null sink. The polish runner seeds 135 isolated metadata items
per browser over an HTTP-uploaded object, exercises page boundaries and group
membership, and saves desktop/mobile screenshots. It is not a live N1 test unless
the opt-in live credentials authenticate successfully.

After all test browsers have exited, remove only the null sink created for them:

```bash
pactl unload-module "$mx_test_audio_module_id"
```

This does not certify physical devices, Safari/iOS, browser capture-picker audio,
TURN deployments, or real UDP packet loss. Exercise those on the target devices
and network before production rollout. The separate Chromium capture/fullscreen
regression remains available with `npm run test:calls:browser`.

Validation caveat (2026-10-08): repeated mixed-engine stress testing on the
development host intermittently crashed the Linux Playwright WebKit page;
subsequent reruns passed. The crash's root cause remains unconfirmed. Keep failed
artifacts and do not treat a later green run as production/Safari certification.
Earlier runs used Node.js 20.19.2. The subsequent checks, production build,
cross-browser regression suites, and live N1 tests used Node.js 24.21.0.

Run MX with the production frontend from `frontend/dist`:

```bash
cargo run
```

For browser-interface development, run `npm run dev` inside `frontend`. Vite
proxies `/mx` and `/ping` to the Rust HTTPS server. Svelte is a build-time
dependency; production does not require a Node.js process.

Production build:

```bash
cd frontend && npm ci && npm run build && cd ..
cargo build --release
```

Run the production binary:

```bash
./target/release/litiaina-mx
```

---

## Project Structure

```text
mx/
├── CHANGELOG.md
├── Cargo.toml
├── mx.config
├── mx.env
├── frontend/
│   ├── src/                 # Svelte 5 and TypeScript application
│   ├── public/              # Static frontend assets
│   ├── dist/                # Shipped production frontend served by Rust
│   ├── package.json
│   └── vite.config.ts
│
└── src/
    ├── api/
    │   ├── admin/
    │   ├── mx/
    │   │   ├── attachment_fields.rs
    │   │   ├── formula.rs
    │   │   ├── handler.rs
    │   │   ├── migration.rs
    │   │   ├── model.rs
    │   │   ├── n1.rs
    │   │   ├── n1_tests.rs
    │   │   ├── records.rs
    │   │   ├── schema.rs
    │   │   └── storage.rs
    │   │
    │   ├── user/
    │   ├── audit.rs
    │   ├── backup.rs
    │   ├── calls.rs
    │   ├── collaboration.rs
    │   ├── dashboard.rs
    │   ├── lifecycle.rs
    │   ├── live.rs
    │   ├── modules.rs
    │   ├── notifications.rs
    │   ├── preferences.rs
    │   ├── reports.rs
    │   ├── query_handler.rs
    │   └── route.rs
    │
    ├── config/
    ├── db/
    ├── middleware/
    ├── macros/
    ├── util/
    └── main.rs
```

The main responsibilities are intentionally separated:

```text
schema.rs             dynamic Record Structure
modules.rs            module identities and access matrices
records.rs            dynamic values, search, sorting, pagination
attachment_fields.rs  File Attachment field behavior and metadata
storage.rs            configurable and frozen N1 namespaces
handler.rs            record/attachment operations and N1 coordination
n1.rs                 N1 v4 object-key, multipart, and streaming transport
reports.rs            statistics, dashboard reporting, CSV export
dashboard.rs          dashboard support
live.rs               WebSocket sessions, live events, and presence
calls.rs              authorized WebRTC room state and private signaling
collaboration.rs      channels, messages, N1 file shares, and record links
notifications.rs      durable per-user notification delivery
preferences.rs        per-user appearance and notification overrides
lifecycle.rs          record versions, soft delete, Trash, and recovery
audit.rs              accountability history
backup.rs             verified SQLite snapshots stored in N1
```

---

