# Third-Party Notices

Screen Cap'n includes and is built with open-source and third-party software.
This file summarizes the third-party components used by the application and
the licenses identified from the repository lockfiles and bundled source files.

This notice is provided for attribution and license-compliance purposes. It is
not legal advice.

## Runtime Components

These components are bundled with, statically linked into, or loaded by the
Screen Cap'n Windows application.

| Component | Version | License | Notes |
| --- | ---: | --- | --- |
| Konva | 10.3.0 | MIT | Bundled in `crates/screencaptn-win/assets/web-ui/vendor/konva.js`. |
| Lexical | 0.28.0 | MIT | Bundled into the Web UI bundle. Copyright (c) Meta Platforms, Inc. and affiliates. |
| @lexical/history | 0.28.0 | MIT | Bundled into the Web UI bundle. |
| @lexical/plain-text | 0.28.0 | MIT | Bundled into the Web UI bundle. |
| Microsoft WebView2 Loader | 1.0.2903.40 | Microsoft software terms | Packaged as `WebView2Loader.dll`; WebView2 runtime is provided by Microsoft Edge WebView2. |
| Rust standard library | Toolchain-provided | MIT OR Apache-2.0 | Linked into the Rust executable as applicable. |

### Konva Notice

Konva JavaScript Framework v10.3.0

Copyright (C) 2011 - 2013 by Eric Rowell (KineticJS)

Copyright (C) 2014 - present by Anton Lavrenov (Konva)

Licensed under the MIT License.

### Lexical Notice

Lexical and `@lexical/*` packages are licensed under the MIT License.

Copyright (c) Meta Platforms, Inc. and affiliates.

## Rust Crates

Screen Cap'n uses Rust crates from crates.io. The following list is based on
`Cargo.lock` and local crate metadata available at release-candidate time.

| Crate | Version | License |
| --- | ---: | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| arrayref | 0.3.9 | BSD-2-Clause |
| arrayvec | 0.7.6 | MIT OR Apache-2.0 |
| autocfg | 1.5.0 | Apache-2.0 OR MIT |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| bitflags | 1.3.2 / 2.11.1 | MIT OR Apache-2.0 |
| bytemuck | 1.25.0 | Zlib OR Apache-2.0 OR MIT |
| byteorder-lite | 0.1.0 | Unlicense OR MIT |
| cfg-if | 1.0.4 | MIT OR Apache-2.0 |
| color_quant | 1.1.0 | MIT |
| core_maths | 0.1.1 | MIT |
| crc32fast | 1.5.0 | MIT OR Apache-2.0 |
| data-url | 0.3.2 | MIT OR Apache-2.0 |
| euclid | 0.22.14 | MIT OR Apache-2.0 |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| flate2 | 1.1.9 | MIT OR Apache-2.0 |
| float-cmp | 0.9.0 | MIT |
| fontdb | 0.23.0 | MIT |
| gif | 0.13.3 | MIT OR Apache-2.0 |
| image-webp | 0.2.4 | MIT OR Apache-2.0 |
| imagesize | 0.13.0 | MIT |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| kurbo | 0.11.3 | Apache-2.0 OR MIT |
| libc | 0.2.186 | MIT OR Apache-2.0 |
| libm | 0.2.16 | MIT |
| log | 0.4.29 | MIT OR Apache-2.0 |
| memchr | 2.8.1 | Unlicense OR MIT |
| memmap2 | 0.9.10 | MIT OR Apache-2.0 |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| pico-args | 0.5.0 | MIT |
| png | 0.17.16 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.106 | MIT OR Apache-2.0 |
| quick-error | 2.0.1 | MIT OR Apache-2.0 |
| quote | 1.0.45 | MIT OR Apache-2.0 |
| resvg | 0.45.1 | Apache-2.0 OR MIT |
| rgb | 0.8.53 | MIT |
| roxmltree | 0.20.0 | MIT OR Apache-2.0 |
| rustybuzz | 0.20.1 | MIT |
| serde | 1.0.228 | MIT OR Apache-2.0 |
| serde_core | 1.0.228 | MIT OR Apache-2.0 |
| serde_derive | 1.0.228 | MIT OR Apache-2.0 |
| serde_json | 1.0.150 | MIT OR Apache-2.0 |
| simd-adler32 | 0.3.9 | MIT |
| simplecss | 0.2.2 | Apache-2.0 OR MIT |
| siphasher | 1.0.3 | MIT OR Apache-2.0 |
| slotmap | 1.1.1 | Zlib |
| smallvec | 1.15.1 | MIT OR Apache-2.0 |
| strict-num | 0.1.1 | MIT |
| svgtypes | 0.15.3 | Apache-2.0 OR MIT |
| syn | 2.0.117 | MIT OR Apache-2.0 |
| thiserror | 1.0.69 | MIT OR Apache-2.0 |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 |
| tiny-skia | 0.11.4 | BSD-3-Clause |
| tiny-skia-path | 0.11.4 | BSD-3-Clause |
| tinyvec | 1.11.0 | Zlib OR Apache-2.0 OR MIT |
| tinyvec_macros | 0.1.1 | MIT OR Apache-2.0 OR Zlib |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 |
| unicode-bidi-mirroring | 0.4.0 | MIT OR Apache-2.0 |
| unicode-ccc | 0.4.0 | MIT OR Apache-2.0 |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-properties | 0.1.4 | MIT OR Apache-2.0 |
| unicode-script | 0.5.8 | MIT OR Apache-2.0 |
| unicode-vo | 0.1.0 | MIT OR Apache-2.0 |
| usvg | 0.45.1 | Apache-2.0 OR MIT |
| version_check | 0.9.5 | MIT OR Apache-2.0 |
| webview2-com | 0.34.0 | MIT |
| webview2-com-macros | 0.8.1 | MIT |
| webview2-com-sys | 0.34.0 | MIT |
| weezl | 0.1.12 | MIT OR Apache-2.0 |
| windows | 0.58.0 | MIT OR Apache-2.0 |
| windows-core | 0.58.0 | MIT OR Apache-2.0 |
| windows-implement | 0.58.0 | MIT OR Apache-2.0 |
| windows-interface | 0.58.0 | MIT OR Apache-2.0 |
| windows-result | 0.2.0 | MIT OR Apache-2.0 |
| windows-strings | 0.1.0 | MIT OR Apache-2.0 |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.53.1 | MIT OR Apache-2.0 |
| xmlwriter | 0.1.0 | MIT |
| zmij | 1.0.21 | MIT |
| zune-core | 0.4.12 | MIT OR Apache-2.0 OR Zlib |
| zune-jpeg | 0.4.21 | MIT OR Apache-2.0 OR Zlib |

## JavaScript Build-Time Packages

The Web UI is bundled before distribution. The following package is used during
the build process and is not intended as a user-facing runtime dependency.

| Component | Version | License |
| --- | ---: | --- |
| esbuild | 0.25.12 | MIT |

## MIT License Text

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

