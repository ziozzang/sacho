Sacho is licensed under MIT. Signed update verification/activation, GitHub release
selection, their tests, and the CodeMirror editor integration were adapted from
[ziozzang/hangang](https://github.com/ziozzang/hangang), copyright Jioh Jung,
under the MIT license in LICENSE. The bundled editor contains the complete MIT
notices for CodeMirror and its dependencies. No remote CDN is used at runtime.

Status page information architecture was inspired by https://status.openai.com/
and https://status.claude.com/. Their logos, brand assets and source code are not
included. Sacho's interface is implemented locally with its own styling.

Local encrypted inputs use RustCrypto chacha20poly1305 0.10.1 (MIT) and zeroize
(MIT). Notification rendering uses MiniJinja 2.24 (Apache-2.0). Their complete
licenses are retained in [docs/licenses](docs/licenses) and embedded for
`sacho help licenses`. These libraries do not add provider-specific clients.
