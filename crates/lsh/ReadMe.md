# ruf4-lsh

Syntax highlighting definitions for [ruf4](https://github.com/kromych/ruf4), a
dual-panel file commander for the terminal. The crate is published so that the
other `ruf4` crates can be built from crates.io; it carries no stability
guarantees outside of that use.

The library is named `lsh` and contains:

- the compiler for the `.lsh` highlighting definition language (frontend,
  optimizer, regex/charset engine, backend) and a Rust code generator meant
  to be run from a build script (feature `compiler`),
- glob matching of file names to pick a language (feature `glob`),
- the runtime that applies the generated definitions to text,
- the built-in `definitions/` for C, C++, C#, diff, F#, git, Go, Java,
  JavaScript, JSON, Lua, Markdown, Odin, PowerShell, properties, Python, Ruby,
  Rust, shell scripts, SQL, TOML, XML, YAML, Zig and more.

Both features are on by default. The crate has no dependencies.

## License

MIT, see [LICENSE](https://github.com/kromych/ruf4/blob/main/LICENSE).

This crate is derived from code in
[Microsoft Edit](https://github.com/microsoft/edit) (MIT); see
[LICENSE-EDIT](https://github.com/kromych/ruf4/blob/main/LICENSE-EDIT).
