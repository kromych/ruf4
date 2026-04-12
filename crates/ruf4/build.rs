fn main() {
    #[cfg(target_os = "windows")]
    {
        let rc_path = std::path::Path::new("../../resources/windows/ruf4.rc");
        if rc_path.exists() {
            embed_resource::compile(rc_path, embed_resource::NONE);
        }
    }

    compile_lsh();
}

fn compile_lsh() {
    stdext::arena::init(128 * 1024 * 1024).unwrap();
    let scratch = stdext::arena::scratch_arena(None);

    let lsh_path = lsh::compiler::builtin_definitions_path();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = format!("{out_dir}/lsh_definitions.rs");

    let mut generator = lsh::compiler::Generator::new(&scratch);
    match generator
        .read_directory(lsh_path)
        .and_then(|_| generator.generate_rust())
    {
        Ok(c) => std::fs::write(out_path, c).unwrap(),
        Err(err) => panic!("failed to compile lsh definitions: {err}"),
    };

    println!("cargo::rerun-if-changed={}", lsh_path.display());
}
