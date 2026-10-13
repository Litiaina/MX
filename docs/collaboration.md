# Collaboration and call reference

[Documentation index](README.md) · [Project overview](../README.md)

## Real-Time Collaboration

MX includes authenticated real-time synchronization for multi-user deployments.

Connected browsers maintain a WebSocket session with the MX server. When records,
attachments, schema configuration, dashboard configuration, deployment settings,
or account presence change, MX can notify connected clients immediately instead
of waiting for manual refreshes.

### Reliable live connection

The browser reconnects indefinitely if the WebSocket is interrupted.

Reconnect behavior uses bounded exponential backoff: the delay can increase during
an outage, but MX does not permanently give up after an arbitrary retry count.
After a successful reconnect, the client resynchronizes authoritative state from
the normal HTTP API so events missed during the outage do not leave the interface
stale.

MX also uses heartbeat/liveness handling so long-running browser sessions can
detect dead connections and recover.

### Network failure and save recovery

Temporary connection errors and server outages preserve session credentials.
At startup, a failed session check shows a reconnect screen; only a definitive
authentication rejection signs the user out. Live connections have a silent-link
watchdog, an opening deadline, jittered reconnect backoff, and recovery triggers
when the browser returns online or visible. Reconnection refreshes records,
dashboards, modules, notifications, collaboration, preferences, and call discovery
from the HTTP API. Existing record rows remain mounted during refresh.

JSON API requests have finite deadlines covering headers and the response body.
Reads retry transient failures up to three attempts. Other writes are not
automatically replayed unless their endpoint explicitly supports it. Record and
message creation carry a client operation UUID, bound to the account, target,
and submitted payload in SQLite in the same transaction as the write. Replaying
that operation returns the existing resource. Deploy the updated frontend and
backend together; the replay guarantee requires the updated backend.

Buffered downloads, document previews, profile photos, and custom sounds also
detect a stalled response body (60 seconds without a chunk). Failed reads retry
from the beginning, never treating a partial buffer as a complete file. Buffered
transfers have a 30-minute total deadline; native video/range playback does not
use this buffer or total deadline.

A pending new-record operation is retained with its draft in the current tab's
session storage. A lost save response does not mean the server failed to save:
retry the retained operation to reconcile it. Inline cell editing keeps the
original revision loaded when editing began, even after a background refresh.
Concurrent edits still use the normal field-conflict decisions.

Chat composers retain separate in-memory drafts per conversation. After an
uncertain send, **Retry send** reconciles the original message operation;
an attachment error does not create another base message. Switching conversations
is paused only while a send is actively running. Chat drafts and local file
selections are not a persistent outbox and do not survive a full page reload.

Uploads have progress/stall and processing deadlines. MX retries transient N1
standard-multipart failures at the same part index with identical bytes, and
checks the committed version after an uncertain finalize response. This is not
browser-to-MX resumable upload: after an interrupted attachment request, inspect
the attachment list before retrying. Local file selections cannot be restored
after a page reload. One-shot uploads and arbitrary mutations are not blindly
replayed. N1 streams use a 60-second read-idle timeout, not a total video duration
limit; connect/authentication (including waiting for a shared token refresh) and
non-streaming mutations have separate bounds. Invalid HTML upload acknowledgements
are reported as uncertain writes, never as a successful attachment.

Call signaling retries lost offers and answers without rebuilding healthy media
connections. Duplicate descriptions and obsolete sessions are ignored; candidate
replay and staggered ICE restart handle interrupted negotiation/connectivity.
This cannot replace a TURN relay on restrictive networks or guarantee usable
audio/video while the underlying network is disconnected.

### Voice, video, and screen sharing

Conversation members can start or join a voice call, enable a camera, and share
a screen without leaving MX. The call dock remains available while the user
navigates between records, dashboards, and collaboration. Media is transported
between browsers with WebRTC; authenticated MX HTTP and private WebSocket events
provide membership enforcement, room presence, and offer/answer/ICE signaling.

