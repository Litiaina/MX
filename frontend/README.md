# MX Frontend

The MX frontend lives in this directory as part of the same repository and
release artifact as the Rust backend.

The frontend and Rust package share one product version. For MX 3.0.0, both
package manifests and the committed production bundle must be released together.

## Layout

- `src/` contains the Svelte 5 and TypeScript replacement frontend.
- `dist/` is the generated, verified production bundle shipped with MX so a
  fresh checkout can run without installing Node.js first.
- `public/` contains static assets copied into the production build.

Rust remains responsible for the API, authentication, WebSockets, SQLite, N1
integration, TLS, and static hosting. Svelte produces static HTML, CSS, and
JavaScript only; MX does not require a Node.js server in production.

## Development

Use Node.js 24 or newer:

```bash
cd frontend
npm ci
npm run dev
```

Vite listens on `http://127.0.0.1:5173` and proxies API traffic to
`https://127.0.0.1:21001` by default. Override the backend when needed:

```bash
MX_BACKEND_ORIGIN=https://127.0.0.1:22001 npm run dev
```

Validate and build:

```bash
npm run check
npm test
npm run build
```

The Rust server serves `frontend/dist` in production. Rebuild and commit that
bundle whenever frontend source changes. A Node.js process is not required at
runtime, and an unchanged fresh checkout can start directly with `cargo run`.

MX 3.0 media previews depend on the matching Rust routes for short-lived preview
tickets and HTTP byte-range forwarding. Restart the Rust service after deploying
a newly built frontend so the client and API cannot become version-skewed.

Voice, video, and screen sharing likewise require the matching Rust call-state
and signaling routes. Browser device capture requires HTTPS outside localhost;
configure the backend `[webrtc]` STUN/TURN settings for clients that cannot make
a direct peer connection. Supported browsers can send tab/system audio when the
user enables **Share audio** in the screen-capture picker; it remains independent
from the microphone track. Incoming private calls use a bounded repeating ring
and persistent browser notification, while group calls use one softer chime and
a non-blocking notification; saved sound, volume, browser-alert, and quiet-hour
preferences remain authoritative. Automated frontend tests validate the media policy and
signaling contract, while final deployment verification requires two real browser
sessions because Node.js cannot emulate cameras, screen capture, or ICE routing.

Call video presentation preserves its current decoder when the underlying track
and camera/screen mode are unchanged. Each tile uses one playing video; source
changes retain a still-frame overlay capped at 1280×720 until the replacement
is decoded. No video opacity crossfade or second playing video is used.
Unchanged participant heartbeat responses reuse existing objects, and the
device-panel preview compares video tracks so microphone-only changes do not
restart video playback. These are post-3.0.0 fixes recorded under **Unreleased**
in the root changelog. Deployment verification should include camera-to-screen-
to-camera switching, microphone toggles, and several heartbeat intervals.
Remote media publishes a new stream wrapper when its track set changes so track
arrival cannot be hidden by an earlier participant-state update. Group sender
updates run concurrently across peers; pending negotiation survives an
outstanding offer and resumes after the answer. A loaded first frame is enough
to reveal a static screen share. Include three-person calls with each person
sharing, delayed signaling to one viewer, and repeated stop/start in verification.
Self screen-share preview defaults to hidden, supports explicit show/hide, and
pauses in native fullscreen to break capture feedback. Screen sharing no longer
automatically focuses the local tile, and the device panel hides shared-screen
preview. Capture options request `selfBrowserSurface: 'exclude'` where supported.
Remote playback attaches only audio tracks to its audio element and preserves
that stream across video changes. Verification should also cover sharing the
call tab itself, local preview toggling, repeated fullscreen cycles, microphone
continuity, and remote fullscreen viewing.

The self-capture regression test starts an isolated Vite server and three
Chromium sessions, forces actual capture of the call tab, and checks hidden
self-preview, share restarts, local and remote fullscreen, and a stable audio
playback binding. It uses generated microphone audio and bypasses the live MX
API; it does not modify accounts, records, or N1 data.

```bash
npx playwright install chromium
npm run test:calls:browser
```

Set `MX_BROWSER_EXECUTABLE` to use an existing Chromium executable. The test
prints its temporary screenshot directory. `MX_TEST_CAPTURE_SOURCE=synthetic`
runs the same UI checks with a generated screen source when native tab capture
is unavailable; native capture is the default.

Module navigation is server-derived in MX 3.0. The client renders only the
effective modules and capabilities returned for the signed-in account; it does
not treat a hidden control as authorization. Search, reporting, record routes,
attachments, notifications, and preview tickets are independently enforced by
the Rust API using the same role-ceiling and explicit-grant intersection.

Record editing is stateless and optimistic. Opening or typing in a record does
not claim a lock or start a heartbeat. The editor retains the loaded record
revision, sends only changed fields, automatically accepts server merges for
unrelated updates, and presents stored/proposed choices for same-field
conflicts. Attachment mutations advance the same record revision.
