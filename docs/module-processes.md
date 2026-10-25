# Module processes

Available in **MX 1.0.0-rc.9**. Processes are opt-in; upgrading does not enable
one for an existing module.

A module still owns its records, fields, permissions, relationships and files.
An optional **process** adds controlled stages and actions on those records.
For example, an opportunity can move from Lead to Proposal to Won, while a
purchase request can move from Draft to Awaiting review to Approved.

This is a reusable process foundation, not a claim that MX already implements a
complete CRM, ERP or financial accounting suite. Purpose labels describe the
configuration; they do not install hidden business logic.

## Configure a process

1. Open **Administration → Modules & fields**. Create a module or use its
   **Configure** button. Check the displayed module name before editing.
2. Add your business fields in **Fields**. For example, add Title (Text), Amount
   (Decimal), Supplier (Relationship), and Receipt (File attachments). These are
   suggestions, not mandatory built-in names. Existing fields can be reused.
3. Open **Process & actions**. On an unconfigured module, choose an editable
   draft: **CRM · opportunity pipeline**, **ERP · purchase-request approval**,
   **Accounting · expense-request approval**, or **Custom process**.
4. Set the **Process name**, **Queue title field** and **Starting stage**.
   A title field must be module-visible; restricted fields cannot become queue
   titles or action requirements. Otherwise another person's queue could reveal
   protected values or infer their presence.
5. Review **Stages** and **Actions**. Each action has a source stage, a destination,
   permitted account levels, a creator policy and optional requirements. Expand
   **Required fields** to select what must be present before that action.
6. Decide whether to **Require the process to restart after record edits**.
   Keep this on for approvals. The CRM draft leaves it off so updating an
   opportunity's details does not reset its sales stage.
7. Check **Enable this process**. If records already exist, explicitly confirm
   applying the process to them. Records with no decision history start in the
   starting stage; no new business records are inserted.
8. Select **Save process**. Unsaved changes are not active. Open the module and
   select **Work queue** to verify the configuration with real records.

Choosing a draft never creates a module, field or record, and never enables a
process by itself. Fresh installations remain fully blank. Existing modules have
no process unless someone explicitly configures and enables one.

Importing records does not automatically submit or approve them. They start in
the initial stage. A file required by a particular process action must be added
before that action, not supplied merely to import a spreadsheet row.

## Understand permission and requirement controls

- **Permitted account levels** are an additional restriction. A user also needs
  module View and Edit permission, including their individual account grant.
  Marking Viewer here does not grant Edit permission to a read-only viewer.
- **Any permitted editor** allows anyone who satisfies those permissions.
- **Only the record creator** limits an action to the verified creator.
- **A different person from the creator** prevents self-approval, including by
  administrators. Historical records with unknown ownership cannot satisfy this
  rule; unknown does not mean “someone else.”
- **Required fields** checks presence, not a business calculation or truth test.
  A numeric zero and a Boolean false are present values. A required attachment
  field must contain a stored file, not merely a file queued in an unsaved form.
  Calculated/lookup/rollup fields are not accepted as requirements in this phase.
- **Require an action note** requires an explanation. Notes are visible to people
  permitted to view the record. Do not put information there that should exist
  only in a restricted field.

Creator-only records remain creator-only in queues, actions, decision history
and notifications. Configuring a reviewer does not override row isolation. A
shared approval process therefore normally needs module-visible records and
carefully configured account/module grants, rather than creator-only visibility.

## Use a process day to day

Open the module and select **Work queue**. Filter by **Stage**, or check **Only
records where I may act**. This includes eligible records with incomplete
requirements so you can see what still needs attention. It is not an assignment
inbox: eligibility is derived from the configured permissions and creator policy.

Select **Review record**. Review the saved values first, then choose the action
in the **Record process** section. Enter a note if required and use the explicit
confirmation button. Disabled actions explain missing fields or creator-policy
restrictions beside their button. For data changes, use **Edit**, save, and
return to viewing before taking a process action.

**Decision history** shows who took each action, when, its note and its saved
record revision. **View decision snapshot** opens the existing record-history
viewer at that exact snapshot, subject to current record/field access checks.
History is paginated; **Older decisions** replaces the current history page.

