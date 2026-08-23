// SPDX-License-Identifier: MIT OR Apache-2.0

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use toml::Value;

const MANIFEST: &str = "spec/UPSTREAM.toml";
const PRODUCTION_MAP: &str = "spec/PRODUCTION_MAP.toml";
const DEVIATIONS: &str = "spec/DEVIATIONS.toml";
const PINNED_RELEASE: &str = "2024.3";
const PINNED_COMMIT: &str = "677cbafabb8c3c5eed458fd3b1ec0daec8d67d23";

struct Manifest {
    values: Value,
    files: BTreeMap<String, String>,
}

struct Production {
    name: String,
    disposition: String,
    target: String,
    witness: String,
}

struct ProductionMap {
    values: Value,
    productions: Vec<Production>,
}

pub fn verify(root: &Path) -> Result<(), String> {
    let manifest = read_manifest(root)?;
    require_value(&manifest, "release", PINNED_RELEASE)?;
    require_value(&manifest, "tag", PINNED_RELEASE)?;
    require_value(&manifest, "commit", PINNED_COMMIT)?;

    let vendor_root = manifest_string(&manifest, "vendor_root")?;
    validate_relative_path(vendor_root)?;
    let actual = inventory(&root.join("spec").join(vendor_root), Path::new(vendor_root))?;
    compare_inventory(&manifest.files, &actual)?;
    verify_count(&manifest, "file_count", actual.len())?;
    verify_count(
        &manifest,
        "feature_file_count",
        actual
            .keys()
            .filter(|path| path.ends_with(".feature"))
            .count(),
    )?;
    verify_count(
        &manifest,
        "graph_file_count",
        actual
            .keys()
            .filter(|path| path.contains("/tck/graphs/"))
            .count(),
    )?;

    let grammar_path = format!("{vendor_root}/grammar/openCypher.bnf");
    let grammar_hash = actual
        .get(&grammar_path)
        .ok_or_else(|| format!("missing grammar: {grammar_path}"))?;
    require_value(&manifest, "grammar_sha256", grammar_hash)?;
    let bnf = fs::read_to_string(root.join("spec").join(&grammar_path))
        .map_err(|error| format!("{grammar_path}: {error}"))?;
    let bnf_productions = production_names(&bnf);
    verify_count(&manifest, "production_count", bnf_productions.len())?;

    let production_map = read_production_map(root)?;
    require_table_value(
        &production_map.values,
        PRODUCTION_MAP,
        "grammar",
        &grammar_path,
    )?;
    require_table_value(
        &production_map.values,
        PRODUCTION_MAP,
        "grammar_sha256",
        grammar_hash,
    )?;
    verify_table_count(
        &production_map.values,
        PRODUCTION_MAP,
        "production_count",
        bnf_productions.len(),
    )?;
    let mapped = &production_map.productions;
    let mapped_names: Vec<_> = mapped.iter().map(|entry| entry.name.as_str()).collect();
    let bnf_names: Vec<_> = bnf_productions.iter().map(String::as_str).collect();
    if mapped_names != bnf_names {
        return Err("PRODUCTION_MAP.toml does not exactly match BNF order/names".into());
    }
    validate_productions(mapped)?;
    validate_deviation_evidence(root)?;

    println!(
        "verified openCypher {} at {}: {} files, {} features, {} graph fixtures, {} productions",
        PINNED_RELEASE,
        PINNED_COMMIT,
        actual.len(),
        actual
            .keys()
            .filter(|path| path.ends_with(".feature"))
            .count(),
        actual
            .keys()
            .filter(|path| path.contains("/tck/graphs/"))
            .count(),
        bnf_productions.len()
    );
    Ok(())
}

