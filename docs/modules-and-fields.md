# Modules and fields reference

[Documentation index](README.md) · [Project overview](../README.md)

## Modules and Dynamic Record Structure

Each **Module Structure** is the source of truth for one part of a deployment's business model.

A fresh installation can contain multiple independently configured modules, and each module can contain zero fields.

Administrators add only what that deployment requires.

### Account module isolation

Module authorization is the intersection of two server-side layers:

1. the account's system role defines the maximum capability ceiling; and
2. an explicit account grant selects which modules and capabilities that person
   actually receives.

Non-administrator accounts are default-denied. A module without an explicit
grant is absent from navigation and is rejected by its schema, record, search,
report, attachment, notification, and collaboration-linked-record APIs.
Administrators always retain full access to active modules so an accidental
grant edit cannot lock the deployment out of configuration.

Each account/module grant can independently allow view, create, edit,
archive/delete, structure configuration, reports, and attachments. A grant can
reduce its role's permissions but cannot exceed them. For example, an Editor
grant requesting delete access is still denied because the Editor role ceiling
does not allow record deletion.

Module-access saves include an optimistic revision key. If two Administrator
sessions edit the same account concurrently, exactly one save advances the
revision; the stale save receives HTTP `409 Conflict` and must reload before it
can overwrite the newer decision. Every successful access change is written to
the audit log and sent only to the affected account so its navigation refreshes
without exposing the assignment to other users.

### Supported field types

```text
Text
Long Text
Integer
Decimal
Formula / Calculated
Relationship / Linked record
Lookup from linked records
Rollup / Aggregate linked records
Date
Boolean
Select
Auto Number
File Attachment
```

### Conditional forms and option values

Open **Administration → Modules & fields → select the module → Form rules**.
Choose a field and click **Configure form rules**. This is module configuration,
not an attendance-specific feature; it works for requests, inventory, registration
and other custom modules. No additional server or Internet service is required.

- Configure **Show field when**, **Allow editing when**, and **Require a value
  when**, using named control fields and nested **All (AND)** / **Any (OR)** groups.
  Controls can compare text, dates, numbers, booleans and choices; relationships
  can be checked for empty/non-empty. Numeric controls support threshold comparisons.
- For a Select field, choose **Dropdown**, **Radio buttons**, or **Checkboxes
  (one choice)**. Expand an option to change its display label, conditionally
  offer it, or **Fill these fields when selected**. The stored option value remains
  stable. One selection can fill a code, classification, numeric rate or other
  ordinary fields; these are editable copies, not live calculations.
- Existing Boolean fields can share an **Exclusive checkbox group** name.
  Selecting one unchecks the other members. Optionally require one applicable
  choice. For new designs, one Select field is usually clearer than separate booleans.

For example, a configurable request-type choice can show one set of detail fields
for Off and another for Leave. Give each detail field a **Show field when** rule;
use **Require a value when** only where needed. Relationship auto-fill can copy an
employee's classification into an ordinary field, which can then control the
available request categories. Choose the actual field names and values for your
module; MX does not hardcode these examples.

Switching choices can clear ordinary values that no longer apply, and invalidated
dependent choices. MX asks before clearing existing/unsaved details or queued files.
Cancelling keeps the details; confirming affects only the local edit until Save.
Previously saved values stay in record history. Stored attachments are never
automatically deleted. Tables keep stable columns and show **Not applicable**
for inactive branches. An inline change that needs additional required details
opens the record form with that choice already selected.

The server independently applies these rules to creates, PATCH/PUT saves and version
restoration, inside the existing atomic record/version transaction. Unrelated stale
edits still merge. Changes to a controlling field or an automatically cleared/copied
destination receive normal conflict checks. Keeping the latest conflicting branch
details cancels that branch switch without discarding unrelated unsaved fields.
Uploads check applicability before transfer and again before metadata admission;
a branch change during upload cannot attach a file to the wrong branch.

