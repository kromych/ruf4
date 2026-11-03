# ruf4-tui

The immediate mode TUI framework behind
[ruf4](https://github.com/kromych/ruf4), a dual-panel file commander for the
terminal. The crate is published so that `ruf4` can be built from crates.io; it
carries no stability guarantees outside of that use.

It provides a VT parser and writer, a framebuffer with damage tracking, input
handling (keyboard, mouse, bracketed paste), an Oklab-based color system,
Unicode-aware text measurement, a clipboard, and the widget layer used by the
file commander. Runs on Linux, macOS, and Windows.

## License

MIT, see [LICENSE](https://github.com/kromych/ruf4/blob/main/LICENSE).

This crate is derived from code in
[Microsoft Edit](https://github.com/microsoft/edit) (MIT); see
[LICENSE-EDIT](https://github.com/kromych/ruf4/blob/main/LICENSE-EDIT).
