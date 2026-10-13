# Relationships and automatic values

[Documentation index](README.md) · [Getting started](getting-started.md)

**Goal:** choose an employee's name once in Inventory or Activity, then show that
employee's classification or position without selecting it again.

This guide starts from a new module configuration. Employees, Inventory, Issuer
and Classification are example labels, not built-in application requirements.
Use your own labels if they differ.

## Choose the result you need

| Result | Configure Classification as | Can the operator edit it separately? | Later employee changes |
|---|---|---|---|
| Show the selected employee's current classification | **Lookup** through Issuer | No | Reflected when refreshed/reloaded |
| Save a classification copied at entry time | **Text or Select**, filled by Issuer | Yes | Do not change the saved copy |

Start with **Lookup** when the requirement is simply “display the selected
employee's classification.” Use an editable copy when you need a historical
classification for that transaction. Choose one method for the destination field.

If Classification must be an ordinary stored cell for field searches, formulas
or CSV export, use **Method B**. The current implementation excludes Relationship,
Lookup and Rollup fields from CSV exports; their calculated values are not stored
as historical cell snapshots. See the [reference limitations](modules-and-fields.md#relationships-lookups-and-rollups).

**Do not make both Issuer and Classification separate Relationship fields.**
That creates two independent record selectors; it does not make one fill the other.

## Before configuring

In Employees, create **Name** as Text and **Classification** as Text or Select.
Create at least one test employee record. Administrators and operators must have
appropriate access to both Employees and the destination module.

These employee records are separate from MX sign-in accounts. A module can be
named Users, but that does not make it the built-in Accounts administration page.

## Method A: display Classification automatically with a Lookup

### Step 1 — Create the name selector

1. Open **Administration → Modules & fields** and select Inventory in **Change module**.
2. In the new-field form, enter **Label:** `Issuer`.
3. Set **Type:** `Relationship / linked record`.
4. Set **Linked module:** `Employees`.
5. Set **Display field:** `Name`.
6. Leave **Allow multiple linked records** unchecked.
7. Leave **Fill fields automatically** empty for this method.
8. Click **Add field to Inventory**.

The relationship is now the field where the operator selects a person by name.
Choosing Display field = Name changes the picker label; it does not copy Name
into a separate text field.

### Step 2 — Add the automatic display field

1. Still in Inventory, enter **Label:** `Classification` in the new-field form.
2. Set **Type:** `Lookup from linked records`.
3. Set **Relationship:** `Issuer`.
4. Set **Linked field:** the employee's `Classification` field.
5. Click **Add field to Inventory**.

The Lookup reads Classification from the employee chosen in Issuer. The operator
does not fill or select the Lookup separately.

### Step 3 — Test the entry form

1. Open Inventory and click **+ New inventory**.
2. In **Issuer**, search/select a test employee.
3. Confirm **Classification** displays that employee's classification automatically.
4. Choose a different employee; confirm the displayed classification changes.
5. Save the record, reopen it and verify the selection and display.

The expected flow is **select Issuer → Classification appears → save**.
Lookup is a current-data view, not an independently editable historical copy.

## Method B: copy Classification into an editable saved field

Use this instead of Method A if the value must remain as entered or the operator
must be able to override it.

1. Create Inventory's **Classification** field as **Text**, or as **Select** with
   options matching the classifications in Employees. **Do not use Relationship
   or Lookup for this destination.**
2. Create or edit Inventory's **Issuer** Relationship with **Linked module = Employees**,
   **Display field = Name** and **Allow multiple linked records unchecked**.
3. Under **Fill fields automatically**, click **Add field mapping**.
4. Set **Linked source field:** `Classification`. This is the field in **Employees**.
5. Set **Fill this field:** `Classification`. This is the writable field in **Inventory**.
6. Click **Save field** for an existing Issuer, or **Add field to Inventory** for a new one.
7. Open a new Inventory record, select Issuer and verify Classification fills.
8. Save and reopen the record. The classification is stored as an ordinary record value.

Only add more mappings if you actually want more values copied. A field such
as Item, Notes or Stock used is simply another field in the module; its presence
in the list does not mean you must select it.

Changing or clearing Issuer replaces or clears its configured copies. You can edit
the copied values before saving. Later changes in Employees do not rewrite existing
Inventory copies. Existing records are not backfilled when a mapping is added.

## What “Allow multiple linked records” means

It controls **how many records one relationship cell can select**:

- **Unchecked:** one issuer/employee for this Inventory record.
- **Checked:** several issuers/employees in that same cell.

It does **not** control how many people can use MX, edit a module, or edit records
concurrently. It is not a permission setting.

Leave it unchecked for one employee → one classification. Editable auto-fill
requires one selected source record. Multiple selection is useful for a deliberately
multi-valued relationship; Lookups can return multiple values and Rollups can
aggregate those explicitly selected records.

## Why the field you want is missing from auto-fill

Auto-fill can write only ordinary value fields: **Text, Long text, Select,
Integer, Decimal, Date or Boolean**, with compatible source types.

It cannot write another Relationship, a Lookup, a Rollup, a Formula, an Auto Number
or a File Attachment. Unsupported fields are currently excluded from the destination
list; the UI does not yet explain each excluded field there.

If Classification is absent and only an unrelated field is offered, check
Classification's **Type**. Do not select the unrelated field merely to get past
the configuration. The usual correction is to choose Method A or B and create the
appropriate destination type in the fresh module.

Existing field types cannot be converted simply by relabeling them. Do not delete
existing links or records to force the mapping to work. Back up and plan a migration
before changing a populated schema.

## Troubleshooting

| What happens | Meaning and action |
|---|---|
| Classification has its own searchable picker | It is another Relationship. For Method A, create it as Lookup through Issuer instead. |
| Classification is missing from **Fill this field** | Check that it is Text/Select or another compatible writable type. |
| **Add field mapping** is disabled | Choose a linked module, leave multiple selection off, and create an eligible destination field. |
| Copying into Select fails | The employee's value must be one of the destination Select's configured options. |
| Source/destination types do not match | Use compatible fields; text is not a number and a record link is not text. |
| The picker has no employees | Add employee records and check module read permissions. |
| A Lookup is empty | Select an employee and check the source field has a value and is accessible. |
| A copied value does not follow a later employee edit | Expected for Method B. Use Lookup for a current-data display. |
| Existing records remain unchanged after adding a mapping | Expected; mappings do not automatically backfill old records. |
| **Save field** appears not to work | Configuration validation may currently appear above the field cards. Read the message before retrying; it is not necessarily a network failure. |

For technical scope, permissions and API behavior, see the
[relationship reference](modules-and-fields.md#relationships-lookups-and-rollups).
