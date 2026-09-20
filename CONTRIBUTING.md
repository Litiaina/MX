# Contributing to MX

Thank you for your interest in MX.

MX is open source under the **Apache License 2.0**, but the upstream MX project is intentionally **maintainer-directed** and does **not accept external code contributions**.

## Contribution Policy

The official MX repository does not accept:

- pull requests;
- unsolicited patches;
- external code contributions;
- documentation changes submitted as pull requests;
- dependency-update pull requests;
- refactors submitted from outside the maintainer team;
- feature implementations submitted from outside the maintainer team.

Pull requests opened against the upstream repository may be closed without review or merge.

This is an intentional project-governance decision. Public source availability does not require the upstream project to operate as an open-contribution project.

## What You May Do

The Apache License 2.0 allows you to use, study, modify, and redistribute MX under the terms of that license.

You are welcome to:

- inspect the source code;
- build MX yourself;
- use MX in your own deployments;
- maintain your own fork;
- modify MX for your own requirements;
- redistribute your modifications in accordance with the Apache License 2.0.

A fork does not become part of the official MX project unless the MX maintainer independently chooses to incorporate equivalent work.

## Bug Reports and Feedback

Bug reports and technical feedback are welcome when the repository's issue tracker is enabled.

Reports are most useful when they include:

- the MX version or commit;
- operating system and environment;
- clear reproduction steps;
- expected behavior;
- actual behavior;
- relevant logs with secrets removed.

Submitting a report does not imply that a fix, feature, or change will be accepted or implemented.

Please do **not** attach unsolicited replacement source files or patches with the expectation that they will be merged upstream.

## Feature Requests

Feature requests may be considered as feedback, but MX development remains directed by the project maintainer.

There is no guarantee that a requested feature will be implemented, scheduled, or accepted into the official project.

## Security Reports

Do not publish sensitive vulnerability details, credentials, private keys, authentication tokens, database contents, or deployment secrets in a public issue.

If the repository provides a private security-reporting mechanism, use that mechanism for security-sensitive reports.

## Forks

Forking is fully supported by the Apache License 2.0.

If you maintain a modified version of MX, clearly distinguish your fork from the official upstream project and comply with the applicable Apache License 2.0 requirements.

The upstream MX repository is not obligated to merge changes from forks.

## Why MX Uses This Model

MX is developed as a cohesive information-system platform with tightly connected behavior across:

- the Rust backend;
- the dynamic Record Structure;
- SQLite state and migrations;
- N1 object storage integration;
- authentication and authorization;
- real-time WebSocket synchronization;
- field-level concurrency handling;
- reporting and dashboards;
- the unified web interface.

The project therefore keeps upstream implementation decisions under a single maintainer-controlled development process.

This model keeps the source open while preserving a consistent architectural direction for the official MX distribution.

## License

MX is licensed under the **Apache License 2.0**.

The license governs your rights to use, reproduce, modify, and distribute the software. This contribution policy only describes how the **official upstream MX repository** is maintained.

In short:

> **MX is open source, but upstream development is not open contribution.**

You may use it, study it, modify it, and fork it under the Apache License 2.0, but the official MX repository does not accept external contributions.
