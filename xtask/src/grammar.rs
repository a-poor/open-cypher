// SPDX-License-Identifier: MIT OR Apache-2.0

use lalrpop::Configuration;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const INPUT_FILE: &str = "grammar/cypher.lalrpop";
const OUTPUT_DIR: &str = "src/generated";
const OUTPUT_FILE: &str = "src/generated/cypher.rs";
const GENERATED_SPDX_HEADER: &str = "// SPDX-License-Identifier: Apache-2.0\n\n";
const GENERATED_PARSER_COUNT: usize = 5;
const GENERATED_TOKEN_TO_SYMBOL_START: &str = "    fn __token_to_symbol<";
const GENERATED_TOKEN_TO_SYMBOL_END: &str = "    fn __simulate_reduce<";
const SHARED_EXTERNAL_TOKEN_SYMBOL: &str = "__Symbol::Variant0(__token)";

// LALRPOP's fixed terminal mapping cannot express the BNF rule that every
// keyword may also be a non-reserved symbolic name. The deterministic rewrite
// marks contextual terminals in token_to_index, tries the identifier action
// only when their native action is an error, and removes the marker before the
// shared semantic-token conversion. Exact site counts deliberately turn a
// generator layout change into an actionable failure instead of stale output.
const TOKEN_TO_INDEX: &str = r#"        fn token_to_index(&self, token: &Self::Token) -> Option<usize> {
            __token_to_integer(token, core::marker::PhantomData::<(&())>)
        }"#;
const CONTEXTUAL_TOKEN_TO_INDEX: &str = r#"        fn token_to_index(&self, token: &Self::Token) -> Option<usize> {
            __token_to_integer(token, core::marker::PhantomData::<(&())>).map(|integer| {
                if token.is_contextual_name_candidate() {
                    integer | (1usize << (usize::BITS - 1))
                } else {
                    integer
                }
            })
        }"#;

const PARSER_ACTION: &str = r#"        fn action(&self, state: i16, integer: usize) -> i16 {
            __action(state, integer)
        }"#;
const CONTEXTUAL_PARSER_ACTION: &str = r#"        fn action(&self, state: i16, integer: usize) -> i16 {
            let contextual_mask = 1usize << (usize::BITS - 1);
            let raw_integer = integer & !contextual_mask;
            let action = __action(state, raw_integer);
            if integer & contextual_mask != 0 && action == 0 {
                let identifier_integer = __token_to_integer(
                    &ParserToken::Identifier,
                    core::marker::PhantomData::<(&())>,
                )
                .expect("the grammar must declare the identifier terminal");
                __action(state, identifier_integer)
            } else {
                action
            }
        }"#;

const TOKEN_TO_SYMBOL: &str = r#"        fn token_to_symbol(&self, token_index: usize, token: Self::Token) -> Self::Symbol {
            __token_to_symbol(token_index, token, core::marker::PhantomData::<(&())>)
        }"#;
