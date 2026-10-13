# Records and search reference

[Documentation index](README.md) · [Project overview](../README.md)

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

