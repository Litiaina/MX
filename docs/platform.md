# Platform and architecture

[Documentation index](README.md) · [Project overview](../README.md)

## Overview

**MX** is a self-hosted, modular digital workplace platform. It brings structured
information management, team collaboration, real-time calls, and personal/shared
file storage into one authenticated workspace.

Its information-management core remains schema-driven: deployments are not locked
to one predefined record format. Calling MX only a general-purpose information
system no longer describes the full product; that is one capability within the
broader workplace platform. MX is not a predefined ERP suite or a claim of Google
Drive/Discord feature parity.

A fresh MX deployment starts with **no starter module, mandatory business fields or predefined dashboard statistics**. Administrators create modules, give each module its own fields and permissions, and build the information model through the application itself. Existing legacy Records modules are preserved during upgrades. Forms, record tables, search, attachment handling, N1 storage paths, dashboard widgets, statistics, and exports adapt to that configuration.

MX is therefore not a hard-coded document tracker, correspondence registry, inventory system, ticketing system, or workflow application.

It is the reusable system underneath those possibilities.

A deployment can use MX for:

- document and correspondence tracking;
- procurement and request records;
- inventory and asset information;
- incident and maintenance records;
- referrals and operational workflows;
- research or administrative datasets;
- office-specific registries;
- other structured information systems that can be represented as records and fields.

The core application stays the same while the deployment-defined schema changes.
People also use the same workspace to message their teams, meet through private
or group calls, share screens, and organize/share files through MX Drive.

---

## Core Principle

MX separates **platform responsibilities** from **business structure**.

```text
MX platform
├── identity
├── authentication
├── authorization
├── persistence
├── validation
├── search
├── attachment handling
├── N1 object storage integration
├── personal Drive and shared-file access
├── team collaboration and membership
├── call signaling and media controls
├── reporting
├── dashboard rendering
├── audit logging
└── backup safety

Deployment configuration
├── modules and module identities
├── module-specific fields and field order
├── module access matrices
├── validation and file-attachment fields
├── module-specific N1 folder layouts
├── table presentation
├── dashboard widgets
├── statistics
└── reporting dimensions
```

MX does not need to know what **Office**, **Department**, **Subject**, **Status**, **Action Taken**, **Control Number**, or any other organization-specific concept means.

If a deployment needs one of those concepts, the Administrator creates it as a field.

---

## Architecture

```text
Browser
   │
   │ HTTPS / JSON
   ▼
MX Server
Rust + Axum + Tokio
   │
   ├──────────────► SQLite
   │                structured state
   │                schema
   │                records
   │                users
   │                audit
   │                reports
   │                Drive metadata, versions, quotas and sharing
   │                backup metadata
   │
   └──────────────► N1
                    attachment bytes
                    immutable Drive file objects
                    verified database backups
```

### Browser

The browser interface is a Svelte 5 + TypeScript application built to static
assets in `frontend/dist` and served by the Rust process. It provides:

- Dashboard
- independently configured modules
- Collaboration channels and direct messages
- Notifications
- Administration
- attachment preview
- search and filtering
- account management
- personal theme, accent color, exact font size, interface scale, density, refresh, and notification preferences
- schema configuration
- reporting and exports
- real-time synchronization
- online/offline account presence
- parallel multi-user record editing
- personal/shared Drive, paginated folders, file versions, and upload queues
- collaboration spaces, messaging, and file sharing
- voice/video calls, screen sharing, and device controls
- linked-record selectors, lookups and rollups between modules
- bundled client-side Office editing with opt-in offline document copies

### MX Server

The Rust server owns:

- API routing;
- authentication and authorization;
- schema validation;
- dynamic record validation;
- transactional database operations;
- attachment coordination;
- N1 integration;
- search construction;
- dashboard/report queries;
- audit events;
- database backup operations;
- authenticated WebSocket sessions;
- live change broadcasting;
- account presence tracking;
- stateless optimistic record concurrency;
- Drive ownership, sharing, quotas, versions, and durable cleanup jobs;
- membership-scoped call state and signaling (browser WebRTC carries media).

### SQLite

SQLite stores structured and queryable state.

Binary attachment data is deliberately kept out of SQLite.

### N1

N1 stores:

- original uploaded attachment bytes;
- attachment versions;
- collaboration file shares;
- immutable MX Drive file objects and versions;
- the uploaded deployment logo;
- verified SQLite backup objects.

The N1 object is authoritative for file content. Preview files are derived data and may be regenerated.

---

## Main SQLite State

Important MX tables include:

```text
mx_records
mx_modules
mx_module_permissions
mx_module_account_permissions
mx_module_access_revisions
mx_schema_meta
mx_fields
mx_record_values
mx_unique_values
mx_field_sequences
mx_attachments
mx_record_storage
mx_module_storage_layout_meta
mx_module_storage_layout_folders
mx_record_versions
mx_record_version_values
mx_record_version_attachments
mx_dashboard_config
mx_user_preferences
mx_notifications
mx_channels
mx_messages
mx_audit_log
mx_backups
```

The exact internal schema may evolve, but deployment-defined business fields remain data rather than Rust struct columns.

---

## Design Principles

MX follows these principles:

- start with a blank business schema;
- make the deployment define the information model;
- treat File Attachment as a normal configurable field type;
- keep business fields out of hard-coded Rust structs;
- use stable internal record and field identities;
- enforce validation and authorization on the server;
- allocate automatic numbers transactionally;
- perform search, filtering, pagination, and reporting server-side;
- keep large binary data out of SQLite;
- use N1 as authoritative object storage for attachment bytes;
- keep storage namespaces deterministic and stable;
- allow dashboard/statistics behavior to be built from the same schema;
- keep operational audit history separate from business completion semantics;
- verify database backups before treating them as valid;
- keep the WebSocket layer as live invalidation/notification rather than durable state;
- reconnect live clients indefinitely and resynchronize after outages;
- allow parallel record work without whole-record locks;
- protect same-field concurrent edits with a record revision and per-field
  last-change metadata, without edit locks or collaboration state;
- avoid recompilation when a deployment changes its business record structure.

---

## Philosophy

**MX is a configurable digital workplace, not a fixed business application.**

The platform provides shared mechanisms for managing information, communicating,
meeting, and storing/sharing files. Its record engine can store, search, secure,
audit, visualize, and preserve deployment-defined information.

The organization decides what that information means.

```text
Start blank.

Define the Record Structure.
Define File Attachment fields.
Define the N1 layout.
Define the dashboard.
Define the statistics.
Define the reports.

Connect people through collaboration spaces and calls.
Organize and share files through MX Drive.

MX becomes the digital workplace the deployment needs.
```

That boundary allows the same MX core to serve very different organizations and workflows without rewriting the application for every deployment.

