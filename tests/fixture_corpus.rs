use std::fs;
use std::path::{Path, PathBuf};

use open_cypher::parse;

fn fixture_files(kind: &str) -> Vec<PathBuf> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("smoke")
        .join(kind);
    let mut files = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
        .map(|entry| {
            entry
                .expect("fixture directory entry should be readable")
                .path()
        })
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "cypher")
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

#[test]
fn valid_smoke_fixtures_parse() {
    let files = fixture_files("valid");
    assert!(
        !files.is_empty(),
        "the valid fixture corpus must not be empty"
    );

    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        if let Err(errors) = parse(&source) {
            panic!(
                "valid fixture {} failed to parse:\n{errors:#?}",
                path.display()
            );
        }
    }
}

#[test]
fn invalid_smoke_fixtures_are_rejected() {
    let files = fixture_files("invalid");
    assert!(
        !files.is_empty(),
        "the invalid fixture corpus must not be empty"
    );

    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert!(
            parse(&source).is_err(),
            "invalid fixture {} unexpectedly parsed",
            path.display()
        );
    }
}