The record creator and eligible next reviewers receive durable workflow
notifications, excluding the actor and honoring their **Workflow changes**
preference. Existing background-only desktop delivery rules still apply. These
notifications do not create a Collaboration conversation or send email.

## Three worked examples

| Process draft | Suggested user-created fields | Example route |
|---|---|---|
| CRM opportunity pipeline | Opportunity title, Customer relationship, Contact, Expected amount, Notes | Lead → Qualified → Proposal → Won or Lost |
| ERP purchase-request approval | Request title, Supplier relationship, Amount, Supporting files | Draft → Awaiting review → Approved, or returned for changes |
| Accounting expense-request approval | Expense title, Employee relationship, Amount, Receipt | Draft → Awaiting review → Approved, or returned for changes |

For either approval example, open **Submit for review → Required fields** and
select the information your organization requires, including attachments if
appropriate. For **Approve request**, keep a separate reviewer and require a note.
Test using two different accounts; testing self-approval with one administrator
should be rejected, not silently bypassed.

All stages, labels, actions and requirements are configurable. These examples
are not hard-coded attendance, procurement or organization-specific rules.
Relationships and automatic fills continue to connect modules as before.

## What changes after approval?

With restart-after-edit enabled, advancing a record's revision makes its
effective process stage the starting stage. This covers ordinary edits,
attachment changes and restoring older record versions. The interface explains
that the old decision does not approve the new values. Earlier decisions remain
intact in history; the next action records a new snapshot.

Recalculating saved formulas also advances the record revision when their stored
result changes. A decision snapshot freezes stored values and attachment
references, not live linked-record values. Lookups, incoming rollups, supplier
names or other linked records may change independently; their live values are
not a frozen accounting transaction. Copy an agreed value into an ordinary saved
field if it must be reviewed as part of this record's decision.

With this option off, the stage stays unchanged after edits, but the interface
still says that the previous decision refers to an older saved revision.

Process state is stored separately from editable fields. Creating a Select field
named Status, changing it through import, or calling the normal record update
endpoint cannot execute a process action. Conversely, a process action does not
silently update an ordinary Status field or make an existing rollup filter match.
The process stage is currently available in the work queue and record viewer,
not as an ordinary calculated record column.

## Reliability, changes and limits

Actions check the exact loaded record revision, process revision and configuration
revision. A stale action fails with an explanation; reload and review the latest
saved values before resubmitting. Network retries reuse a payload-bound operation
identifier. The decision, snapshot, state and durable notifications commit together
or roll back together. If the response is uncertain, use **Check and retry action**
or inspect decision history; do not assume an unacknowledged action succeeded.

This follows the deployment's existing SQLite/storage durability policy; atomic
transactions are not a power-loss guarantee or financial-system certification.
Keep verified backups and appropriate power/storage protection in place.

Configuration saves use revision checks too. Stable stage keys and the original
starting stage are retained after first save so existing records/history are not
silently reinterpreted. Labels can be renamed, and stages/actions can be added.
Historical labels are preserved in decision events. Disable a process to stop new
actions without deleting its records or history. Disabling does not remove field
dependencies; remove an action requirement/title reference before archiving a
field. The archive dialog links directly to **Process & actions**.

Queues and histories return at most 40 entries per page. Queues use record-ID
cursors and recheck current access. **Previous/Next** navigate a live dataset,
not an immutable export; concurrent changes may move records between stage
filters. New tables and indexes are initialized at startup. Read endpoints do not
initialize workflow state or acquire SQLite's write lock for every record.

Process actions themselves do **not** post money or reserve stock. Optional
[operational books](operational-books.md) now provide explicit exact-money journal
posting and stock reservations, and can require a completed process stage and
unchanged source revision. Ordinary Decimal/calculated fields still use floating
point and must not be represented as financial ledger money.

Tax/payroll, automatic field-to-posting mappings, linked-module writes,
assignment/SLA scheduling, arbitrary scripting and a drag-and-drop Kanban board
remain outside this phase. These require separate domain rules and qualification.
