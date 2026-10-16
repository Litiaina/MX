<div align="center">

  <img
    src="./assets/litiaina_icon.png"
    alt="Litiaina"
    width="150"
    height="150"
  />

  <h1>Litiaina MX</h1>

</div>

Self-hosted, modular digital workplace platform. **Current candidate: 1.0.0-rc.3.**

This is a candidate for the new 1.0 release line, not the final stable release.
The earlier 4.x labels remain development history. Read the
[current candidate notes](docs/releases/1.0.0-rc.3.md) and, for a 4.x deployment,
the [one-time version-reset instructions](docs/releases/1.0.0-rc.1.md#upgrading-a-4x-development-installation)
before upgrading an existing installation.

MX combines configurable records, relationships, dashboards, collaboration,
voice/video calls, screen sharing, Drive and browser-based Office editing.
Administrators define the business modules and fields; MX does not prescribe
an inventory, attendance or registration schema.

## Start here

- **New administrator:** [Set up your first modules](docs/getting-started.md).
- **Select a person once and display their classification:**
  [Relationship setup, step by step](docs/relationships.md).
- **Show different fields for different choices:**
  [Conditional forms, step by step](docs/conditional-forms.md).
- **Import spreadsheet rows:** [Import, inspect failures and resume](docs/importing-records.md).
- **Install or upgrade MX:** [Deployment guide](docs/deployment.md).
- **Manage signed updates:** [Maintenance windows, offline packages and rollback](docs/updates.md).
- **All guides and reference material:** [Documentation index](docs/README.md).

The operator guides explain the actual UI labels, what to enter, what each
setting means and how to verify the result. Detailed technical documentation
lives in `docs/`, not in this README.

## Runtime

MX runs as the Rust server with SQLite and an N1 v4 object-storage connection.
Deploy the matching **MX executable and `frontend/dist`** together, alongside
your configuration and TLS credentials.

Ordinary LAN use does not require public Internet access. The Office engine is
bundled locally and runs on the user's browser; no separate Office server is
required. Prepared offline Office copies have additional browser/TLS requirements.
See [Office operation and limits](docs/office.md).

## Build from source

Build tooling needs Rust/Cargo and Node.js 24 or newer. These are build-time
requirements, not additional services for a packaged deployment.

Run from the repository root:

```bash
cd frontend
npm ci
npm run build
cd ..
cargo build --release --locked
```

The executable is `target/release/litiaina-mx`. Use `--version` or `-V` to print
its built-in version without starting the server or creating configuration. Follow the
[deployment guide](docs/deployment.md) before starting a new installation;
empty configuration templates are not a working deployment.

For an already configured development workspace, rebuild the frontend after UI
changes and use `cargo run --locked`. Restart an old running process to load new
backend routes.

## Documentation

| Area | Guide |
|---|---|
| Daily operation and first modules | [Getting started](docs/getting-started.md) |
| Relationships, lookups and auto-fill | [Operator instructions](docs/relationships.md) |
| Conditional fields and choices | [Operator instructions](docs/conditional-forms.md) |
| Deployment and configuration | [Deployment](docs/deployment.md), [configuration reference](docs/configuration.md) |
| Records, files, search and storage | [Modules](docs/modules-and-fields.md), [records](docs/records-and-search.md), [N1 storage](docs/n1-storage.md) |
| Drive, collaboration and Office | [Drive](docs/drive.md), [collaboration](docs/collaboration.md), [Office](docs/office.md) |
| Dashboards, reporting and administration | [Reports](docs/dashboards-and-reports.md), [administration/security](docs/administration-and-security.md) |
| Integration and development | [HTTP API](docs/http-api.md), [development](docs/development.md), [architecture](docs/platform.md) |

## Release and validation

Read [CHANGELOG.md](CHANGELOG.md) for released and unreleased changes, and
[TESTING.md](TESTING.md) for actual checks, known failures and deployment gates.
A successful build or a version number is not production certification.
The Office editor in particular has unresolved document/browser acceptance
limitations documented in those guides.

Back up SQLite and retain your deployment configuration before upgrading.
Do not replace a deployment database or delete N1 objects as a troubleshooting
step. See [migration and release guidance](docs/migration-and-releases.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and
[development instructions](docs/development.md).
