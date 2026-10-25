# Getting started with MX

[Documentation index](README.md) · [Deployment guide](deployment.md)

This guide is for an administrator configuring a new business module and an
operator entering records. It assumes MX is running and you can sign in. It
does not require deleting an existing installation or knowing SQL.

## 1. Understand the three building blocks

| Term | Meaning | Example |
|---|---|---|
| Module | A collection of records about one subject | Employees |
| Field | One piece of information collected in each record | Name, Classification |
| Record | One saved entry containing those fields | One employee |

A fresh deployment starts with no modules, business fields or records.
Administrators create the modules and fields the organization needs. The module
names, field labels and examples in this guide are not built-in MX settings.
Upgrades preserve existing modules, including any earlier generic Records module.

## 2. Create an Employees module

1. Sign in as an Administrator.
2. Open **Administration → Modules & fields**.
3. Click **New module**. This creates a separate module; it does not rename an
   existing module. The module library opens without selecting one for editing.
4. Enter **Module name:** `Employees` and **Singular name:** `Employee`.
5. Click **Create new module**. The configuration scope switches to Employees.

Always check **You are configuring** before changing fields. To configure another
module, choose **All modules → Configure [module name]**. Use the **Fields**,
**Module details**, **Permissions**, **Form rules**, **Column totals** and
**File storage** sections to focus on one task at a time.

## 3. Add the information an employee record needs

Use the new-field form in the Employees configuration. Its heading follows
your module's singular name.

1. Enter **Label:** `Name`, choose **Type:** `Text`, enable **Required** and click
   **Add field to Employees**. Leave **Key (optional override)** empty; MX generates it.
2. Enter **Label:** `Classification`, choose **Type:** `Select` and enter your
   actual classifications under **Options (comma separated)**. For example,
   `Regular, Contract`. Click **Add field to Employees**.

Text is suitable for free-form names. Select gives operators a list of allowed
choices. Classification is **not** a Relationship field in this example.

For a job-title workflow, use Position instead of Classification. The same
relationship steps apply; the labels are your choice.

## 4. Enter a test employee

1. Open **Modules**, search for **Employees** and open it. In expanded navigation,
   expand **Modules** and use its shortcuts or **Browse all modules**.
2. Click **+ New employee**.
3. Enter a fictional test name and choose a classification.
4. Click **Save Employee**.
5. Confirm the record appears in the table with the values you entered.

You now have a record another module can select. Relationships select business
records, not the sign-in accounts listed under **Administration → Accounts**.

## 5. Link an Inventory or Activity module

Follow [Relationships and automatic values](relationships.md). The simplest
setup for “select a name and show its classification” is:

| Field in the second module | Type | Operator behavior |
|---|---|---|
| Issuer | Relationship to Employees, displaying Name | Select the employee once |
| Classification | Lookup through Issuer to employee Classification | Displays automatically; no second picker |

Do not create a second Relationship for Classification if you want this result.
Use the editable-copy alternative only when the saved classification must be
independently editable or remain unchanged after later employee updates.

## 6. Give operators access

Create their MX accounts in **Administration → Accounts**, then assign module
access. A non-administrator's role alone does not make every module visible.
Users selecting employees need access to both Employees and the destination module.

For ordinary data entry, allow the relevant read/create/edit capabilities. Grant
attachment access if their workflow includes files. Do not make an operator an
Administrator just to make a missing module appear.

See [Accounts and module permissions](administration-and-security.md#access-levels)
and [module isolation](modules-and-fields.md#account-module-isolation).

### Organize navigation when you have many modules

The collapsed navigation rail has one **Modules** button, not one icon for every
module. It opens a searchable library. The expanded sidebar offers an expandable
Modules menu with up to 12 matching shortcuts; use **Browse all modules** to see
the library. Search matches module names, descriptions and groups. The library
shows at most 40 modules per page and offers a group filter and Previous/Next.

Administrators control placement through **Administration → Modules & fields →
Configure [module name] → Module details**:

- **Navigation order:** lower numbers appear first. Use 10, 20, 30 so another
  module can be inserted between them later. Equal positions sort by name.
- **Navigation group:** use a name such as Human Resources or Finance. Operators
  can filter the library by this group.
- Choose **Save changes**. Ordering and groups are saved in MX; they do not
  rename modules, move records, or grant additional access.

The library only offers modules supplied by the signed-in account's access
checks. Creating an account does not automatically grant it every module.

## 7. Verify before using real records

- Sign in as an operator, not only as Administrator.
- Confirm only the assigned modules appear.
- Select two different test employees in turn; verify Classification follows
  the selected employee without another selection.
- Save, reopen and verify the record.
- If two users edit, unrelated fields should merge; the same changed field should
  produce a conflict requiring a decision, not silently overwrite the other user.

Add [conditional fields](conditional-forms.md), totals, attachments and dashboards
after the basic entry flow works. Save each configuration step and verify it
before adding another feature.

## If something does not work

| Symptom | Check |
|---|---|
| A module is missing for an operator | Account module grants and role capabilities |
| The employee picker is empty | Employee records exist and the operator can read Employees |
| Classification asks for a second name selection | Its type is Relationship; the automatic display method needs Lookup |
| Classification is absent from an auto-fill destination list | It must be an ordinary writable field, not Relationship or Lookup |
| Save does not complete | Read the validation message; some configuration save errors currently appear above the field cards |
| Saved changes appear to disappear | Confirm the selected module, successful save and whether a conflict was returned |

Do not wipe records as a configuration troubleshooting step. Existing field
types are not converted automatically by changing a label or adding a mapping.