pub fn report(root: &Path) -> Result<(), String> {
    verify(root)?;
    let manifest = read_manifest(root)?;
    let productions = read_production_map(root)?.productions;
    let mut dispositions = BTreeMap::<String, usize>::new();
    for production in productions {
        *dispositions.entry(production.disposition).or_default() += 1;
    }
    println!("source: {}", manifest_string(&manifest, "release_url")?);
    println!(
        "grammar sha256: {}",
        manifest_string(&manifest, "grammar_sha256")?
    );
    println!("production mapping:");
    for (disposition, count) in dispositions {
        println!("  {disposition}: {count}");
    }
    Ok(())
}

/// Download and inspect an explicitly pinned candidate without altering the repository.
pub fn update(root: &Path, tag: &str, commit: &str) -> Result<(), String> {
    validate_tag(tag)?;
    if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("COMMIT must be an exact 40-character hexadecimal object ID".into());
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock: {error}"))?
        .as_nanos();
    let temp =
        std::env::temp_dir().join(format!("open-cypher-spec-{}-{nonce}", std::process::id()));
    fs::create_dir(&temp).map_err(|error| format!("{}: {error}", temp.display()))?;
    let result = inspect_update(root, tag, commit, &temp);
    if let Err(error) = fs::remove_dir_all(&temp) {
        eprintln!("warning: could not remove {}: {error}", temp.display());
    }
    result
}

fn compare_inventory(
    expected: &BTreeMap<String, String>,
    actual: &BTreeMap<String, String>,
) -> Result<(), String> {
    let expected_paths: BTreeSet<_> = expected.keys().cloned().collect();
    let actual_paths: BTreeSet<_> = actual.keys().cloned().collect();
    let missing: Vec<_> = expected_paths.difference(&actual_paths).cloned().collect();
    let extra: Vec<_> = actual_paths.difference(&expected_paths).cloned().collect();
    if !missing.is_empty() || !extra.is_empty() {
        return Err(format!(
            "snapshot inventory differs: missing [{}]; extra [{}]",
            missing.join(", "),
            extra.join(", ")
        ));
    }
    for (path, expected_hash) in expected {
        let found = actual
            .get(path)
            .ok_or_else(|| format!("manifest file is missing: {path}"))?;
        if found != expected_hash {
            return Err(format!(
                "checksum mismatch for {path}: expected {expected_hash}, found {found}"
            ));
        }
    }
    Ok(())
}

fn inspect_update(root: &Path, tag: &str, commit: &str, temp: &Path) -> Result<(), String> {
    let archive = temp.join("openCypher.tar.gz");
    let archive_url = format!("https://github.com/opencypher/openCypher/archive/{commit}.tar.gz");
    run(
        Command::new("curl")
            .args(["-L", "--fail", "--silent", "--show-error", "-o"])
            .arg(&archive)
            .arg(&archive_url),
        "download upstream archive",
    )?;
    run(
        Command::new("tar")
            .args(["-xzf"])
            .arg(&archive)
            .arg("-C")
            .arg(temp),
        "extract upstream archive",
    )?;

    let source = temp.join(format!("openCypher-{commit}"));
    for path in [
        "LICENSE",
        "NOTICE",
        "grammar/openCypher.bnf",
        "tck/features",
        "tck/graphs",
    ] {
        if !source.join(path).exists() {
            return Err(format!("archive does not contain required path: {path}"));
        }
    }

    let candidate = selected_inventory(&source)?;
    let current_manifest = read_manifest(root)?;
    let current_root = manifest_string(&current_manifest, "vendor_root")?;
    let current: BTreeMap<_, _> = current_manifest
        .files
        .iter()
        .map(|(path, hash)| {
            let relative = path
                .strip_prefix(current_root)
                .and_then(|path| path.strip_prefix('/'))
                .unwrap_or(path);
            (relative.to_owned(), hash.to_owned())
        })
        .collect();

    let added: Vec<_> = candidate
        .keys()
        .filter(|path| !current.contains_key(*path))
        .cloned()
        .collect();
    let removed: Vec<_> = current
        .keys()
        .filter(|path| !candidate.contains_key(*path))
        .cloned()
        .collect();
    let changed: Vec<_> = candidate
        .iter()
        .filter(|(path, hash)| current.get(*path).is_some_and(|old| old != *hash))
        .map(|(path, _)| path.clone())
        .collect();
    let grammar_path = source.join("grammar/openCypher.bnf");
    let grammar =
        fs::read_to_string(&grammar_path).map_err(|error| format!("candidate grammar: {error}"))?;

    println!("openCypher update inspection (repository unchanged)");
    println!("  tag label: {tag}");
    println!("  commit: {commit}");
    println!("  archive: {archive_url}");
    println!("  archive sha256: {}", sha256_file(&archive)?);
    println!("  grammar sha256: {}", sha256_file(&grammar_path)?);
    println!("  productions: {}", production_names(&grammar).len());
    print_paths("added", &added);
    print_paths("removed", &removed);
    print_paths("changed", &changed);
    println!(
        "review and adopt by updating spec/vendor, UPSTREAM.toml, PRODUCTION_MAP.toml, DEVIATIONS.toml, and the xtask release/commit pins together"
    );
    Ok(())
}

