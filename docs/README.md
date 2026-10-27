# MX documentation

[Project overview](../README.md)

Use the operator guides first. They describe the actual buttons and field types
without requiring knowledge of the API or database. Reference pages explain
implementation details and advanced configuration.

## Operators and module administrators

| Task | Start here |
|---|---|
| Create modules and enter your first records | [Getting started](getting-started.md) |
| Import Excel, ODS, CSV or TSV rows | [Spreadsheet import walkthrough](importing-records.md) |
| Select an employee once and display their classification | [Relationships and automatic values](relationships.md) |
| Combine names, calculate numeric choices and linked balances | [Calculations and privacy](calculations-and-privacy.md) |
| Limit records or fields to their creator | [Record and field access](calculations-and-privacy.md#limit-a-record-or-field-to-its-creator) |
| Understand “Allow multiple linked records” | [Single versus multiple selection](relationships.md#what-allow-multiple-linked-records-means) |
| Choose between Lookup and editable auto-fill | [Which method to use](relationships.md#choose-the-result-you-need) |
| Show different fields for different choices | [Conditional forms](conditional-forms.md) |
| Configure CRM pipelines, ERP or expense approvals | [Module processes](module-processes.md) |
| Design a module’s CRM, operations, document or accounting interface | [Module views and layouts](module-workspaces.md) |
| Post balanced journals or receive, reserve and issue stock | [Operational books](operational-books.md) |
| Show an OFF attachment, dates and status | [OFF/LEAVE worked example](conditional-forms.md#your-off-fields-attachment-dates-and-status) |
| Configure totals beneath numeric columns | [Column summaries](modules-and-fields.md#record-column-summaries) |
| Understand a field archive dependency | [Archiving fields](modules-and-fields.md#understand-why-a-field-cannot-be-archived) |
| Organize, upload and share files | [MX Drive](drive.md) |
| Message, call and share screens | [Collaboration](collaboration.md) |
| Enable background-only desktop alerts | [Desktop notifications](desktop-notifications.md) |
| Choose the product theme, compact views and personal text size | [Appearance and compact workspaces](appearance.md) |
| Open, edit, save or prepare an offline Office document | [Office editor](office.md) |

## Deployment administrators

- [Current candidate: MX 1.0.0-rc.10](releases/1.0.0-rc.10.md)
- [Published candidate: MX 1.0.0-rc.9](releases/1.0.0-rc.9.md)
- [Published candidate: MX 1.0.0-rc.8](releases/1.0.0-rc.8.md)
- [Published candidate: MX 1.0.0-rc.7](releases/1.0.0-rc.7.md)
- [Published candidate: MX 1.0.0-rc.6](releases/1.0.0-rc.6.md)
- [Published candidate: MX 1.0.0-rc.5](releases/1.0.0-rc.5.md)
- [Published candidate: MX 1.0.0-rc.4](releases/1.0.0-rc.4.md)
- [Deployment and upgrade procedure](deployment.md)
- [Signed updates, maintenance windows and rollback](updates.md)
- [Configuration reference](configuration.md)
- [Accounts, permissions, audit and backups](administration-and-security.md)
- [N1 storage layouts and frozen namespaces](n1-storage.md)
- [Migration and release notes](migration-and-releases.md)
- [Validation evidence and known limitations](../TESTING.md)

## Reference and integration

- [Modules, field types and permissions](modules-and-fields.md)
- [Records, attachments, numbering, formulas and search](records-and-search.md)
- [Dashboards, reports and CSV exports](dashboards-and-reports.md)
- [HTTP API](http-api.md)
- [Platform and architecture](platform.md)
- [Development and testing](development.md)
- [Changelog](../CHANGELOG.md) and [contributing](../CONTRIBUTING.md)

## Reading conventions

**Bold names** identify UI labels or buttons. Names such as Employees, Inventory,
Issuer and Classification are examples of modules and fields you create; they
are not built-in requirements. Replace them with your organization's names.

Administrator configuration defines the forms. Operators then enter records
through those forms. A business module named Users or Employees is separate from
**Administration → Accounts**, which manages MX sign-in accounts and permissions.
The same distinction applies if your custom business module is itself named Accounts.

These guides describe the checked-in implementation. A deployed executable and
frontend must be updated together for matching UI and backend behavior. Released
and unreleased changes are distinguished in the changelog; passing tests do not
remove the documented acceptance limits.
