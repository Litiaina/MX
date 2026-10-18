# N1 storage reference

[Documentation index](README.md) · [Project overview](../README.md)

## Release compatibility

| Contract | MX's current target |
| --- | --- |
| N1 product | `1.0.0-RC.1` |
| N1 storage-format specification | `1.0.0-RC.1` |
| On-disk encoding | `4`, unchanged |
| Object API | NOA v1, `/noa/v1` |
| Native protocol, not used by MX | `n1/0.5` |
| N1 backup repository | `n1-backup-v1`, unchanged |

N1's formal RC numbering replaces its private `4.0.0` development label; it does
not rewrite encoding-4 roots, object versions or backups. MX does not compare N1's
product version to its own or rename API routes based on the product number.
Older unsupported development storage encodings are not migrated automatically.

MX still owns file names, ownership, folders and file-version history. Each MX
file version uses a new immutable N1 object key with the original extension.
Renaming metadata does not rename N1 objects. Drive uses durable multipart for
restart-resumable uploads; bounded record/chat attachments use standard multipart.
Soft deletion and retention remain N1's byte-reclamation mechanism; an MX upgrade
does not empty N1 trash, reset fragments or replace operator credentials.

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

