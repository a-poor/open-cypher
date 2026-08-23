// SPDX-License-Identifier: MIT OR Apache-2.0

use lalrpop::Configuration;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const INPUT_FILE: &str = "grammar/cypher.lalrpop";
const OUTPUT_DIR: &str = "src/generated";
const OUTPUT_FILE: &str = "src/generated/cypher.rs";
const GENERATED_SPDX_HEADER: &str = "// SPDX-License-Identifier: Apache-2.0\n\n";

pub fn generate(root: &Path) -> Result<(), String> {
    ensure_source(root)?;
    fs::create_dir_all(root.join(OUTPUT_DIR)).map_err(|error| format!("{OUTPUT_DIR}: {error}"))?;
    generate_into(root, &root.join(OUTPUT_DIR))?;
    println!("generated {OUTPUT_FILE} from {INPUT_FILE}");
    Ok(())
}

pub fn verify(root: &Path) -> Result<(), String> {
    ensure_source(root)?;
    let checked = root.join(OUTPUT_FILE);
    if !checked.is_file() {
        return Err(format!(
            "checked-in parser is missing: {OUTPUT_FILE}; run `cargo run -p open-cypher-xtask -- grammar generate`"
        ));
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock: {error}"))?
        .as_nanos();
    let temp = std::env::temp_dir().join(format!(
        "open-cypher-grammar-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&temp).map_err(|error| format!("{}: {error}", temp.display()))?;

    let result = (|| {
        generate_into(root, &temp)?;
        let generated = temp.join("cypher.rs");
        let expected = fs::read(&checked).map_err(|error| format!("{OUTPUT_FILE}: {error}"))?;
        let found =
            fs::read(&generated).map_err(|error| format!("{}: {error}", generated.display()))?;
        if expected == found {
            println!("verified {OUTPUT_FILE} is current");
            Ok(())
        } else {
            Err(format!(
                "{OUTPUT_FILE} is stale; run `cargo run -p open-cypher-xtask -- grammar generate`"
            ))
        }
    })();

    if let Err(error) = fs::remove_dir_all(&temp) {
        eprintln!("warning: could not remove {}: {error}", temp.display());
    }
    result
}

fn ensure_source(root: &Path) -> Result<(), String> {
    if root.join(INPUT_FILE).is_file() {
        Ok(())
    } else {
        Err(format!("grammar source is missing: {INPUT_FILE}"))
    }
}

fn generate_into(root: &Path, output: &Path) -> Result<(), String> {
    let input = root.join(INPUT_FILE);
    Configuration::new()
        .set_out_dir(output)
        .force_build(true)
        .emit_rerun_directives(false)
        .process_file(input)
        .map_err(|error| format!("could not generate parser: {error}"))?;

    let generated = output.join("cypher.rs");
    let contents = fs::read_to_string(&generated)
        .map_err(|error| format!("{}: {error}", generated.display()))?;
    if !contents.starts_with(GENERATED_SPDX_HEADER) {
        fs::write(&generated, format!("{GENERATED_SPDX_HEADER}{contents}"))
            .map_err(|error| format!("{}: {error}", generated.display()))?;
    }
    let status = Command::new("rustfmt")
        .args(["--edition", "2024"])
        .arg(&generated)
        .status()
        .map_err(|error| format!("could not run rustfmt on {}: {error}", generated.display()))?;
    if !status.success() {
        return Err(format!(
            "rustfmt failed while formatting {}: {status}",
            generated.display()
        ));
    }
    Ok(())
}
