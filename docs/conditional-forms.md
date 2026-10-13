# Conditional forms: show fields for the selected choice

[Documentation index](README.md) · [Getting started](getting-started.md)

**Goal:** an operator chooses one type, then sees only the details relevant to
that type. This is configurable for any module, not just attendance.

Configure relationships first if the form also selects an employee. Conditional
forms control field behavior; they do not replace that relationship setup.

## Start with one choice field

For your OFF/LEAVE example:

1. Open **Administration → Modules & fields**, select your module and add a field.
2. Set **Label:** `Attendance type` and **Type:** `Select`.
3. Enter **Options (comma separated):** `OFF, LEAVE`.
4. Enable **Required** if each record must choose a type; save the field.
5. In **Form rules**, select Attendance type and click **Configure form rules**.
6. Under **Choice appearance → Display as**, choose **Dropdown**, **Radio buttons
   (one choice)** or **Checkboxes (one choice)**. Click **Save form rules**.

These appearances all store one selected value. Choosing LEAVE replaces OFF;
you do not need two independent Boolean fields for a new design.

### If the OFF/LEAVE field already exists

Reuse it; do not create a duplicate choice field. For example, if Inventory
already has **Status** with options OFF and LEAVE, use **Status** wherever the
steps below say Attendance type. Keep the selector visible for both options.

In that setup, **Status** is the OFF/LEAVE selector itself. If you later need a
separate approval/completion status, add a clearly named detail field such as
Approval status with its own choices; do not make the selector depend on itself.

Existing File Attachments can receive the OFF visibility rule without deleting
their stored files. Add Start date and End date only if those fields do not already
exist. Keep unrelated fields, such as an existing transaction Date, unchanged.

## Your OFF fields: attachment, dates and status

Create these fields in the same module as Attendance type. The Status options
are your organization's choices; examples such as Pending or Approved are not
predefined MX workflow states.

| Label | Type | Type-specific setting |
|---|---|---|
| OFF attachment | File attachments | Choose whether multiple files are allowed |
| Start date | Date | No special type configuration needed |
| End date | Date | No special type configuration needed |
| Status | Select | Enter your status options, comma separated |

Click **Add field to [your module]** after defining each field.

### Configure OFF attachment

1. In the **Form rules** section, choose **OFF attachment** under **Field to configure**.
2. Click **Configure form rules**.
3. Check **Show field when**.
4. Click **Add condition**.
5. Set **Control field:** `Attendance type`.
6. Set **Comparison:** `Equals`.
7. Set **Value:** `OFF`.
8. Click **Save form rules**.

### Configure the other three OFF fields

Repeat the same eight steps for **Start date**, **End date** and **Status**. Each
field gets the same condition: **Attendance type / Equals / OFF**.

**Expected result:** selecting OFF displays all four fields. Selecting LEAVE
hides those four fields. This setup does not yet define any LEAVE-specific fields;
add those separately with Attendance type / Equals / LEAVE when you know what
LEAVE needs to collect.

The current UI configures the **detail field**, not a central list under the choice.
Select OFF attachment, Start date, End date or Status in the rules panel.
Do not put the OFF visibility condition on Attendance type itself.

### Make a visible detail mandatory

For an ordinary field that must always be filled while visible, enable **Required**
in its field settings and click **Save field**. The visibility rule makes that
requirement apply only when the field is visible/editable. For example, a required
Start date with the OFF visibility rule needs a date for OFF, not for LEAVE.

Use **Require a value when** only when a visible field needs different requirements
under different conditions. File Attachments support **Show field when** but not
that conditional-requirement switch. A File Attachment's ordinary Required flag is
checked by the entry form while visible; file-byte upload remains a separate
operation, not an atomic server-side condition on the record save.

## What the three rule switches do

| Setting | Operator result |
|---|---|
| Show field when | The field appears only while the condition matches. |
| Allow editing when | The field remains visible but can be changed only while the condition matches. |
| Require a value when | Save needs a value while the condition matches and the field is visible/editable. |

**All conditions (AND)** means every condition must match. **Any condition (OR)**
means at least one must match. A simple OFF/LEAVE setup needs one condition per
rule and does not require nested groups.

Zero and false are values, not empty entries. Requiring a Boolean value does
not mean it must be checked true. To require one selected choice, use a required
Select field or an exclusive Boolean group.

## Fill extra cells from the selected option

This is optional; it is not needed simply to show the four OFF fields.
For example, LEAVE can also set a code or a default paid flag:

1. Create ordinary destination fields, such as **Code** (Text) and **Paid** (Boolean).
2. Configure **Attendance type → Form rules**.
3. Expand LEAVE under **Options and automatic values**.
4. Under **Fill these fields when selected → Add a destination**, select Code
   and enter the value to copy, for example `LV`.
5. Add Paid and choose Yes / true if appropriate.
6. Configure OFF's defaults separately, then click **Save form rules**.

These values are your organization's configuration, not built-in meanings of
LEAVE or OFF. Copies are ordinary, editable record values. Display labels can
change while stored option values remain stable.

## Test the operator experience

1. Open a new record and choose OFF. Verify the attachment, dates and Status appear.
2. Choose LEAVE. Verify those four OFF-specific fields disappear.
3. Choose OFF again, enter dates/status and queue a small test file.
4. If you enabled Required, try omitting a required detail and verify validation.
5. Save, reopen and verify the dates, status and file.
6. Switch to LEAVE. Confirm the warning names the ordinary details that will be cleared.
7. Cancel the warning; verify OFF and its details remain selected.
8. Confirm the change, save and inspect history; the previous saved values remain
   in the earlier version. Stored attachments are not automatically deleted.

Start date and End date collect dates; this rule does not enforce End date ≥
Start date. Form visibility is not a full attendance scheduling/approval workflow.

Text/numeric controls can also determine visibility. A typed change may hide a
detail immediately; MX asks before clearing that detail on Save. A concurrent
user's newer dependent edit must produce a conflict before a stale branch change
can clear it. Keeping latest branch details cancels that branch switch.

## Existing separate checkboxes

For an existing design with two Boolean fields, configure each field with the
same **Exclusive checkbox group** name. Selecting one unchecks the others.
Optionally enable **Require one applicable choice in this group**.

The group name is an identifier you choose, such as Attendance type. This feature
is independent of “Allow multiple linked records” on Relationship fields.

## Limits and troubleshooting

- Calculated fields and File Attachments support visibility only. File uploads
  are separate operations; conditional file-byte requirements are not atomic
  with the record save. Hidden stored files are retained.
- Calculated/Lookup/Rollup/Auto Number fields and attachments cannot control a rule.
  For a classification-based condition, use an ordinary copied Text/Select value
  rather than a live Lookup. See [editable relationship copies](relationships.md#method-b-copy-classification-into-an-editable-saved-field).
- A circular-dependency error means two or more fields depend back on one another.
  Check the named fields and remove the backward condition or copy mapping.
- Adding rules does not automatically rewrite old records. Review existing
  records and version-restoration behavior when adding stricter requirements.
- Show/hide rules are not field-level read permissions. Access remains governed
  by the module and record permission layers, including history.

See the [form-rules reference](modules-and-fields.md#conditional-forms-and-option-values)
for supported comparisons, validation and concurrency behavior.
