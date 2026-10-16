# Records and search reference

[Documentation index](README.md) · [Project overview](../README.md)

## Continuous records view

Record browsing defaults to **Pages**. In **View**, choose **Continuous scroll**
when you prefer a bounded scrolling window. Existing saved view choices are
preserved per MX account and module in this browser. A fresh deployment has no
starter module or records; these choices apply after an administrator creates
your modules and fields.

Switching views keeps the current horizontal position and stays near the current
record. A measured paged column layout is reused in continuous mode at the same
viewport width, so switching does not squeeze long text or widen date/number
columns unexpectedly. View changes reuse the loaded schema, cancel obsolete
requests and avoid rendering the entire cached window as a page. Continuous
mode keeps a slim horizontal control visible whenever its columns overflow.

- Scroll down or up to load the next or previous cursor window. The compact
  up/down buttons beside **View** also provide keyboard-accessible **Load earlier**
  / **Load more** actions. Loading feedback shares the results line; there is no
  permanent bottom control bar. A failed edge request shows **Retry** without
  clearing the current records.
- The table fills the remaining screen height. Long-text cells wrap their full
  contents (including line breaks), expanding the row instead of cutting it off
  after three lines. Off-screen rows remain virtualized.
- **Batch** sets the baseline request size (25/50/100/256). Requests can adapt to
  viewport height, up to the API's 256-record cap. The cache holds at most five
  configured batches; only visible rows, a small buffer and focused/edited rows
  are rendered. Distant data is evicted and can be fetched again in reverse.
- A focused/edited row is never evicted silently. If it prevents moving the
  bounded cache, finish editing or move focus, then retry.
- Navigation remembers the visible record UID, its viewport offset, horizontal
  scroll, filters and sort in session storage. Returning from an editor or another
  module restores the anchor. If deleted, its saved ordering cursor positions
  the view near the surviving records. Changing the query starts a new view.
- Incoming collaboration changes show **Refresh this view**, rather than
  reordering the list while someone is reading. Deletions remove the row promptly
  and keep the existing stale-response barriers. Totals flag pending updates.
- The cache indicator is not a claim that all matching rows have downloaded.
  Row numbers describe positions in the current browsing session, not immutable
  record IDs. Search/filter/sort happen on the server. Summaries cover the entire
  filtered result, not just rendered or cached records.

Cursor navigation uses no growing SQL OFFSET. Custom field sorting still depends
on query complexity and indexing; continuous scrolling is not a promise of
constant-time filtering or sorting. Numbered pagination remains available.
The browsing window is not an immutable snapshot of the whole module: concurrent
edits to sort/filter values can change which records belong in later requests.
Use the update notice to refresh deliberately while retaining the visible anchor.

The module records API accepts `window=start|after|before|anchor`. Edge requests
send a returned row cursor; anchor requests send `anchor_uid` and can include its
saved cursor to recover near a deleted row. Cursors are bound to the module,
query, attachment access and resolved sort field/type, not an authorization grant.
Existing page/limit requests are unchanged. For edge requests, `metadata=false`
skips the full-result count and summaries; when
`window.metadata_included` is false, retain the last metadata snapshot rather
than interpreting the placeholder `total: 0` / empty summaries as fresh totals.

## File Attachment Is a Normal Field Type

MX does **not** require one permanent built-in attachment area.

`File Attachment` is an ordinary Record Structure type.

An Administrator can create:

```text
Supporting Documents    File Attachment
Signed Copy             File Attachment
Photo Evidence          File Attachment
Action Documents        File Attachment
```

or create **no attachment fields at all**.

Each File Attachment field can define:

- its field name;
- its N1 storage-folder name;
- an optional user-facing description;
- whether multiple files are allowed;
- the maximum number of files;
- whether the field is required.

Large files are transferred to N1 in bounded multipart segments rather than as
one oversized N1 part. Images receive record-level thumbnails, and the record
viewer provides previous/next navigation across every attached file. Browser
media previews use short-lived access tickets and forward HTTP byte ranges to
N1 so video and audio can begin without downloading the complete object first.

Because attachment fields are part of Record Structure, the same schema drives:

- the record form;
- attachment upload controls;
- table presentation;
- search;
- reporting;
- exports;
- N1 storage organization.

---

## Record Model

MX keeps the internal identity of a record small and stable.

Business values are stored separately and interpreted using Record Structure.

```text
Record
├── UID
├── Created At
├── Updated At
│
├── Dynamic Values
│   ├── Field UID -> Value
│   ├── Field UID -> Value
│   └── ...
│
└── File Attachment Fields
    ├── Field UID
    │   ├── Attachment
    │   ├── Attachment
    │   └── ...
    └── Field UID
        └── Attachment
```

There is no required hard-coded `Office`, `Date`, `Subject`, `Requestor`, `Routing`, `Status`, or `Control Number` column in the business model.

---

## Auto Number

`Auto Number` is a dynamic field type.

A deployment may have one auto-number field, several, or none.

The server allocates authoritative sequence values transactionally.

Configuration can include:

```text
Scope     global | yearly
Prefix    optional text
Padding   0..12
```

Examples:

```text
1
000001
REQ-000001
2026-000001
```

The browser does not allocate the authoritative sequence value.

---

## Formula Fields

`Formula / Calculated` fields are server-computed numeric fields. A formula
references the stable keys of other numeric fields and is recalculated whenever
the record changes.

For example, fields with keys `value` and `khw` can produce a `total` field:

```text
value * khw
```

Expressions support parentheses, the `+`, `-`, `*`, `/`, `%`, and `^`
operators, the `pi` and `e` constants, and these functions:

```text
SUM       PRODUCT    AVERAGE / AVG
MIN       MAX        ABS
CEIL      FLOOR      SQRT
LN        LOG10      EXP
SIN       COS        TAN
POWER     MOD        ROUND
CLAMP
```

Formula fields are read-only in record forms and tables. The server validates
the expression and field references, stores the calculated value, and includes
it in search, sorting, reports, history, and exports.

---

## Dynamic Search

Search is generated from the active Record Structure rather than hard-coded business columns.

### Universal Search

Universal Search searches active fields marked `Searchable` and attachment filenames across every module the current account may read. Results are permission-filtered on the server and open the matching record directly.

### Field Search

Queries can use dynamic field UIDs for field-specific filtering.

MX supports server-side:

- search;
- filtering;
- sort selection;
- sort direction;
- pagination.

The browser does not need to download the complete database just to find or display records.

Wide record tables have a slim, synchronized horizontal scrollbar at the bottom
of the visible table area when their normal bottom scrollbar is off-screen.
Drag it or focus it and use Left/Right, Home/End or Page Up/Down. It disappears
when the normal scrollbar is reachable, columns fit, mobile cards are shown or
a record editor is open. Table styling and server-side pagination are unchanged.

---

