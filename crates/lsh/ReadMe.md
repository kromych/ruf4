# ruf4-lsh

Syntax highlighting definitions for [ruf4](https://github.com/kromych/ruf4), a
dual-panel file commander for the terminal. The crate is published so that the
other `ruf4` crates can be built from crates.io; it carries no stability
guarantees outside of that use.

The library is named `lsh` and contains:

- the compiler for the `.lsh` highlighting definition language (frontend,
  optimizer, regex/charset engine, backend),
- a Rust code generator meant to be run from a build script,
- the runtime that applies the generated definitions to text,
- the built-in `definitions/` for diff, git, JSON, JavaScript, Markdown,
  PowerShell, TOML-like properties, and more.

## License

MIT, see [LICENSE](https://github.com/kromych/ruf4/blob/main/LICENSE).

This crate is derived from code in
[Microsoft Edit](https://github.com/microsoft/edit) (MIT); see
[LICENSE-EDIT](https://github.com/kromych/ruf4/blob/main/LICENSE-EDIT).
