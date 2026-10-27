# Appearance and compact workspaces

[Documentation index](README.md)

This design is an **unreleased development preview**. Committing or pushing its
source does not announce RC.11 or change the version of any deployed MX server.

## Product theme

MX Sage uses the same visual language as the MX product preview: warm white
content, muted sage navigation and board surfaces, dark readable text, and a
terracotta action/selection accent. Green, amber and red retain their status
meanings. The MX logo itself is unchanged.

Dark mode deliberately uses a different, console-inspired palette: neutral
near-black/charcoal surfaces, clear gray separators and a blue action accent.
It does not tint the entire workspace green. The default accent adapts with
Light/Dark/System; an explicit personal or organization color is preserved.

For a new deployment this is the default appearance. Existing saved organization
colors, density, corner style and personal overrides are not silently reset.

An administrator can explicitly choose the product theme:

1. Open **Administration → Deployment**.
2. Under **Deployment identity → Appearance → Preset**, select
   **MX Sage · product theme**.
3. Choose the required corner style, density and default light/dark/system mode.
4. Choose **Save identity**. Upload limits and the existing logo are preserved.

Existing Blue, Emerald, Violet, Amber, Rose, Slate and Custom choices remain
available. Saving an appearance does not create modules, alter records, change
permissions or post accounting/inventory operations.

## Module workspace

- A compact title/action row replaces the large repeated workspace banner.
- Slim view tabs retain the organization's configured names and order.
- The **Info** button beside the module name expands the full module and current
  view descriptions. Click again to collapse them. Instructions are not deleted.
- Pipeline stages use quiet sage surfaces and white record cards. **Open & review**
  still opens the saved record and its permitted process actions; moving a card
  is not an approval.
- Directories put identity and title on one row. Full field values remain readable.
- Operations, document libraries and inline books use the same surface, border
  and typography rules. The existing Records view, paging and optional continuous
  scroll remain available.

Record dialogs remain anchored while process/file information loads, rather
than moving their Close button. Keyboard focus stays within the dialog. Escape
closes it subject to the existing save/upload/queued-file guards; this does not
bypass pending-save checks or discard queued files.

## Personal readability

Open **Personal settings → Text size** to use Standard, Large, Extra large or
200%. Text sizing and interface sizing remain separate controls. Light, Dark and
Use system setting remain supported. A personal accent overrides the administrator
accent until the user chooses **Use administrator default accent**.

Module **Options → Reading settings** controls personal bold fields and column
appearance. In a module without configured workspace tabs, use its existing
**Reading settings** button.

1. Find the field by name.
2. Check **Bold** to emphasize its label and value in tables, forms and history.
3. For record tables choose **Plain text** or **Badge**, then a color.
4. **Automatic** uses no special color for plain text. For badges it assigns a
   stable category color; it does not infer approval, urgency or other meaning.
   Select and Yes/No fields use badges by default. Attachments keep file controls.
5. Review the sample and choose **Save my reading settings**. The setting follows
   your account on other browsers. **Use default** removes a field's display/color
   override; bold remains a separate choice.

These settings affect only your view. They do not change field types, stored
record values, other users' views, permissions or exports. If another window
changes the same module's preferences, MX rejects the stale save and asks you to
reload. Unsaved appearance/module-creation drafts require confirmation before
outside-click, Escape or Close discards them. The module browser itself can close
with Escape or a click outside and returns focus to its opener.

Tables use readable sentence-case headers, quiet horizontal row rules, numeric
alignment and labeled category badges instead of a dense spreadsheet grid.
Long values wrap or use the existing internal scroll surfaces; enlargement is
not implemented by shrinking the workspace or truncating data.

New views/dialogs use short entrance fades with no delayed exit, scroll animation
or duplicate mounted views. Both the OS **Reduce motion** setting and MX's own
reduced-motion setting disable these effects.

Color-token contrast tests and real-browser reflow checks support local review;
they are not an accessibility certification of every configured module. Review
your organization's actual fields, long names, permissions, devices and preferred
text sizes before adopting a new frontend.

## Local verification and release boundary

Source changes stay in the original MX repository. Generated bundles, test
databases, browser profiles and the populated preview stay on the project HDD.
The published frontend, release assets and production deployments are not replaced.
See [TESTING.md](../TESTING.md) for the final local evidence and known limits.
