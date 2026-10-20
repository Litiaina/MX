# Calculations and record privacy

[Documentation index](README.md) · [Modules and fields](modules-and-fields.md)

These features are available in RC.5. They do not create
starter modules, add organization-specific fields, or change existing access
settings automatically. Configure and test them with sample records first.

## Combine name fields into one searchable name

For example, an **Employees** module already has First name, Middle name and
Last name. You want Requests to select one employee by their complete name.

1. Open **Administration → Modules & fields → Configure Employees → Fields**.
2. Add a field labelled **Full name**, with type **Formula / calculated**.
3. Under **Calculated value**, choose **Text / combine fields**.
4. Click **Add text part** three times. Choose First name, Middle name, then
   Last name. Leave **Between fields** as one space and **Skip empty fields** on.
5. Add the field. Existing employees are recalculated; a blank middle name
   produces `José Dela Cruz`, not a double space.
6. Configure Requests. Add or edit its Employee **Relationship / linked record**:
   Linked module = Employees, Display field = Full name. Save the field.

Users search the complete name in the Employee picker. The relationship stores
the employee's stable record ID, not the name, so renaming an employee does not
break the link. Full name can also be the source of an automatic field mapping.
An automatic mapping creates an editable snapshot; a relationship label follows
the linked record's current name. See [Relationships](relationships.md).

Text calculations combine saved scalar values, not lists, attachments or live
linked totals. They support up to 64 parts and a 64 KiB result. They are read-only
record values and are recalculated when source values are saved.

## Give choices a display label and a numeric value

A choice has three distinct properties:

| Property | Example | Purpose |
| --- | --- | --- |
| Stored value | `SL` | Stable record value used by imports and conditions |
| Display label | Sick leave | What people see when choosing it |
| Numeric value | `5` | What a numeric formula uses for that choice |

1. Create a Select field, such as **Kind**, with options `SL, VL`.
2. Open this module's **Form rules** section. Choose Kind and **Configure form
   rules**. Expand the SL option.
3. Enter its display label and optional **Numeric value for calculations**.
4. Save the form rules. Repeat for every non-empty choice used in arithmetic.
5. Add a numeric Formula field, such as Charge, using `kind * units`, where
   `kind` and `units` are the actual field keys shown in configuration.

If SL = 5 and Units = 2, Charge = 10. The record still stores Kind = `SL`.
Changing the display label does not change the stored code or numeric value.
Zero and negative numbers are supported. Missing ordinary numeric inputs are
treated as zero; a non-empty choice with no numeric mapping is an error, not an
assumed zero. Invalid arithmetic, non-finite results and circular calculations
are rejected. A failed configuration/recalculation does not partially commit.

Changing a choice's numeric value recalculates current local formulas. If a rate
must be fixed historically, store the agreed numeric amount in an ordinary
record field instead of using a live configuration value as a historical rate.
Record versions preserve their saved snapshots.

## Total requests linked to an employee

Example structure (names and keys are yours to choose):

| Module | Fields |
| --- | --- |
| Employees | Name, Allowance, Used, Remaining |
| Requests | Employee relationship, Kind, Units, Charge, Status |

1. In Requests, point Employee at Employees. Configure Charge as above.
2. In Employees, add **Used** with type **Rollup / aggregate linked records**.
3. For **Which linked records?**, choose **Records in another module that link
   to this record**. Choose Requests as Source module and Employee as the
   relationship back to Employees.
4. Choose Sum and Charge as the numeric source field.
5. To exclude pending/cancelled requests, enable **Only include records matching
   conditions**, add a condition, and set Status **Equals** Approved. This uses
   the stored choice code, which may differ from its display label.
6. Add **Remaining**, a Number formula: `allowance - used`.
7. Test one employee and one request: Allowance 15, SL 5, Units 1. Pending should
   leave Remaining at 15; Approved should make it 10. Changing Units to 2 makes
   it 5. Cancelling or trashing the request restores it to 15 on refresh.

This is a current calculation, **not** a subtraction performed every time a user
saves. Repeated saves/retries therefore do not deduct the charge repeatedly.
Count, average, minimum and maximum are available too. Sum/count with no matching
records returns zero; average/minimum/maximum returns empty.

Important boundaries:

- Totals include only source records, relationship fields, numeric fields and
  filter fields the viewing account may access. Lack of source-module access
  produces an unavailable result, not a misleading zero. Creator-scoped requests
  produce that creator's accessible subtotal, not an organization-wide balance.
- Give balance reviewers appropriate source access. Do not present an accessible
  subtotal as an authoritative organization-wide allowance.
- This does not approve requests, reserve allowance, prevent overdrafts or
  implement transactions across an approval workflow. Those are separate rules.
- Incoming totals use saved numbers or local numeric formulas. Recursive linked
  aggregates are not supported. Formulas depending on linked totals are live,
  read-only, and cannot be searched or sorted as stored columns.
- Reports use saved scalar inputs, not live linked formulas. Choose the saved
  source amount when building a report; MX rejects a live linked formula rather
  than silently grouping its missing stored value as zero/empty.
- Reload/refresh shows source changes. The displayed number is not a permanent
  ledger entry. Detailed CSV exports retain their saved-scalar scope; live
  linked totals are not an authoritative exported balance.
- Referenced relationships, source fields, filter fields and modules must be
  reconfigured before they can be archived/deleted or made incompatible.

## Limit a record or field to its creator

Module grants still come first. Creator access never grants entry to a module.

For whole records, configure the module's **Permissions → Record visibility**:
choose **Record creators and administrators**, then save its permissions.
For a single field, use **Fields → [field] → Field access** and save the field:

- **Everyone with module access**: normal module permissions apply.
- **Record creator and administrators**: other module members cannot read or
  write its value or access files attached to that field.
- **Administrators only**: reserved for system administrators, even when a
  different account created the record.

The creator is the authenticated account that first created/imported the record;
it does not change when another person edits it. The importer is the creator of
imported records, not the person named in a spreadsheet cell. Administrators
retain recovery/configuration access; these settings do not hide data from
server operators, database administrators or backups.

Privacy is enforced by MX's APIs, including lists, counts, search, filters,
summaries, history, reports, collaboration links, previews and live updates.
Sharing a record link does not bypass these controls. Already-issued attachment
preview URLs recheck current access when used. A downloaded/exported copy cannot
be recalled by changing permissions later.

For older records, MX uses actual creation history or successful-import evidence.
It never guesses ownership from the last editor. If creation evidence is missing,
opting into creator-only visibility leaves that record accessible to
administrators only. Inspect historical records before enabling this policy.
Existing module-wide visibility is unchanged unless you opt in.

A calculated field or conditional rule cannot expose a more restricted input
through a less restricted result. Protect dependent fields first, or remove the
dependency; MX names the conflicting fields. Mutually exclusive checkboxes must
share an access scope. Conditional writable fields and automatic option copies
also require matching access scopes so a public control cannot silently clear or
change another person's private value. Hiding a field using a form condition or table-column
setting is a presentation feature, not a security restriction.

## Verify before using live data

Create two ordinary test accounts and one administrator. Check both the full
record and table, search, export, history, record sharing and file previews from
each account. Exercise approval/edit/cancel/trash/restore and save retries on the
sample balance. Keep the normal backup and rollback procedure for deployment.
