# Taratasy Reader

A small e-book reader for EPUB and MOBI/AZW books, written in Rust with egui. No system libraries required.

- One tab per open book, summary panel on the right (progress, time left, clickable contents)
- Remembers open tabs, the active book and the last page of each book (`~/.config/taratasy-reader/state.json`)
- Keys: Left/Right or PageUp/PageDown to turn pages, Ctrl+O to open

```bash
cargo run --release
```

Limits: DRM-protected AZW/KFX files cannot be read; images are not rendered; pages are text chunks of ~1800 characters, not the publisher's layout.
