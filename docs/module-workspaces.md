# Module views and layouts

MX stores business information as records, but a module does not have to open as a spreadsheet. An administrator can configure a module’s main workspace independently of its fields, relationships, process and books.

Existing modules keep their Records interface until a layout is explicitly saved. New installations remain blank: no CRM, ERP or demonstration data is created automatically.

## Configure a workspace

1. Open **Administration → Modules & fields**, find the intended module and check **You are configuring**.
2. Configure its fields and permissions first. For a pipeline, enable **Process & actions**. For accounting or stock, enable the appropriate book under **Accounting & stock**. A document library needs a File Attachment field.
3. Open **Views & layout**. Choose an optional starting layout and click **Use starting layout**. This changes the draft only.
4. Rename, add, remove and reorder views. Select the **Opening view**. Configure directory titles, card information and optional overview summaries.
5. Optionally group record fields into named sections. Unassigned fields remain available under Other information; they are not silently hidden.
6. Click **Save workspace layout**, then **Open this module**. Another administrator’s intervening changes produce a revision conflict rather than overwriting their configuration.

## Available views

| View | Main experience | Prerequisite |
| --- | --- | --- |
| Records | Existing searchable, paged or optional continuous record table | None |
| Pipeline board | One column per process stage, cards and review actions | Enabled process |
| Cards / directory | Searchable cards with configured title and information | Fields configured for useful titles |
| Operations overview | Whole-module totals, stage counts, action inbox and quick-access register | Optional process; summary tiles require configured Column totals |
| Document library | File-bearing records with previews and Office opening | File Attachment field and attachment permission |
| Accounting workspace | Full-page journals, balances and posting tools | Accounting enabled and whole-book access |
| Inventory workspace | Full-page stock balances, reservations and movements | Inventory enabled and whole-book access |

Starting layouts are editable arrangements of these views. CRM opens a pipeline; ERP / operations opens an overview; Accounting opens the journal; Inventory opens stock; DMS opens documents; an information system opens Records. A preset does not create a chart of accounts, configure approval policy or certify a complete ERP/accounting product.

## Pipeline decisions and scalability

Columns follow configured Process & actions, including their stable stage keys and title field. Open a card to review its saved values and use a permitted action. Moving a card visually is not an approval: MX still verifies required information, actor permissions, record revisions and process revisions before committing a decision.

Each stage has a bounded 40-record page with Previous / Next. Directory and document views have real 24-record pages. Pipeline totals are computed on the server, independently of the loaded cards. Overview metrics cover all permission-visible records, not just the quick-access sample. Stage totals use one grouped read rather than downloading every record. At most two stage pages load concurrently.

While a pipeline is open, live changes produce a refresh notice instead of unexpectedly reordering the board during a review. Refresh explicitly to load the latest stage contents.

## Security and saved data

Views are presentation, not access control. Existing server-enforced module grants, creator-only records, restricted fields, attachment permissions and whole-book operator access continue to apply. Shared card titles and summaries must use module-visible fields. Personal bold-field preferences apply to directory information as well as records.

Saving a layout does not alter records, relationships, versions, approvals, attachments or posted balances. Layout changes retain a configuration revision and operator history. A field used in a saved layout cannot be archived until its named view/section dependency is removed or replaced.

Do not treat ordinary numeric overview summaries as an accounting ledger. Authoritative financial balances come from exact posted books. These are configurable workspace views, not tax, payroll, inventory-valuation or regulatory certifications.
