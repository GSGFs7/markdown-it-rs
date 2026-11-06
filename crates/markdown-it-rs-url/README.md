# markdown-it-rs-url

This is a submodule of markdown-it-rs.

The purpose of this module is to replace the mdurl crate and provide the basic API required by the main library.
This is a compatibility layer, not a complete general purpose lib.

The private Punycode codec is derived from `idna` 1.1.0 and used under the MIT
license (see [LICENSE-MIT-PUNYCODE](LICENSE-MIT-PUNYCODE)).