fn print_paths(label: &str, paths: &[String]) {
    println!("  {label}: {}", paths.len());
    for path in paths {
        println!("    {path}");
    }
}

fn run(command: &mut Command, description: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("could not {description}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "could not {description}: process exited with {status}"
        ))
    }
}

fn validate_tag(tag: &str) -> Result<(), String> {
    if tag.is_empty()
        || !tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("TAG may contain only ASCII letters, digits, '.', '_' and '-'".into());
    }
    Ok(())
}

fn read_manifest(root: &Path) -> Result<Manifest, String> {
    let values = read_toml(root, MANIFEST)?;
    validate_hash(table_string(&values, "archive_sha256", MANIFEST)?)?;
    validate_hash(table_string(&values, "grammar_sha256", MANIFEST)?)?;
    let records = values
        .get("file")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{MANIFEST}: missing [[file]] records"))?;
    let mut files = BTreeMap::new();
    for record in records {
        let path = table_string(record, "path", MANIFEST)?.to_owned();
        let hash = table_string(record, "sha256", MANIFEST)?.to_owned();
        validate_relative_path(&path)?;
        validate_hash(&hash)?;
        if files.insert(path.clone(), hash).is_some() {
            return Err(format!("{MANIFEST}: duplicate file {path}"));
        }
    }
    Ok(Manifest { values, files })
}

fn read_production_map(root: &Path) -> Result<ProductionMap, String> {
    let values = read_toml(root, PRODUCTION_MAP)?;
    let productions = values
        .get("production")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{PRODUCTION_MAP}: missing [[production]] records"))?
        .iter()
        .map(|record| {
            Ok(Production {
                name: table_string(record, "name", PRODUCTION_MAP)?.to_owned(),
                disposition: table_string(record, "disposition", PRODUCTION_MAP)?.to_owned(),
                target: table_string(record, "target", PRODUCTION_MAP)?.to_owned(),
                witness: table_string(record, "witness", PRODUCTION_MAP)?.to_owned(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ProductionMap {
        values,
        productions,
    })
}

fn read_toml(root: &Path, relative: &str) -> Result<Value, String> {
    let text =
        fs::read_to_string(root.join(relative)).map_err(|error| format!("{relative}: {error}"))?;
    toml::from_str(&text).map_err(|error| format!("{relative}: {error}"))
}

fn table_string<'a>(value: &'a Value, key: &str, path: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}: missing or invalid {key}"))
}

fn manifest_string<'a>(manifest: &'a Manifest, key: &str) -> Result<&'a str, String> {
    table_string(&manifest.values, key, MANIFEST)
}

fn require_value(manifest: &Manifest, key: &str, expected: &str) -> Result<(), String> {
    let found = manifest_string(manifest, key)?;
    if found == expected {
        Ok(())
    } else {
        Err(format!(
            "{MANIFEST}: {key} must be {expected}, found {found}"
        ))
    }
}

