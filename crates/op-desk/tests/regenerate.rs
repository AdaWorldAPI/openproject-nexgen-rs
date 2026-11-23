//! `src/generated/` equals a fresh run of the generator (rails-compiler's
//! emitter over `openproject.toml` and OpenProject's `db/structure.sql`, then
//! the date/float post-mapping), byte for byte, file for file.

mod common;
mod gen;

#[test]
fn committed_generated_files_equal_a_fresh_run() {
    let Some(_) = common::openproject_src("regenerate") else {
        return;
    };
    let fresh = gen::generate(&gen::repo_root().join("openproject.toml")).expect("generate");
    let dir = gen::repo_root().join("crates/op-desk/src/generated");
    let mut committed: Vec<String> = std::fs::read_dir(&dir)
        .expect("src/generated")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    committed.sort();
    assert_eq!(
        committed,
        fresh.keys().cloned().collect::<Vec<_>>(),
        "file list"
    );
    for (name, text) in &fresh {
        let now = std::fs::read_to_string(dir.join(name)).expect("committed file");
        assert!(
            &now == text,
            "src/generated/{name} differs from a fresh run; regenerate with \
             `OPENPROJECT_SRC=<checkout> cargo run -p op-desk --example generate`"
        );
    }
}
