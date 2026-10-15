# MX updates

[Documentation index](README.md) · [Deployment](deployment.md) · [Release policy](migration-and-releases.md)

MX supports **operator-driven installation** and **automatic installation inside
a configurable daily maintenance window**. Both use the same signed packages,
independent worker, final local SQLite backup and failed-activation rollback.
There is no additional application server, Office server, container runtime,
Python, Node or Cargo dependency on a packaged MX deployment. Activation currently
requires **Linux with systemd**, the deployment's service manager.

Updates are opt-in. An unconfigured or internet-disconnected MX continues normal
LAN operation. Offline packages use exactly the same verification and activation
path as repository downloads.

## Using the Updates page

Open **Administration → Updates**. Only administrators can inspect update jobs,
upload packages, change installation policy or request installation.

1. **Check for updates** reads published releases from the configured repository.
   Only a newer, signed, compatible 1.x release in the permitted channel can be
   offered. A GitHub tag, source archive or unsigned binary is not an update.
2. **Download and verify** streams the package to local storage and checks its
   signing key, CPU/platform, every installed file's size/SHA-256, executable
   version and frontend identity. The running server stays untouched.
3. **Install now…** asks for confirmation before scheduling a restart. Connected
   workspace users receive a save/restart warning. Calls and transfers will be
   interrupted; save open forms and Office documents first.
4. Watch **Update activity**. Byte progress is shown only when measurable;
   backup, restart and health verification use indeterminate activity, not an
   invented percentage. The job continues if you navigate away or close the
   browser. During restart the page reconnects without submitting the job again.
5. A verified successful activation reopens requests. Failed activation restores
   the previous binary/frontend **and the final pre-update SQLite snapshot**.
   The original failure and rollback result remain inspectable in job history.

Use **Refresh status** after a lost acknowledgement. Do not repeatedly press
Install: one durable job is allowed at a time, and operation IDs bind retries to
their original action/version. A queued job that never starts usually means the
operator has not started the independent path/worker units.
Status refresh and live reconnection preserve an unsaved installation policy,
selected package and pending request identity while the Updates page stays open.
Leaving the page does not save a draft policy; press **Save policy** first.

### Installation policy and maintenance times

Choose **Only when an administrator installs** to keep activation operator-driven.
Choose **Automatically during maintenance** and configure start/end hours and
the window's fixed UTC offset. **Asia/Manila uses UTC+08:00.** Windows can cross
midnight; equal start/end hours are rejected instead of meaning “all day.” Fixed
offsets do not follow daylight-saving changes automatically.

Automatic checks are scheduled independently of installation. At a permitted
window the worker can download, verify and install a trusted newer release.
Activation is deferred while a call is active, if readiness cannot be inspected,
or if the window ends before activation begins. An explicit **Install now…** is
the operator's override of the schedule and can interrupt calls.

A failed/rolled-back activation is not installed repeatedly without review. Read
its job history and retry explicitly or publish a corrected, newly numbered
release. Automatic mode is not permission to bypass signatures, platform/channel
checks, disk-space checks, final backups or health verification.
Transient package transfers have a 15-minute backoff and at most three attempts
before operator review; a successful explicit restage clears that transfer
backoff. Failed-activation blocks survive trimming the visible job history.

### Offline deployment

Transfer the publisher's `.mxupdate` package to the administrator's machine.
Select it under **Offline update package → Upload and verify**. The package is
streamed into the updater's local staging area; it is not uploaded to N1. Only
packages signed by a deployment-trusted public key can become installable.
Use **Install now…** after staging an offline package. Automatic scheduling uses
the configured release-feed offer; uploading a file does not silently opt it
into automatic activation.

## What activation protects

The worker is a separate systemd service, not a child that dies in MX's service
control group. Activation proceeds as follows:

```text
verify immutable slot → warn clients → block new API requests → drain requests
→ stop MX → create + integrity-check + fsync final SQLite snapshot
→ atomically switch matching binary/frontend → start MX + migrate
→ verify version, managed frontend, readiness and SQLite integrity
→ persist commit decision → reopen writes
```

The maintenance marker survives MX and worker restarts. Business APIs, sign-in,
uploads and other normal requests receive `503` with `Retry-After` while blocked;
updater status and non-sensitive readiness remain available. New candidate
background storage cleanup does not run until activation is accepted.

The activation journal is outside SQLite, so restoring SQLite cannot erase the
rollback decision. If the worker is interrupted before commitment, its next run
restores the recorded verified backup and previous slot before reopening access.
If restoration/health verification cannot complete, maintenance stays enabled
with an inspectable failure. Do not delete its marker to conceal the error.

Once commitment is durable, later worker runs **do not rewind the database**.
Users may already have saved new work. A later ordinary crash is not a failed
upgrade and must not trigger restoration of an old snapshot.