const CONTEXTUAL_TOKEN_TO_SYMBOL: &str = r#"        fn token_to_symbol(&self, token_index: usize, token: Self::Token) -> Self::Symbol {
            let contextual_mask = 1usize << (usize::BITS - 1);
            __token_to_symbol(
                token_index & !contextual_mask,
                token,
                core::marker::PhantomData::<(&())>,
            )
        }"#;

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
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock: {error}"))?
        .as_nanos();
    let staging = std::env::temp_dir().join(format!(
        "open-cypher-grammar-stage-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&staging).map_err(|error| format!("{}: {error}", staging.display()))?;

    let result = (|| {
        Configuration::new()
            .set_out_dir(&staging)
            .force_build(true)
            .emit_rerun_directives(false)
            .process_file(input)
            .map_err(|error| format!("could not generate parser: {error}"))?;

        let generated = staging.join("cypher.rs");
        let contents = fs::read_to_string(&generated)
            .map_err(|error| format!("{}: {error}", generated.display()))?;
        if !contents.starts_with(GENERATED_SPDX_HEADER) {
            fs::write(&generated, format!("{GENERATED_SPDX_HEADER}{contents}"))
                .map_err(|error| format!("{}: {error}", generated.display()))?;
        }
        rustfmt(&generated)?;

        let contents = fs::read_to_string(&generated)
            .map_err(|error| format!("{}: {error}", generated.display()))?;
        let contents = postprocess_contextual_names(contents)?;
        fs::write(&generated, contents)
            .map_err(|error| format!("{}: {error}", generated.display()))?;
        rustfmt(&generated)?;

        let destination = output.join("cypher.rs");
        let contents =
            fs::read(&generated).map_err(|error| format!("{}: {error}", generated.display()))?;
        let permissions = fs::metadata(&generated)
            .map_err(|error| format!("{}: {error}", generated.display()))?
            .permissions();
        let mut installed = tempfile::NamedTempFile::new_in(output)
            .map_err(|error| format!("{}: {error}", output.display()))?;
        installed
            .write_all(&contents)
            .map_err(|error| format!("{}: {error}", installed.path().display()))?;
        installed
            .as_file()
            .set_permissions(permissions)
            .map_err(|error| format!("{}: {error}", installed.path().display()))?;
        installed
            .as_file()
            .sync_all()
            .map_err(|error| format!("{}: {error}", installed.path().display()))?;
        installed
            .persist(&destination)
            .map(|_| ())
            .map_err(|error| format!("{}: {}", destination.display(), error.error))
    })();

    if let Err(error) = fs::remove_dir_all(&staging) {
        eprintln!("warning: could not remove {}: {error}", staging.display());
    }
    result
}

fn postprocess_contextual_names(mut contents: String) -> Result<String, String> {
    verify_shared_external_token_symbols(&contents)?;

    for (description, original, replacement) in [
        (
            "token-index contextual marker",
            TOKEN_TO_INDEX,
            CONTEXTUAL_TOKEN_TO_INDEX,
        ),
        (
            "contextual identifier fallback action",
            PARSER_ACTION,
            CONTEXTUAL_PARSER_ACTION,
        ),
        (
            "contextual token-index decoding",
            TOKEN_TO_SYMBOL,
            CONTEXTUAL_TOKEN_TO_SYMBOL,
        ),
    ] {
        let matches = contents.matches(original).count();
        if matches != GENERATED_PARSER_COUNT {
            return Err(format!(
                "could not apply {description}: expected {GENERATED_PARSER_COUNT} generated parser sites, found {matches}; LALRPOP output may have changed"
            ));
        }
        contents = contents.replace(original, replacement);
    }

    Ok(contents)
}

fn verify_shared_external_token_symbols(contents: &str) -> Result<(), String> {
    let function_count = contents.matches(GENERATED_TOKEN_TO_SYMBOL_START).count();
    if function_count != GENERATED_PARSER_COUNT {
        return Err(format!(
            "could not verify shared external-token symbols: expected {GENERATED_PARSER_COUNT} generated parser sites, found {function_count}; LALRPOP output may have changed"
        ));
    }

    let mut remainder = contents;
    for parser_index in 0..GENERATED_PARSER_COUNT {
        let start = remainder
            .find(GENERATED_TOKEN_TO_SYMBOL_START)
            .expect("the generated token-to-symbol function count was checked");
        let function_and_remainder = &remainder[start + GENERATED_TOKEN_TO_SYMBOL_START.len()..];
        let end = function_and_remainder
            .find(GENERATED_TOKEN_TO_SYMBOL_END)
            .ok_or_else(|| {
                format!(
                    "could not verify shared external-token symbols in generated parser {}: the token-to-symbol function boundary changed",
                    parser_index + 1
                )
            })?;
        let function = &function_and_remainder[..end];
        let shared_symbol_count = function.matches(SHARED_EXTERNAL_TOKEN_SYMBOL).count();
        let external_symbol_count = function.matches("__Symbol::Variant").count();
        if shared_symbol_count != 1 || external_symbol_count != 1 {
            return Err(format!(
                "could not verify shared external-token symbols in generated parser {}: expected one shared token-symbol arm, found {external_symbol_count}",
                parser_index + 1
            ));
        }
        remainder = &function_and_remainder[end..];
    }

    Ok(())
}

fn rustfmt(generated: &Path) -> Result<(), String> {
    let status = Command::new("rustfmt")
        .args(["--edition", "2024"])
        .arg(generated)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn generated_parser_sites(count: usize) -> String {
        let mut contents = String::new();
        for _ in 0..count {
            contents.push_str(TOKEN_TO_INDEX);
            contents.push_str(PARSER_ACTION);
            contents.push_str(TOKEN_TO_SYMBOL);
            contents.push_str(GENERATED_TOKEN_TO_SYMBOL_START);
            contents.push_str(SHARED_EXTERNAL_TOKEN_SYMBOL);
            contents.push_str(GENERATED_TOKEN_TO_SYMBOL_END);
        }
        contents
    }

    #[test]
    fn contextual_postprocessing_transforms_every_generated_parser() {
        let contents = postprocess_contextual_names(generated_parser_sites(GENERATED_PARSER_COUNT))
            .expect("all generated parser sites should be transformed");

        for (original, replacement) in [
            (TOKEN_TO_INDEX, CONTEXTUAL_TOKEN_TO_INDEX),
            (PARSER_ACTION, CONTEXTUAL_PARSER_ACTION),
            (TOKEN_TO_SYMBOL, CONTEXTUAL_TOKEN_TO_SYMBOL),
        ] {
            assert_eq!(contents.matches(original).count(), 0);
            assert_eq!(
                contents.matches(replacement).count(),
                GENERATED_PARSER_COUNT
            );
        }
    }

    #[test]
    fn contextual_postprocessing_rejects_generator_shape_drift() {
        let error = postprocess_contextual_names(generated_parser_sites(
            GENERATED_PARSER_COUNT.saturating_sub(1),
        ))
        .expect_err("a changed parser count must fail generation");

        assert!(error.contains("expected 5 generated parser sites, found 4"));
    }

    #[test]
    fn contextual_postprocessing_rejects_split_external_token_symbols() {
        let contents = generated_parser_sites(GENERATED_PARSER_COUNT).replacen(
            GENERATED_TOKEN_TO_SYMBOL_END,
            "__Symbol::Variant1(__token)    fn __simulate_reduce<",
            1,
        );
        let error = postprocess_contextual_names(contents)
            .expect_err("contextual tokens require one shared semantic-symbol variant");

        assert!(error.contains("could not verify shared external-token symbols"));
        assert!(error.contains("expected one shared token-symbol arm, found 2"));
    }

    #[test]
    fn contextual_postprocessing_rejects_a_second_application() {
        let contents = postprocess_contextual_names(generated_parser_sites(GENERATED_PARSER_COUNT))
            .expect("the first application should succeed");
        let error = postprocess_contextual_names(contents)
            .expect_err("postprocessing is deliberately not silently idempotent");

        assert!(error.contains("expected 5 generated parser sites, found 0"));
    }
}
