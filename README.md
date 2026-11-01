# Taratasy Reader

A small e-book reader for EPUB, MOBI and AZW3 books, written in Rust with egui.

## Platform

Tested on **Linux only** (Linux Mint) for now. Windows and macOS have not been tried; the code has no Linux-specific dependencies, but expect rough edges there (for example, the reading font is looked up in `/usr/share/fonts` and falls back to egui's built-in font elsewhere).

## Features

- One tab per open book, summary panel on the left (cover, progress, time left, clickable contents)
- Inline images, light and dark theme (View menu or Ctrl+D)
- Remembers open tabs, the active book, and the section and scroll position of each book (`~/.config/taratasy-reader/state.json`)
- Keys: Left/Right for previous/next section, Space/PageDown/PageUp to scroll, Ctrl+O to open

## Formats

| Format | Status |
| --- | --- |
| EPUB | Tested on real books (text, chapters, cover, images) |
| MOBI / AZW / AZW3 / PRC | Supported through the `mobi` crate, **not yet tested on real files**. Newer AZW3 (KF8) files may fail to open; an error message is shown instead of crashing. |

DRM-protected Kindle files cannot be read. Pages follow the book's sections (spine items), not the publisher's page layout.

## Build and run

```bash
cargo run --release
```

No system libraries are required beyond a Rust toolchain.