fn require_table_value(
    values: &Value,
    path: &str,
    key: &str,
    expected: &str,
) -> Result<(), String> {
    let found = table_string(values, key, path)?;
    if found == expected {
        Ok(())
    } else {
        Err(format!("{path}: {key} must be {expected}, found {found}"))
    }
}

fn verify_count(manifest: &Manifest, key: &str, found: usize) -> Result<(), String> {
    verify_table_count(&manifest.values, MANIFEST, key, found)
}

fn verify_table_count(values: &Value, path: &str, key: &str, found: usize) -> Result<(), String> {
    let expected = values
        .get(key)
        .and_then(Value::as_integer)
        .ok_or_else(|| format!("{path}: missing or invalid {key}"))?;
    if usize::try_from(expected).ok() == Some(found) {
        Ok(())
    } else {
        Err(format!("{path}: {key} is {expected}, found {found}"))
    }
}

fn validate_productions(productions: &[Production]) -> Result<(), String> {
    let allowed = ["unassessed", "lexer", "parser", "alias", "deviation"];
    let mut seen = BTreeSet::new();
    for production in productions {
        if !seen.insert(production.name.as_str()) {
            return Err(format!("duplicate mapped production: {}", production.name));
        }
        if !allowed.contains(&production.disposition.as_str()) {
            return Err(format!(
                "invalid disposition for {}: {}",
                production.name, production.disposition
            ));
        }
        if production.disposition != "unassessed"
            && (production.target.is_empty() || production.witness.is_empty())
        {
            return Err(format!(
                "mapped production {} needs both target and witness",
                production.name
            ));
        }
    }
    Ok(())
}

fn validate_deviation_evidence(root: &Path) -> Result<(), String> {
    let values = read_toml(root, DEVIATIONS)?;
    let deviations = values
        .get("deviation")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{DEVIATIONS}: missing [[deviation]] records"))?;
    let allowed_statuses = ["accepted", "open"];
    let mut ids = BTreeSet::new();
    for deviation in deviations {
        let id = table_string(deviation, "id", DEVIATIONS)?;
        if id.is_empty() {
            return Err(format!("{DEVIATIONS}: deviation id must not be empty"));
        }
        if !ids.insert(id) {
            return Err(format!("{DEVIATIONS}: duplicate deviation id: {id}"));
        }
        let status = table_string(deviation, "status", DEVIATIONS)?;
        if !allowed_statuses.contains(&status) {
            return Err(format!(
                "{DEVIATIONS}: invalid status for {id}: {status}; expected accepted or open"
            ));
        }
        let evidence = table_string(deviation, "evidence", DEVIATIONS)?;
        let (path, fragment) = evidence
            .split_once('#')
            .map_or((evidence, None), |(path, fragment)| (path, Some(fragment)));
        validate_relative_path(path)?;
        let evidence_path = root.join("spec").join(path);
        if !evidence_path.is_file() {
            return Err(format!("{DEVIATIONS}: evidence does not exist: {path}"));
        }
        if let Some(fragment) = fragment {
            if fragment.is_empty() {
                return Err(format!("{DEVIATIONS}: empty evidence fragment for {id}"));
            }
            let contents = fs::read_to_string(&evidence_path)
                .map_err(|error| format!("{DEVIATIONS}: could not read {path}: {error}"))?;
            if !evidence_fragment_exists(&contents, fragment) {
                return Err(format!(
                    "{DEVIATIONS}: evidence fragment does not exist for {id}: #{fragment}"
                ));
            }
        }
        for key in [
            "kind",
            "summary",
            "baseline",
            "rationale",
            "positive_witness",
            "negative_witness",
        ] {
            if table_string(deviation, key, DEVIATIONS)?.is_empty() {
                return Err(format!("{DEVIATIONS}: {id} has an empty {key}"));
            }
        }
    }
    Ok(())
}

