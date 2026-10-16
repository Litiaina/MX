# Deploying or upgrading MX

[Documentation index](README.md) · [Configuration reference](configuration.md)

This guide is for the deployment administrator. Module operators should start
with [Getting started](getting-started.md).

Current candidate: **1.0.0-rc.2**. The 1.0 line intentionally resets the earlier 4.x
development numbering; it is not the final stable 1.0.0. Existing installations
must follow the [one-time transition instructions](releases/1.0.0-rc.1.md#upgrading-a-4x-development-installation),
without deleting their database or N1 objects.

Managed RC.1 installations can upgrade through **Administration → Updates**;
see the [RC.2 notes](releases/1.0.0-rc.2.md). Do not reset the database or replace
deployment credentials during this upgrade.

## Deployment package and dependencies

Deploy a matching MX executable and `frontend/dist`, plus your `mx.config`,
`mx.env` and TLS certificate/key. MX uses SQLite for structured state and N1 v4
for file objects. There is no separately installed database server or Office server.

Rust/Cargo and Node.js 24+ are needed to build from source, not to run an already
packaged executable. Local network operation needs the browser to reach MX and
MX to reach N1; it does not require a public Internet connection. Browser Office
assets must be present in the deployed frontend package.

## Prepare a new installation

1. Use a dedicated deployment directory and place the MX executable and matching
   `frontend/dist` there. Relative configuration paths resolve from the working directory.
2. Prepare TLS certificate/key files for the hostname/IP the clients will use.
   Clients must trust the certificate for Service Workers and offline Office
   preparation. Dismissing a certificate warning is not equivalent to trust.
3. Configure `mx.config` for your bind address, ports, SQLite path, TLS paths and
   frontend location. The excerpt below shows the required sections; use your own
   port and storage choices.
4. Configure `mx.env` with strong deployment-specific secrets. Do not use the
   placeholder text below as actual credentials.
5. Configure and verify a dedicated N1 fragment for MX. The fragment identifier
   and raw secret are different values.
6. Start MX from the deployment directory and check its startup log. MX can
   generate configuration templates when absent, but empty templates do not
   supply working secrets or TLS credentials.

```ini
[server]
host=127.0.0.1
http_port=20001
https_port=21001
cert_path=cert.pem
key_path=key.pem

[database]
db_path=mx.db
pool_max_size=16
pool_min_idle=4
pool_connection_timeout_seconds=10
busy_timeout_milliseconds=10000

[jwt_token_config]
login_token_expiration=900
refresh_token_expiration=2592000

[static_site_hosting]
static_site_path=frontend/dist

[n1]
base_url=https://127.0.0.1:50001
fragment=<SHA-256 identifier of the dedicated MX fragment>
insecure_tls=false
attachment_max_size_mb=50
collaboration_file_max_size_mb=100
multipart_part_size_mb=16
```

`host=127.0.0.1` accepts local clients only. Set the appropriate interface address
for LAN access and restrict exposure with your deployment's network controls.
The N1 URL must be reachable **from the MX server**. Use `insecure_tls=true` only
where deliberately accepting an untrusted N1 certificate; trusting the deployed
certificate is preferable.

```env
JWT_SECRET=<strong random signing secret>
AUTH_KEYS=<strong random first-administrator setup key>
N1_MX_SECRET=<raw secret for the dedicated MX N1 fragment>
```

Keep `mx.env`, private TLS keys, database files and backups out of source control.
Restrict filesystem access. An offline/private network does not remove the need
for authentication, authorization and trusted TLS.

## Create the first Administrator

1. Open the configured HTTPS address in a browser.
2. On a deployment with no users, MX shows the first-administrator setup form.
3. Enter the bootstrap authorization key from `AUTH_KEYS`, name, email, password
   and matching confirmation. Use at least 12 characters for the password.
4. Click **Create Administrator**. MX signs in after successful creation.
5. Configure modules using [Getting started](getting-started.md).
6. Create operator accounts and grant only the modules/capabilities they need.

An existing deployment with users is not eligible for the empty-database setup
flow. Do not delete accounts or SQLite files merely to make that form reappear.

## Verify before accepting real work

- Sign in as both Administrator and a limited operator; check module grants.
- Create, edit, reopen and search a disposable test record.
- Test two-user edits and the conflict decision flow.
- Upload/download a small test file and confirm N1 authentication and byte equality.
- Verify the configured Drive quota and sharing policy.
- Test calls on the actual browsers, devices and networks in scope if calls are required.
- Create and verify a backup, and plan a restore exercise outside the live deployment.
- Read [TESTING.md](../TESTING.md) for remaining qualification gates. Office
  document-input/browser limitations are not resolved by a successful build.

## Upgrade an existing installation

The candidate includes an opt-in signed updater, offline package upload and
manual/maintenance-window installation. It needs one-time managed-slot, trusted
public-key and independent systemd-worker setup; an ordinary `cargo run` does
not silently grant service-replacement authority. Follow [MX updates](updates.md).
The steps below remain the manual deployment alternative.

1. Back up SQLite and retain configuration, environment and TLS credentials.
2. Read [CHANGELOG.md](../CHANGELOG.md) and [migration guidance](migration-and-releases.md).
3. Deploy the matching new executable and rebuilt frontend together.
4. Restart MX and inspect startup/schema-migration results.
5. Reload the browser and verify records, files, authentication and key workflows.

### Manual rollback during failed activation

Use this procedure only while normal writes remain blocked after a failed
upgrade. Restoring an older database after users have saved new work discards
that newer work; do not perform that as an unattended response to a later crash.

1. Stop the candidate MX service and all other processes that can write its
   database. Keep clients and background jobs excluded until validation finishes.
2. Identify the exact deployment database path and the verified pre-upgrade
   snapshot. Validate the snapshot's SQLite integrity before replacing anything.
3. Preserve the failed database for diagnosis. With every database connection
   closed, move its exact database file and any associated `-wal` / `-shm` files
   into a separate, access-restricted recovery directory. Do not use broad globs,
   delete the recovery files or copy a live database one file at a time.
4. Restore a copy of the verified standalone snapshot to the configured database
   path, with the deployment's original owner and restrictive permissions. Do
   not pair the restored database with WAL/SHM files from the failed candidate.
   Keep the original rollback snapshot untouched.
5. Restore the previous matching executable and frontend. Preserve configuration,
   credentials and TLS files; restore their pre-upgrade copies only if the
   attempted upgrade changed them. Leave N1's version and namespace alone.
6. Start the previous release under restricted maintenance access. Verify
   startup, authentication, record/history access and representative N1 file
   downloads. A SQLite restore cannot recreate missing N1 objects; use the N1
   recovery/backup process if storage objects are missing.
7. Reopen normal access only after validation. Retain the failed state, rollback
   snapshot and logs, and report the failed upgrade before attempting it again.

These are operator recovery steps, not a general restoration wizard. The
[managed updater](updates.md) enforces a write barrier, verified local snapshot
and paired application/database rollback during failed activation. Use this
manual procedure if its recovery cannot complete; leave maintenance enabled.

### Taking the final rollback snapshot

The **Create backup** UI can supply a preliminary verified backup before the
maintenance window. If writes continued afterwards, that is not the final
rollback point promised by a no-loss failed-update policy.

For the final point, stop MX and every database writer first. Copy the exact
database and any associated WAL/SHM files into an access-restricted recovery
directory while they are all quiescent. Retain that complete cold copy; do not
discard a WAL just because the main `.db` file exists. Using compatible SQLite
recovery tooling against a separate copy of that set, create a standalone
snapshot with the SQLite Backup API or `VACUUM INTO`, then verify the snapshot's
integrity. This can be done on an operator recovery machine and does not add a
normal MX server runtime dependency.

Do not proceed with the upgrade if the final snapshot cannot be validated.
MX does not yet provide a CLI offline-backup command or automate these steps.

Do not replace the existing database with an empty one, regenerate deployment
secrets casually, or alter existing N1 keys as part of an MX binary upgrade.
MX/N1 version numbers need not match: this implementation targets N1 v4.0.0 /
storage format 4. An MX upgrade is not an N1 storage migration.

Advanced N1, Drive quota/retention and WebRTC traversal settings are in the
[configuration reference](configuration.md). Build and isolated-test commands
are in [Development and testing](development.md).
