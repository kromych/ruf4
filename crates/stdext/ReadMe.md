# ruf4-stdext

Standard library extensions used by [ruf4](https://github.com/kromych/ruf4), a
dual-panel file commander for the terminal. The crate is published so that the
other `ruf4` crates can be built from crates.io; it carries no stability
guarantees outside of that use.

The library is named `stdext` and provides:

- arena allocators and scratch arenas,
- arena-backed collections (`Vec`, `String`),
- SIMD helpers (`memset`, …),
- UTF-8 and Unicode helpers,
- glob matching and small platform shims for Unix and Windows.

## License

MIT, see [LICENSE](https://github.com/kromych/ruf4/blob/main/LICENSE).

This crate is derived from code in
[Microsoft Edit](https://github.com/microsoft/edit) (MIT); see
[LICENSE-EDIT](https://github.com/kromych/ruf4/blob/main/LICENSE-EDIT).