Limitations: calculated/lookup/rollup/auto-number fields and attachments support
visibility only, and cannot act as rule controllers. Automatic option values cannot
write relationships or calculated/file fields. File bytes are uploaded separately,
so dynamic attachment requirements are not part of an atomic record save. These
are form rules, **not field-level secrecy/access controls**; existing module and
record permissions still govern reads and history. Rule saves do not migrate old
rows; review existing records and version restores when tightening requirements.
Configuration rejects missing/archived controls, unsupported types, competing
automatic-value controllers and cycles, with named errors inside the rules dialog.
Archiving a referenced field names the rules that must be changed first.

### Record-column summaries

For cash-flow, rental and similar modules, open **Administration → Modules &
fields**, select the module and use **Column summaries**. Choose a calculation
for each integer, decimal or formula field: **Sum / total**, **Average**,
**Minimum**, **Maximum** or **Count values**. Set the display decimal places
(0–12) and click that field's **Save summary**. Auto-number fields support count
only. None is the default; normal record values and version history are unchanged.

The record table displays the result below the corresponding visible column.
These summaries cover **all matching records across all pages** and follow the
current search and advanced filters; trashed records are excluded. Hiding a
column hides its footer. Count includes non-empty numeric values, including zero.
Empty sums/counts show zero; empty averages/minimum/maximum show a dash.
Totals refresh after saving edits and receiving live changes. Formatting does
not change MX's existing numeric storage precision or create accounting entries.

Stored numbers are aggregated in SQLite. Formula totals recalculate the current
formula definition in bounded server-side batches, which can take longer on
large modules than direct numeric sums. Relationship/lookup/linked-rollup fields
are not eligible for these table summaries. Module access checks apply as for
the record list; the rows, result count and summaries share one read snapshot.

### Relationships, lookups and rollups

For first-time setup, use the [step-by-step operator guide](relationships.md).
It explains the UI labels, single/multiple selection, and the difference between
a Relationship picker, a live Lookup and an editable saved copy. In particular,
two independent Relationship fields do not automatically fill each other.

In **Administration → Modules & fields**, select the source module and add a
**Relationship / linked record** field. Choose the target module, its display
field, and whether multiple records may be selected. For example, a Billing
record can link to a Patient without copying the patient's name into every bill.
Stored values are stable record IDs; display labels follow subsequent renames.
Forms and table-cell editing provide searchable, paginated selectors.

Add a **Lookup** to display another field from those selected records, or a
**Rollup** to count them or calculate sum, average, minimum or maximum of a
numeric linked field. These values are read-only and calculated when records are
loaded, and refresh immediately when their relationship selection changes in
an editor. Empty links produce count/sum zero; average/min/max are empty. Lookups
return a list of values, even for a single link. A record's **Related records**
section lists accessible records referencing it, with paged navigation.

#### Select a name once and fill other fields

