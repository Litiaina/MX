# Accounting and stock books

Available in **MX 1.0.0-rc.9**. These optional books add exact-money journals and
stock reservations to your own modules. They
do not install an Accounting, Inventory or ERP module for you.

## Enable a book

1. Open **Administration → Modules & fields**. Search for your module and click
   **Configure _module name_**. Check the module name before changing settings.
2. Open **Accounting & stock**.
3. For accounting, enable **balanced journal entries**, enter your three-letter
   currency code, and choose its decimal precision. For inventory, enable
   **stock and reservations** and choose quantity precision.
4. Leave book operators as **Administrator** initially. Only add Manager or
   Editor when those users should see the **entire book**, not just their own
   records. They also need the module's **Read, Edit and Reports** permissions
   and an explicit account grant. Viewers cannot post.
5. Save. Open the module's records screen and click **Books**, or configure an
   Accounting / Inventory opening view under **Views & layout**.

Nothing is populated automatically. Existing modules, records, relationships and
permissions are not converted into accounts, opening balances or stock.
Saving settings starts retained book audit history: archive the module instead
of permanently deleting it afterward. Ordinary record edits remain available.

## First balanced journal

For a disposable example, create two chart accounts using **Add account**:

| Code | Name | Category |
|---|---|---|
| CASH | Cash | Asset |
| CAPITAL | Opening capital | Equity |

Then choose **New journal**, select a posting date and enter:

| Account | Debit | Credit |
|---|---|---|
| Cash | 1000.00 | _leave blank_ |
| Opening capital | _leave blank_ | 1000.00 |

Enter an explanation, click **Review & confirm**, inspect your entry and confirm.
MX says **Committed to MX** only after the transaction commits. This example
demonstrates the controls; it is not advice on your organization's chart or
accounting policy. Have your accountant define the real accounts and entries.

Amounts use decimal strings converted to checked, scaled integers. No silent
rounding, binary-floating-point money, exponent notation or thousands separators
are accepted. Each line has a debit **or** a credit; combine repeated accounts
into one line. A journal has 2–100 lines and must balance exactly.

**Accounts & balances** shows cumulative debits, credits and **debit minus
credit** per account. A credit balance is negative in this display; it is not
an automatically generated income statement or balance sheet. Accounts, items
and histories are fetched 40 at a time; **Previous/Next** navigate the book.

## Corrections and closed periods

Posted journals and their original lines cannot be edited or deleted. Open
**Journals → View journal** to inspect evidence. To correct a posting, choose
**Reverse**, enter a date and reason, then confirm. MX creates the opposite
journal, links it to the original and permits only one reversal of that original.
Enter the corrected transaction separately. A reversal cannot predate its source.

Administrators may set **Close posting dates through**. The cutoff blocks new
journals and reversals on or before that date and cannot move backwards. A
reversal of an older posting must therefore use an allowed later date. Currency
and money precision cannot change after the first posting.

## Stock example: receive, reserve, consume

1. Choose **Add stock item**: code `PAPER`, name `Paper`, unit `ream`.
2. Choose **Stock operation → Receive**, quantity `10.5`, and an explanation.
3. Choose **Stock operation → Reserve**, quantity `8`.
4. **Stock balances** now shows on hand `10.500`, reserved `8.000`, available
   `2.500` at the default three-decimal precision.
5. Open **Reservations**, choose **Release / consume**, select **Consume**, and
   explain why. On hand becomes `2.500`, reserved `0.000`, available `2.500`.

**Issue** uses only unreserved stock. **Release** returns a hold to availability
without changing stock on hand. **Consume** issues the held quantity. Holds
cannot be consumed or released twice; simultaneous requests cannot reserve or
issue more than is available. An existing hold can still be released after
inventory is disabled, but not consumed. All movements remain in **Stock history**.

Quantity precision becomes fixed after creating the first item. Opening stock
is an explicit receive operation, not a manually overwritten balance. Corrections
use explained compensating movements; posted movement history is not rewritten.

## Connect a saved record and approval

To preserve supporting evidence, open a saved record in **View** mode and use
its **Books** button. A new journal or stock movement then captures that record's
exact revision and stored-field/attachment snapshot. An edited or deleted source
cannot be silently substituted while the operation is being confirmed.

For approval-gated posting:

1. Configure the module's [Process & actions](module-processes.md), including a
   review action and stage such as Approved.
2. In **Accounting & stock → Source-record approval**, select that stage and save.
3. Review and complete the process decision on a record. Open **Books** from that
   record to post. A completed decision is required even if the required stage
   happens to be the process's initial stage.

MX checks the current stage, completed decision and unchanged record revision
inside the posting transaction. Disabling the required process or editing the
record blocks a new posting until its approval is valid again. Reversals and
finishing existing holds refer to already-posted evidence, not a new approval.

Approval does **not** itself post money or move stock. Operators explicitly
review and enter journal lines or a stock operation. There are currently no
automatic field-to-journal mappings, linked-module writes or stock valuations.
Edits after posting do not change a posted journal or stock balance. A snapshot
freezes stored values and attachment references, not independently changing live
lookups, linked-record labels or rollups.

## If the connection fails

An unacknowledged operation is **unconfirmed**, not marked saved. Its original
payload and operation ID are retained in the tab's account-scoped session
storage before submission. **Retry same operation** sends that same receipt;
MX either confirms its original commit or executes it once. Reusing a receipt
with different entries is rejected. Reloading the tab can recover its receipt.

Do not create a second entry to compensate for an uncertain response. Reconcile
the retained operation and inspect history first. Clearing site storage, losing
the browser profile, closing a non-restored tab or losing the device can lose the
local receipt. MX cannot guarantee recovery of entries it never received. This
is not an offline posting outbox or browser power-loss guarantee.

## Boundaries before production use

These are reusable operational books, **not** a certified accounting/tax/payroll
suite, legal compliance guarantee, complete ERP or multi-currency ledger. They
do not implement tax calculations, posting templates, period statements, bank
reconciliation, depreciation, purchase/sales fulfilment, lots, warehouse transfers
or inventory valuation. Ordinary Decimal/formula record fields remain floating
point and are not exact ledger money. Choose explicit, reviewed posting values.

Posting, balances, evidence and retry acknowledgement are atomic in SQLite.
Durability still depends on the deployment's existing SQLite synchronization,
backups, power protection and storage. Tests validate exercised scenarios, not
every power-cut, browser or accounting-policy failure mode. Keep this local until
your organization's acceptance and backup/restore checks pass.
