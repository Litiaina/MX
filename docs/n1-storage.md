# N1 storage reference

[Documentation index](README.md) · [Project overview](../README.md)

## Configurable N1 Storage Layout

Administrators can select fields from each module to build that module's base N1 hierarchy. A field from one module cannot be reused accidentally by another module's layout.

A File Attachment field automatically contributes its own final folder segment.

Example:

```text
Record Structure

Office                 Select
Year                   Integer
Supporting Documents   File Attachment
Signed Copy             File Attachment
```

Storage layout:

```text
Base folder 1: Office
Base folder 2: Year
```

Result:

```text
records/
└── MISO/
    └── 2026/
        ├── Supporting Documents/
        │   ├── request.pdf
        │   └── quotation.xlsx
        │
        └── Signed Copy/
            └── approved.pdf
```

If no schema-derived base folders are configured, MX can use the record identity as the stable base:

```text
records/<record-uid>/<File Attachment field>/filename.ext
```

### Frozen record namespace

Once a record establishes its storage namespace, MX freezes that base namespace for the record.

This prevents later edits to values such as office, division, or date from scattering one record's attachments across unrelated paths.

Changing the global storage-layout configuration affects records whose storage namespace has not yet been established.

Existing N1 object keys are not silently moved just because a label or module layout changes.

Custom modules use separate `records/<module-slug>/` roots. The migrated default Records module keeps its existing `records/` namespace for compatibility.

---

