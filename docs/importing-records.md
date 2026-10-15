# Import spreadsheet rows into a module

Open the module's Records screen, then **Module actions (⋯) → Import records**.
You need permission to view and create records. Import creates new records;
it never overwrites existing records or builds new modules.

## Before you start

1. Create the destination module and fields in **Administration → Modules & fields**.
2. Configure select choices first. Unknown imported choices are errors, not new options.
3. Keep identifiers such as `00123` as **text** in both Excel and MX. Numeric
   Excel cells merely formatted with leading zeros do not contain that text.
4. Back up before a business migration; test a small subset in a disposable module.

Supported formats: **XLSX, ODS, UTF-8 CSV and TSV**. Save legacy `.xls` as `.xlsx`
first. No Internet conversion service or N1 access is needed to read the file.

## Import an existing workbook

1. Open **Module actions (⋯) → Import records**, beside Refresh.
2. Choose your file. Confirm the separator for CSV/TSV.
3. Click **Read spreadsheet**. Transfer progress is followed by preparation while
   MX reads populated cells. No records are created yet.
4. Tick the sheets to include; click a sheet's name to configure it. Initially
   only the first usable sheet is selected. All selected sheets go to this module.
5. Set **Header row** to the column-name row and **First data row** to the first
   real record after all header/label rows.
6. Choose the destination **MX field** for each wanted column. Leave other columns
   as **Do not import**. Review suggested exact matches; ambiguous/duplicate labels
   are not guessed. Source previews retain row and column locations.
7. Choose the actual format for text dates. `1.5.26` uses **M.D.YY**, meaning
   January 5, 2026. Native Excel dates are detected automatically, including 1904 dates.
8. Enable **Fill down blank dates** only when a blank means the preceding date.
   Carry resets per sheet; carried values alone do not turn blank rows into records.
9. Repeat the mapping for each selected sheet. Different layouts are not silently reused.
10. Click **Validate and preview** and inspect rows/errors. **Back to mapping**
    is available before importing if you need to change the mapping.
11. Click **Start import**. The real progress bar and counts distinguish processed,
    committed saves, needs-attention and pending rows.

Merged cells are not automatically duplicated. Date fill-down is explicit.
Native date/time values retain their timestamp when mapped to text; a Date
destination uses only the calendar date. Excel durations are retained as ISO durations.
Only cached formula values are read. Recalculate and save formulas without
cached values in a spreadsheet editor. MX never executes formulas, macros or external links.

## Tagged templates

Click **Download template** in the first import screen.

| Row | Contents | Action |
|---|---|---|
| 1 | Stable tags, such as `mx:name` | Keep tags unchanged. |
| 2 | Readable field labels | Keep this label row. |
| 3 onward | Your records | Enter one record per row. |

Tags use actual field **keys**, not labels or arbitrary annotations. Automatic
fields and attachments are excluded. You can save it as XLSX/ODS; preserve text IDs.

## Inspect and correct failures

An invalid row is skipped **entirely**, without a partial record/version. Valid
rows continue. **Completed with issues** means some rows need attention even
though processing reached 100%.

1. Select **Needs attention**.
2. Read the sheet, original row, field/column and error explanation.
3. Expand the row to inspect original cells.
4. Click **Correct and retry**, fix the prefilled ordinary record form, then save.
5. The log links the new record. Repeating an acknowledged correction does not duplicate it.

Validation includes types, required fields, select options, form rules, relationship
access and uniqueness. Uniqueness is checked again at commit because another user
may save a matching value after your preview.

**Download failed-row log** exports all failures, not just the visible page. Formula-like
CSV contents are escaped to prevent running spreadsheet commands. Results/history
use **Previous/Next**, 20 rows/jobs per page. For multiline CSV, a source row is the
physical line where that record begins.

## Pause, reconnect and restart

**Pause import** stops after the current request. Closing the dialog or leaving
the module preserves committed progress. The browser drives bounded batches;
there is no unattended import after the page closes.

Reopen **Module actions (⋯) → Import history**, select the file, then **Resume import**.
Temporary connection/service failures retry while the dialog remains active;
Pause stops automatic retries. Permission/configuration errors need attention first.
Revoked module permissions block subsequent saves.

The success marker, normal record and version are committed together in SQLite.
Repeated requests and server restart cannot duplicate successful rows **in that job**.
Resume the existing job: uploading the workbook as a new job is a new import.
Configure a unique business field to reject duplicates across independent uploads.

## Relationships, automatic values and attachments

- Relationships take linked **MX record IDs**, not names. Multiple relationships
  take a JSON array of IDs. MX never guesses duplicate names or bypasses source-module access.
- For relationship auto-fill, map the Relationship and leave destination fields
  **Do not import**. Normal save logic copies them. A mapped destination, including
  a blank, is an intentional override rather than an omitted field.
- Lookup, Rollup, Formula and Auto-number fields are generated by MX, not import destinations.
- Attachment columns/local file paths are not uploaded. Add files through the
  normal record workflow after import. Rows with required attachments are skipped
  with a manual-entry explanation: create them through the ordinary record form
  with their files. Correct-and-retry handles field values, not file uploads.
  Current form rules still apply.

## Large or damaged spreadsheets

Small-server safety budgets: 64 MiB compressed input, 256 MiB expanded archive,
128 sheets, 100,000 populated rows across usable sheets, 512 populated columns
and 64 MiB normalized cell text. Split larger migrations. Formatting-only cells
do not count. These are **import budgets, not Drive upload limits**.

An unusable XLSX sheet is labelled **Sheet needs attention** and excluded without
blocking usable sheets. Correct it and import separately. Corrupt archives,
malformed CSV record boundaries and invalid ODS structure are file-level errors;
MX cannot safely invent record boundaries.

Your `COMMUNICATION JANUARY-JULY 2026.xlsx` has different header rows/layouts
by month. Select and map the desired sheets individually. Its October template
contains headers repeated across thousands of populated columns and is flagged
separately; earlier usable sheets remain available.

## Log storage and cleanup

Original normalized cells, mappings and results remain in SQLite for inspection.
No second original workbook file or N1 source object is retained. Logs are private
to the importing account and require current module access.

Use **Import log storage → Remove import log** when finished. This removes staged
source/results only. **Imported records, attachments and versions are unchanged.**
Pending rows cannot be resumed after their log is removed. SQLite may reuse allocated
space rather than immediately shrinking its file.

Deleting a module or importing account also removes its private import logs;
record/attachment deletion still follows the normal module/account workflow.

This version is create-only: no update/upsert, fuzzy name matching or automatic
cross-module sheet routing. Use the normal record editor to update existing data.