The SQLite backup contains metadata/references, not N1's actual file bytes. The
updater does not reset N1, rewrite object keys or manage N1 upgrades. Retain a
separate N1 recovery strategy. This is not physical power-loss certification or
a guarantee that unsaved client-side edits survive a restart.

## One-time deployment setup

Do this during an operator-controlled maintenance window. Do not run it against
a live database with active writers, and do not overwrite working files with
empty example configurations.

The examples use a dedicated unprivileged **mx** account, `/srv/mx` for existing
deployment configuration/data, `/srv/mx-updates` for releases/backups/jobs and
`/srv/mx-updater` for a stable copy of the same MX executable. Substitute the
deployment's exact paths consistently. The worker intentionally refuses root.

1. Install this updater-capable RC manually first. Stop MX and any other writers;
   take and verify the [final rollback snapshot](deployment.md#taking-the-final-rollback-snapshot).
2. As the dedicated service user, create:

   ```text
   /srv/mx-updates/
     slots/bootstrap/litiaina-mx
     slots/bootstrap/frontend/dist/...
     current -> slots/bootstrap
   /srv/mx-updater/litiaina-mx
   /srv/mx/
     mx.config
     mx.env
     mx-updates.json
     existing SQLite/TLS files at their existing configured paths
   ```

   Copy the **matching** new executable/frontend into `slots/bootstrap`, retaining
   the existing working release separately. The stable worker executable is a
   separate copy, not a link through `current`, and runs the same MX program with
   `--update-worker`. No database/config/secrets belong in a release slot.
3. Set `mx.config`'s `static_site_path` to
   `/srv/mx-updates/current/frontend/dist`; leave database/N1/secret/TLS settings
   alone. The managed service's `ExecStart` must be exactly
   `/srv/mx-updates/current/litiaina-mx`, with `WorkingDirectory=/srv/mx`.
4. Copy [mx-updates.json.example](../deploy/mx-updates.json.example) to
   `/srv/mx/mx-updates.json`. Replace the public key placeholder with the
   publisher's **independently verified** key ID and Ed25519 public key. Choose
   `stable` or `rc`; RC permits stable releases plus release candidates. Do not
   copy a private signing key to the server. Keep updater state private to the
   dedicated service user and do not expose its directory through static hosting.
   The candidate's [publisher public-key file](../deploy/mx-release-signing-public-key.json)
   contains the `trusted_keys` entry, not a private key; independently verify it
   before trusting a release downloaded from the same channel.
5. Copy/adapt the [systemd templates](../deploy/systemd/) into that user's unit
   directory. They assume user-service mode (`scope: "user"`). Enable lingering
   for a dedicated unattended user manager if needed; that one-time host setup
   requires the deployment administrator. Then, as that user:

   ```bash
   systemctl --user daemon-reload
   systemctl --user enable --now mx.service
   systemctl --user enable --now mx-update-worker.path mx-update-worker.timer
   ```

6. Verify sign-in, records, N1 files and **Administration → Updates**. It should
   show the installed version, managed readiness and worker activity. Default
   installation policy is manual. Select automatic mode/window only when ready.

`MX_UPDATE_CONFIG` selects an explicit configuration file. Otherwise MX looks
for `mx-updates.json` in its working directory. A missing optional file disables
updates; an explicitly configured invalid file stops startup with an error.

The timer wakes the lightweight worker every minute; repository requests occur
only when the configured check interval is due (default six hours). The path
unit starts queued work promptly and notices jobs left by an interrupted worker.
Never make the worker `PartOf`/`BindsTo` the MX service it must stop.

The stable worker copy is intentionally retained while application slots change;
an unaccepted candidate cannot replace its own rollback controller. Compatible
format-1 application updates do not need a worker replacement. Updater-protocol
changes or fixes to the worker itself require an explicit operator replacement
of `/srv/mx-updater/litiaina-mx` with a qualified build. Do not assume ordinary
application activation patches that separate copy automatically.

For **system-service mode**, set `scope: "system"`, install adapted system units
with `User=mx` and use normal `systemctl` rather than `--user`. The unprivileged
worker needs authority for only start/stop/reset-failed of its exact MX unit;
[the optional Polkit example](../deploy/mx-update.polkit.rules.example) documents
that boundary. Review it for the host before installing. Do not run MX/the worker
as root or grant general `sudo systemctl`. N1 must remain outside this authority.

### Configuration reference

- `root`, `deployment_dir`, `service`, `scope`: exact managed deployment and unit.
- `repository`: `owner/name`, default `Litiaina/MX`, the canonical GitHub repository
  (the older `Litiaina/aris` git remote redirects there).
- `channel`: `stable` or `rc`; current updater accepts the official 1.x line only.
- `trusted_keys`: key IDs mapped to 64-character hex Ed25519 public keys.
- `check_interval_seconds`: 60–604800; default 21600.
- `notice_seconds`: pre-restart save warning, default 60, maximum 3600.
- `drain_timeout_seconds`: 1–3600; default 60.
- `health_timeout_seconds`: 1–600; default 90.
- `max_package_bytes`, `max_unpacked_bytes`: updater artifact safety budgets,
  default 2 GiB / 8 GiB. These are **not Drive upload limits**. Check free disk
  space for packages, retained slots, rollback snapshots and recovery copies.
- `auto_install`: optional initial policy, default false. If true, configure
  distinct `maintenance_start_utc_hour` / `maintenance_end_utc_hour` values.
  Saved administrator policy in `policy.json` takes precedence over these defaults.
- `feed_url`: optional operator-managed HTTPS release feed with the GitHub list
  schema. Normal mode uses GitHub's 100 most recent published releases. A local
  test fixture may explicitly enable `allow_loopback_http`; do not use it for
  public/untrusted network traffic.

GitHub public releases need no token. An optional narrowly scoped
`MX_UPDATE_GITHUB_TOKEN` can increase authenticated API quota; provide it to the
**worker service**, not the browser. It is sent only to the GitHub API, never to
download origins. Private GitHub asset downloads are not qualified; use signed
offline packages or a deployment-managed HTTPS feed for that case. Normal
transport verifies HTTPS. Self-signed TLS is accepted only for the worker's
fixed, verified-local readiness address, with proxies disabled. A private
per-activation HMAC binds readiness to the exact service PID, version, database
identity and frontend. The secret is never sent over HTTP.

Old slots, verified backups, failed-state recovery copies and job history are
retained for diagnosis/rollback; inspect storage use and archive old recovery
points deliberately. No automatic retention deletion is claimed. Partial package
downloads and failed extraction scratch space are disposable, not backups.

## Publishing a signed update

These are release-maintainer operations on a trusted build/signing machine, not
buttons that let an MX administrator invent a new trust key.
Run `./litiaina-mx --help` for the native command syntax; diagnostic commands do
not start MX or initialize deployment files.

Generate a release signing key once, outside any package-input directory:

```bash
./litiaina-mx --update-keygen /secure/offline/mx-release.pk8
```

The new file is created with restrictive permissions and is never overwritten.
The command prints only the public key/key ID needed by deployment operators.
Keep the private key offline/backed up and distribute the public key through a
trusted channel. Rotation requires an explicit operator trust change.

Build and qualify a matching release, then prepare a clean directory containing
`litiaina-mx`, `frontend/dist`, `LICENSE` and optionally README/changelog/docs/assets.
Never point the packager at a deployment directory containing SQLite, credentials
or TLS material. The packager rejects those inputs and symlinks.

For an x86-64 GNU/Linux build:

```bash
./litiaina-mx --update-package /build/mx-release /secure/offline/mx-release.pk8 /output/mx-x86_64-unknown-linux-gnu.mxupdate rc
```

Use `stable` for a stable executable version. The command reads `--version` from
the built binary, signs the exact manifest bytes and emits the package plus
`mx-x86_64-unknown-linux-gnu.json`. It generates a matching frontend identity in
the archive without editing the input directory. Existing output files are not
replaced. Publish both assets under the exact `vVERSION` GitHub release tag, with
the correct prerelease flag. Other CPU/libc targets use their corresponding names.

Every installed file is covered by signed size/SHA-256 metadata. Verification
rejects extra/duplicate ZIP entries, path traversal, symlinks, special files,
missing required files, wrong platforms, modified payloads and downgrades.
The worker re-verifies staged bytes immediately before activation.

Do not relabel or overwrite a published candidate. Use a new RC number. The
historical 4.x development line is not eligible for 1.x autonomous updates;
transition it manually using the [release-line migration guidance](migration-and-releases.md).

## Testing and recovery

`npm run test:updates` is an explicit opt-in integration test. It builds a real
second RC from a disposable source copy, creates local signing keys and starts
uniquely named test-only systemd user units. It exercises release/network faults,
automatic windows, offline upload, concurrent install requests, post-migration
rollback, worker termination/recovery and browser UI. It needs Node/Cargo/systemd
only on the **test machine**, not on a normal packaged deployment.

Keep `MX_TEST_ARTIFACTS`, `TMPDIR`, `CARGO_TARGET_DIR`, `MX_TEST_BINARY` and
`MX_TEST_STATIC_SITE_PATH` on the external test drive as documented in
[TESTING.md](../TESTING.md). The test never uses a live N1 namespace, production
signing key, real account or physical audio device. Runtime unit registrations
are removed afterwards; diagnostic artifacts remain in its exact test directory.
The shared test build cache can contain its fixture RC binary afterwards; rebuild
the repository before treating that cache's executable as a distribution artifact.

If automatic rollback needs attention, leave maintenance enabled. Inspect
`state.json`, `activation.json`, the verified snapshot under `backups`, the
preserved failed state under `recovery` and the worker's systemd journal. Confirm
the exact configured database and service before following
[manual recovery](deployment.md#manual-rollback-during-failed-activation). Do not
restore an old database after normal writes have resumed without a deliberate
data-recovery decision.