fn evidence_fragment_exists(contents: &str, fragment: &str) -> bool {
    let production = format!("<{}> ::=", fragment.replace('-', " "));
    if contents.lines().any(|line| line.trim_end() == production) {
        return true;
    }

    contents.lines().any(|line| {
        let Some(heading) = line.trim().strip_prefix('#') else {
            return false;
        };
        let heading = heading.trim_start_matches('#').trim();
        markdown_fragment(heading) == fragment
    })
}

fn markdown_fragment(heading: &str) -> String {
    let mut fragment = String::new();
    let mut pending_dash = false;
    for character in heading.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if pending_dash && !fragment.is_empty() {
                fragment.push('-');
            }
            pending_dash = false;
            fragment.push(character);
        } else if character.is_whitespace() || character == '-' {
            pending_dash = true;
        }
    }
    fragment
}

fn validate_relative_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || Path::new(path)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("unsafe relative path in spec metadata: {path}"));
    }
    Ok(())
}

fn validate_hash(hash: &str) -> Result<(), String> {
    if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("invalid SHA-256 digest: {hash}"))
    }
}

fn inventory(directory: &Path, relative_root: &Path) -> Result<BTreeMap<String, String>, String> {
    if !directory.is_dir() {
        return Err(format!(
            "snapshot directory is missing: {}",
            directory.display()
        ));
    }
    let mut files = Vec::new();
    walk(directory, &mut files)?;
    let mut result = BTreeMap::new();
    for file in files {
        let relative = file
            .strip_prefix(directory)
            .map_err(|error| format!("{}: {error}", file.display()))?;
        let path = relative_root
            .join(relative)
            .to_string_lossy()
            .replace('\\', "/");
        result.insert(path, sha256_file(&file)?);
    }
    Ok(result)
}

fn selected_inventory(source: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut result = BTreeMap::new();
    for path in ["LICENSE", "NOTICE", "grammar/openCypher.bnf"] {
        result.insert(path.to_owned(), sha256_file(&source.join(path))?);
    }
    for directory in ["tck/features", "tck/graphs"] {
        result.extend(inventory(&source.join(directory), Path::new(directory))?);
    }
    Ok(result)
}

fn walk(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("{}: {error}", entry.path().display()))?;
        if file_type.is_dir() {
            walk(&entry.path(), files)?;
        } else if file_type.is_file() {
            files.push(entry.path());
        } else {
            return Err(format!(
                "snapshot contains unsupported filesystem entry: {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn production_names(bnf: &str) -> Vec<String> {
    bnf.lines()
        .filter_map(|line| {
            line.trim_end()
                .strip_suffix(" ::=")?
                .strip_prefix('<')?
                .strip_suffix('>')
                .map(str::to_owned)
        })
        .collect()
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::{evidence_fragment_exists, production_names, validate_relative_path};

    #[test]
    fn extracts_only_production_definitions() {
        assert_eq!(
            production_names("# <not one>\n<program> ::= \n  <statement>\n<statement> ::= x\n"),
            ["program"]
        );
    }

    #[test]
    fn rejects_paths_outside_spec() {
        assert!(validate_relative_path("vendor/file").is_ok());
        assert!(validate_relative_path("../file").is_err());
        assert!(validate_relative_path("/file").is_err());
    }

    #[test]
    fn resolves_named_evidence_fragments_exactly() {
        let bnf = "### <path pattern prefix>\n\n<path pattern prefix> ::= \n";
        assert!(evidence_fragment_exists(bnf, "path-pattern-prefix"));
        assert!(!evidence_fragment_exists(bnf, "path-pattern"));

        let markdown = "## Release provenance\n";
        assert!(evidence_fragment_exists(markdown, "release-provenance"));
        assert!(!evidence_fragment_exists(markdown, "provenance"));
    }
}