Direct conversations produce an incoming private-call card; spaces produce a
group-call card with the caller, space, participant count, and voice/video join
actions. A private call rings for at most 30 seconds and uses a persistent
operating-system alert when enabled; a group call uses one softer chime and a
non-blocking alert. Answering, declining, joining from another signed-in tab, or
the call ending stops the active alert. Ongoing calls are restored after page refresh or live reconnection and
remain visible as call badges in the conversation list. The call dock includes
mute, deafen, camera, screen sharing, input/output device selection, elapsed
time, focused-video layout, and native full-screen viewing for shared screens.
When the browser and selected capture source support it, screen sharing also
transports tab/system audio on a sender independent from the microphone; users
must enable the browser's **Share audio** option in the capture picker.
Active microphone tracks drive live speaking rings on participant tiles, and the
local device panel includes an outgoing preview, input-level meter, microphone
and camera selection, browser-supported speaker routing, and an audible output
test. These indicators are calculated locally from call media and do not send
recorded audio to the MX server.

Camera and screen-share playback stays attached to the current video track
during unchanged participant heartbeats and microphone-only updates. When the
video source changes, a bounded still frame covers the transition until the
replacement is decoded. Each tile uses one playing video, avoiding overlapping
video layers and fullscreen crossfade effects. The microphone uses a separate
audio-only playback stream that is preserved when the video source changes.
The device-panel video preview also avoids restarting for microphone-only changes.
In group calls, each viewer receives source updates independently. A newly
received video track updates its tile even if the sharing status arrived first;
adding video to a voice call starts negotiation immediately, and queued changes
resume when the current offer is answered. Static screen shares can be displayed
from their first decoded frame without waiting for another screen change.

Your own shared-screen preview is hidden by default; **Show my preview** and
**Hide my preview** control local viewing without stopping the stream sent to
others. Starting a share keeps the current call layout instead of automatically
enlarging your own screen. Native fullscreen of your own share displays sharing
status with the local preview paused, preventing the shared screen from
recursively capturing its fullscreen video. Remote viewers can still watch the
share in fullscreen. The capture picker requests exclusion of the current MX
tab where the browser supports it, and the device panel hides shared-screen
self-preview as well.

Call access follows conversation membership. A removed member is immediately
evicted from an active call, signaling is addressed only to joined members, and
inactive participants expire automatically. MX reconnects call signaling and
rejoins a participant after long background-tab timer throttling.

Each browser join uses a separate call-session identifier. Quitting and
rejoining creates fresh peer connections, while delayed leave or WebRTC
signaling from the previous join is ignored. This prevents new microphone,
camera, and screen-share tracks from attaching to a stale peer. Obsolete ICE
candidates from an offer rollback or restart are discarded without failing the
call; a genuinely failed connection triggers ICE restart and renegotiation.
Peer pairs choose one deterministic initial offerer and use atomic browser
offer/answer application so simultaneous discovery cannot produce negotiation
glare during join or rejoin.

Browser media APIs require a secure context. Use HTTPS for deployed MX instances
(`localhost` is the browser's development exception). Direct/LAN candidates can
work without an ICE service, but reliable calls across NAT, VPN boundaries, or
restrictive firewalls require deployment-owned STUN/TURN configuration. This
release uses a bounded peer mesh rather than an SFU, so `max_participants` is
limited to 12 and deployments should choose a lower limit when client upload
bandwidth is constrained.

### Account presence

Authenticated sessions contribute to deployment-wide presence information.

MX can show whether an account is currently:

```text
Online
Offline
```

Multiple browser tabs for the same account are treated as one online account for
presence display.

Presence is informational. It is **not** a record lock.

### Parallel editing without record locks

MX deliberately does not lock an entire record when somebody opens or edits it.

For example, these operations can happen at the same time:

```text
User A -> changes Status
User B -> changes Routed To
User C -> uploads a supporting document
```

Independent changes can be committed without forcing the users to wait for one
another.

When a record opens, the client retains its loaded record revision and edits
locally without leases, presence heartbeats, or server editing sessions. Save
sends only changed fields plus that base revision. The server atomically merges
fields whose last change predates the base and returns a field conflict only
when the same logical field changed. Conflict responses contain the stored and
proposed values so the user can choose either one. Attachment additions merge;
same-attachment operations are conflict checked and advance the same revision.

This prevents silent lost updates while preserving parallel work on unrelated
fields and attachments.

### Live events

The real-time layer can publish compact events such as:

```text
record.created
record.fields.updated
record.deleted

attachment.created
attachment.deleted

schema.updated
storage.updated
dashboard.updated
deployment.updated

presence.user.online
presence.user.offline
sync.required
```

WebSocket events are notifications, not the durable source of truth.

```text
WebSocket -> something changed
HTTP API  -> fetch authoritative state
SQLite    -> authoritative structured state
N1        -> authoritative attachment bytes
```

This keeps reconnection and recovery deterministic.

---

