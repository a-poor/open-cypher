// SPDX-License-Identifier: MIT OR Apache-2.0

mod grammar;
mod spec;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be inside the workspace")
        .to_path_buf();
    let args: Vec<_> = env::args().skip(1).collect();

    let result = match args.as_slice() {
        [scope, command] if scope == "grammar" && command == "generate" => grammar::generate(&root),
        [scope, command] if scope == "grammar" && command == "verify" => grammar::verify(&root),
        [scope, command] if scope == "spec" && command == "verify" => spec::verify(&root),
        [scope, command] if scope == "spec" && command == "report" => spec::report(&root),
        [scope, command, tag, commit] if scope == "spec" && command == "update" => {
            spec::update(&root, tag, commit)
        }
        _ => Err(usage()),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> String {
    "usage: cargo run -p open-cypher-xtask -- grammar <generate|verify>\n       cargo run -p open-cypher-xtask -- spec <verify|report|update TAG COMMIT>".into()
}