This section describes **editable copies**. The destination Classification must
already be Text/Select (or another compatible ordinary value field), **not a second
Relationship**. For automatic read-only classification display, follow
[Method A: Lookup through Issuer](relationships.md#method-a-display-classification-automatically-with-a-lookup).

Edit the **Issuer** relationship in **Administration → Modules & fields**.
Choose the employee module and **Name** as its display field. Under **Fill fields
automatically**, choose **Add field mapping** and map employee **Classification**
to the inventory's existing **Classification** field. Add more mappings for
department, job title or other compatible fields, then **Save field**.

Selecting an employee immediately fills the configured fields in the record
form and inline table editor. Changing the selection replaces the copied values;
clearing it clears them. Copied fields remain editable and are saved as ordinary
record values with normal revision/conflict checks and version history. They
are **snapshots**, not live lookups: later employee edits do not alter already
saved inventory records. Use a **Lookup** instead when the latest source value
must always be displayed. Existing records are not backfilled automatically.

Auto-fill requires a single-selection relationship and existing writable
destination fields: text/long text/select, integer/decimal, date or boolean.
Choose compatible types; dropdown source values must be valid destination
options. Calculated fields, attachments, auto numbers and other relationships
cannot be destinations. Missing source values become empty; required fields
still need a valid value before saving. Each destination has one relationship
responsible for filling it. Removing/archiving mapped fields requires removing
their mappings first. This does not convert existing field types or delete data.

Access to both modules is required to select and read source values. Once copied,
values belong to the destination record and follow that module's permissions;
revoking source access does not redact previously saved copies. Failed previews
show a retry action and block saving partial automatic values. Obsolete responses
are cancelled, and typing into a destination while loading is preserved.

The read-only `POST /mx/v1/relationships/{field_uid}/preview` endpoint takes
`{"value":"<linked record UUID>"}` (or `null` to clear) and returns `values`
for copied destinations plus `computed` for dependent lookups/rollups. Record
create/PATCH requests fill omitted mapped destinations inside their normal
SQLite write transaction; explicitly supplied destination values override the
copies. PATCH includes these destinations in ordinary field-conflict checks.

#### Understand why a field cannot be archived

Click **Archive** on a custom field to open an archive-check dialog. It identifies
the exact module and field still using it—for example, **Inventory → Issuer**—
explains whether it is a display field, lookup source, auto-fill mapping or formula
dependency, and tells you what to change. **Open field settings** switches to the
correct module and focuses/highlights that field. Storage and record dependencies
have their own shortcuts. Unsaved field/module/storage edits prompt before leaving
the module.

The same dialog shows stored relationship links (including trashed records),
attachment ownership and N1 folder/prefix use. Resolve those dependencies, then
click **Check again**. **Archive field** remains disabled until clear. Failures
and retries stay inside the dialog rather than appearing above the scrolled page.
The server checks again on commit; historical values and files are not deleted.
Formula source fields are also protected so archiving cannot break their active
calculated fields. Archived dependent fields no longer block configuration checks,
but a relationship's stored links must still be cleared before archiving it.

`GET /mx/v1/admin/schema/fields/{field_uid}/archive-check` is administrator-only
and returns the field, module, `can_archive` and named `blockers` with reasons and
resolutions. Rejected archive updates return `error: "field_archive_blocked"` and
the same `archive_check` structure, plus a readable summary for other API clients.

Example API field configurations:

```json
{"field_type":"relationship","config":{"target_module_uid":"<module UUID>","label_field":"name","multiple":false}}
{"field_type":"relationship","config":{"target_module_uid":"<employee module UUID>","label_field":"name","multiple":false,"auto_fill":[{"source_field":"classification","target_field":"classification"}]}}
{"field_type":"lookup","config":{"relationship_field":"patient","target_field":"department"}}
{"field_type":"rollup","config":{"relationship_field":"selected_bills","target_field":"amount","function":"sum"}}
```

Send a record UUID for a single relationship, an array of UUIDs for a multiple
relationship (up to 100), or null/an empty array to clear an optional link.
Changes participate in the normal record `base_revision`/PATCH merge and version
history. Two edits to the same relationship field conflict; unrelated fields
still merge. Lookup/rollup fields cannot be submitted as editable values.

Both modules must be accessible. MX validates target membership and active state
inside the write transaction. Hidden target IDs, labels, lookups and rollups are
redacted, not merely hidden by the UI. Linked records cannot be trashed while
active records reference them. Clear links or trash their sources first; restore
targets before restoring sources. Reconfigure/archive dependent calculated
fields and clear stored links before archiving a relationship. Active relationship
definitions prevent deleting their target module.

Current scope: rollups aggregate **explicitly selected outgoing links**, not all
inverse child records automatically. Derived values are current-data views, not
historical computed snapshots. Relationships/lookups/rollups cannot be sorted,
raw-searched, used as N1 path components or referenced by formulas. Existing
dashboard/report SQL measures do not aggregate these on-demand computed fields;
CSV exports omit all three new field types rather than exposing raw linked IDs.
This does not turn MX into a SQL administration tool.

### Common field behavior

Depending on the field type, a field can define:

```text
Required
Unique
Searchable
Sortable
Show in records table
Table priority
Field order
Type-specific configuration
```

Fields use stable internal UIDs so labels and presentation can evolve without changing the identity used by stored records.

Archived fields can remain represented in historical data instead of forcing destructive schema changes.

---

