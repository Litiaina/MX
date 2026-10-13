# Deploying or upgrading MX

[Documentation index](README.md) · [Configuration reference](configuration.md)

This guide is for the deployment administrator. Module operators should start
with [Getting started](getting-started.md).

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

1. Back up SQLite and retain configuration, environment and TLS credentials.
2. Read [CHANGELOG.md](../CHANGELOG.md) and [migration guidance](migration-and-releases.md).
3. Deploy the matching new executable and rebuilt frontend together.
4. Restart MX and inspect startup/schema-migration results.
5. Reload the browser and verify records, files, authentication and key workflows.

Do not replace the existing database with an empty one, regenerate deployment
secrets casually, or alter existing N1 keys as part of an MX binary upgrade.
MX/N1 version numbers need not match: this implementation targets N1 v4.0.0 /
storage format 4. An MX upgrade is not an N1 storage migration.

Advanced N1, Drive quota/retention and WebRTC traversal settings are in the
[configuration reference](configuration.md). Build and isolated-test commands
are in [Development and testing](development.md).
