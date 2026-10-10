# Bundled MX Office components

MX's Office adapter is separate from these unmodified upstream components.
All engine and SDK assets are served by MX itself; there is no deployment-time
CDN request. `manifest.json` pins the exact SHA-256 of every executable/data asset.

- ZetaJS 1.2.0, copyright allotropia software GmbH and contributors:
  MIT; see `ZETAJS-LICENSE.txt`. Source:
  https://github.com/allotropia/zetajs/tree/v1.2.0
- ZetaOffice / LibreOffice WebAssembly build, upstream assets dated 2025-05-13:
  see `LIBREOFFICE-MPL.txt` (core COPYING.MPL) and
  `LIBREOFFICE-LICENSE.txt` (core COPYING, GPLv3 text). These are license texts,
  NOT a complete inventory of this binary's bundled component/font notices.
  Corresponding engine source published by upstream:
  https://git.libreoffice.org/core/+/refs/heads/distro/allotropia/zeta-24-2
- Upstream WebAssembly toolchain source:
  https://github.com/allotropia/emscripten/tree/fixed-3.1.65
- Upstream Qt sources used by this build:
  https://github.com/allotropia/qt5/tree/5.15.2%2Bwasm and
  https://github.com/allotropia/qtbase/tree/5.15.2%2Bwasm
  See `QT-LGPL3.txt` and the corresponding source-tree component notices.

Engine download provenance (build preparation only):
https://cdn.zetaoffice.net/zetaoffice_latest/soffice.js,
`soffice.wasm`, `soffice.data`, `soffice.data.js.metadata`.
The `latest` URL is NOT used at runtime or to silently update this bundle.
`.wasm.br` and `.data.br` are the upstream Brotli-encoded bytes, served with the
original MIME type and Content-Encoding by MX's static server.
The production build also generates gzip alternatives from these verified
Brotli inputs, so asset delivery does not depend on Brotli negotiation.

The upstream demo's MIT-licensed worker pattern informed the MX adapter:
https://github.com/allotropia/zetajs/tree/v1.2.0/examples/web-office

This is an integration candidate, not a declaration that this older engine build
has passed a current security/support audit. Release maintainers must review
upstream fixes, source availability and complete third-party distribution notices
when updating or distributing the Office package. The files can be replaced by a
compatible, reviewed self-build; update the manifest only after validation.
