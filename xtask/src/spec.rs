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
const WITNESSES: &str = "spec/WITNESSES.toml";
const DEVIATIONS: &str = "spec/DEVIATIONS.toml";
const PINNED_RELEASE: &str = "2024.3";
const PINNED_COMMIT: &str = "677cbafabb8c3c5eed458fd3b1ec0daec8d67d23";

struct Manifest {
    values: Value,
    files: BTreeMap<String, String>,
}

struct Production {
    name: String,
    support: String,
    mapping: String,
    targets: Vec<String>,
    positive_cases: Vec<String>,
    negative_cases: Vec<String>,
    deviations: Vec<String>,
    note: String,
}

struct ProductionMap {
    values: Value,
    productions: Vec<Production>,
}

struct Witness {
    id: String,
    description: String,
    baseline: String,
    entrypoint: String,
    source: String,
    expect: String,
    tokens: Vec<String>,
    diagnostics: Vec<String>,
}

struct WitnessCatalog {
    witnesses: BTreeMap<String, Witness>,
}

struct Deviation {
    id: String,
    status: String,
    productions: Vec<String>,
    positive_case: String,
    negative_case: String,
}

struct DeviationLedger {
    deviations: BTreeMap<String, Deviation>,
}

struct Traceability {
    production_map: ProductionMap,
    witnesses: WitnessCatalog,
    deviations: DeviationLedger,
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

    let traceability = read_traceability(root)?;
    let production_map = &traceability.production_map;
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
    validate_traceability(root, &bnf_productions, &traceability)?;
    execute_witnesses(&traceability.witnesses)?;

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
    let traceability = read_traceability(root)?;
    let mut support = BTreeMap::<String, usize>::new();
    let mut mappings = BTreeMap::<String, usize>::new();
    for production in &traceability.production_map.productions {
        *support.entry(production.support.clone()).or_default() += 1;
        *mappings.entry(production.mapping.clone()).or_default() += 1;
    }
    println!("source: {}", manifest_string(&manifest, "release_url")?);
    println!(
        "grammar sha256: {}",
        manifest_string(&manifest, "grammar_sha256")?
    );
    println!("production support:");
    for (status, count) in &support {
        println!("  {status}: {count}");
    }
    println!("implementation mapping:");
    for (mapping, count) in mappings {
        println!("  {mapping}: {count}");
    }
    let assessed = traceability
        .production_map
        .productions
        .len()
        .saturating_sub(*support.get("unassessed").unwrap_or(&0));
    println!(
        "assessment: {assessed}/{} productions",
        traceability.production_map.productions.len()
    );
    println!(
        "executable witnesses: {}",
        traceability.witnesses.witnesses.len()
    );
    let mut deviation_statuses = BTreeMap::<String, usize>::new();
    for deviation in traceability.deviations.deviations.values() {
        *deviation_statuses
            .entry(deviation.status.clone())
            .or_default() += 1;
    }
    println!("deviations:");
    for (status, count) in deviation_statuses {
        println!("  {status}: {count}");
    }
    println!(
        "release gate: {}",
        release_blockers(&traceability).map_or("ready", |_| "blocked")
    );
    Ok(())
}

/// Enforces the production-traceability requirements for a conforming release.
///
/// Unlike [`verify`], this deliberately rejects an honest in-progress audit.
/// Every pinned BNF production must have full support, all witnesses must pass,
/// and every policy deviation must have been explicitly accepted.
pub fn release_check(root: &Path) -> Result<(), String> {
    verify(root)?;
    let traceability = read_traceability(root)?;
    if let Some(blockers) = release_blockers(&traceability) {
        return Err(format!("openCypher release gate is blocked: {blockers}"));
    }
    crate::tck::release_check(root)?;
    println!(
        "openCypher release gate passed: {} fully supported productions, {} witnesses, zero open deviations",
        traceability.production_map.productions.len(),
        traceability.witnesses.witnesses.len()
    );
    Ok(())
}

fn release_blockers(traceability: &Traceability) -> Option<String> {
    let mut blockers = Vec::new();
    let total = traceability.production_map.productions.len();
    let full = traceability
        .production_map
        .productions
        .iter()
        .filter(|production| production.support == "full")
        .count();
    if total != 377 || full != 377 {
        blockers.push(format!("{full}/377 productions have full support"));
    }
    if !table_bool(&traceability.production_map.values, "assessment_complete").unwrap_or(false) {
        blockers.push("assessment_complete is false".into());
    }
    let open = traceability
        .deviations
        .deviations
        .values()
        .filter(|deviation| deviation.status == "open")
        .count();
    if open != 0 {
        blockers.push(format!("{open} deviations remain open"));
    }
    if blockers.is_empty() {
        None
    } else {
        Some(blockers.join("; "))
    }
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
    require_schema_version(&values, PRODUCTION_MAP, 2)?;
    let productions = values
        .get("production")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{PRODUCTION_MAP}: missing [[production]] records"))?
        .iter()
        .map(|record| {
            Ok(Production {
                name: table_string(record, "name", PRODUCTION_MAP)?.to_owned(),
                support: table_string(record, "support", PRODUCTION_MAP)?.to_owned(),
                mapping: table_string(record, "mapping", PRODUCTION_MAP)?.to_owned(),
                targets: table_string_array(record, "targets", PRODUCTION_MAP)?,
                positive_cases: table_string_array(record, "positive_cases", PRODUCTION_MAP)?,
                negative_cases: table_string_array(record, "negative_cases", PRODUCTION_MAP)?,
                deviations: table_string_array(record, "deviations", PRODUCTION_MAP)?,
                note: table_string(record, "note", PRODUCTION_MAP)?.to_owned(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ProductionMap {
        values,
        productions,
    })
}

fn read_witnesses(root: &Path) -> Result<WitnessCatalog, String> {
    let values = read_toml(root, WITNESSES)?;
    require_schema_version(&values, WITNESSES, 1)?;
    let records = values
        .get("case")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{WITNESSES}: missing [[case]] records"))?;
    let mut witnesses = BTreeMap::new();
    for record in records {
        let witness = Witness {
            id: table_string(record, "id", WITNESSES)?.to_owned(),
            description: table_string(record, "description", WITNESSES)?.to_owned(),
            baseline: table_string(record, "baseline", WITNESSES)?.to_owned(),
            entrypoint: table_string(record, "entrypoint", WITNESSES)?.to_owned(),
            source: table_string(record, "source", WITNESSES)?.to_owned(),
            expect: table_string(record, "expect", WITNESSES)?.to_owned(),
            tokens: optional_string_array(record, "tokens", WITNESSES)?,
            diagnostics: optional_string_array(record, "diagnostics", WITNESSES)?,
        };
        if witness.id.is_empty() {
            return Err(format!("{WITNESSES}: case id must not be empty"));
        }
        if witnesses.insert(witness.id.clone(), witness).is_some() {
            return Err(format!("{WITNESSES}: duplicate case id"));
        }
    }
    Ok(WitnessCatalog { witnesses })
}

fn read_deviations(root: &Path) -> Result<DeviationLedger, String> {
    let values = read_toml(root, DEVIATIONS)?;
    require_schema_version(&values, DEVIATIONS, 2)?;
    let records = values
        .get("deviation")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{DEVIATIONS}: missing [[deviation]] records"))?;
    let mut deviations = BTreeMap::new();
    for record in records {
        let deviation = Deviation {
            id: table_string(record, "id", DEVIATIONS)?.to_owned(),
            status: table_string(record, "status", DEVIATIONS)?.to_owned(),
            productions: table_string_array(record, "productions", DEVIATIONS)?,
            positive_case: table_string(record, "positive_case", DEVIATIONS)?.to_owned(),
            negative_case: table_string(record, "negative_case", DEVIATIONS)?.to_owned(),
        };
        if deviation.id.is_empty() {
            return Err(format!("{DEVIATIONS}: deviation id must not be empty"));
        }
        if deviations.insert(deviation.id.clone(), deviation).is_some() {
            return Err(format!("{DEVIATIONS}: duplicate deviation id"));
        }
    }
    Ok(DeviationLedger { deviations })
}

fn read_traceability(root: &Path) -> Result<Traceability, String> {
    Ok(Traceability {
        production_map: read_production_map(root)?,
        witnesses: read_witnesses(root)?,
        deviations: read_deviations(root)?,
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

fn table_bool(value: &Value, key: &str) -> Result<bool, String> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("missing or invalid {key}"))
}

fn table_string_array(value: &Value, key: &str, path: &str) -> Result<Vec<String>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{path}: missing or invalid {key}"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{path}: {key} must contain only strings"))
        })
        .collect()
}

fn optional_string_array(value: &Value, key: &str, path: &str) -> Result<Vec<String>, String> {
    match value.get(key) {
        Some(_) => table_string_array(value, key, path),
        None => Ok(Vec::new()),
    }
}

fn require_schema_version(values: &Value, path: &str, expected: i64) -> Result<(), String> {
    let found = values
        .get("schema_version")
        .and_then(Value::as_integer)
        .ok_or_else(|| format!("{path}: missing or invalid schema_version"))?;
    if found == expected {
        Ok(())
    } else {
        Err(format!(
            "{path}: schema_version must be {expected}, found {found}"
        ))
    }
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

fn validate_traceability(
    root: &Path,
    bnf_productions: &[String],
    traceability: &Traceability,
) -> Result<(), String> {
    validate_witnesses(&traceability.witnesses)?;
    validate_deviation_evidence(root, &traceability.deviations)?;

    let productions = &traceability.production_map.productions;
    let production_names: BTreeSet<_> = bnf_productions.iter().map(String::as_str).collect();
    let by_name: BTreeMap<_, _> = productions
        .iter()
        .map(|production| (production.name.as_str(), production))
        .collect();
    if by_name.len() != productions.len() {
        return Err(format!("{PRODUCTION_MAP}: duplicate production name"));
    }
    let symbols = SourceSymbols::read(root)?;
    let mut used_cases = BTreeSet::new();

    for production in productions {
        validate_unique_strings(&production.targets, &production.name, "target")?;
        validate_unique_strings(
            &production.positive_cases,
            &production.name,
            "positive case",
        )?;
        validate_unique_strings(
            &production.negative_cases,
            &production.name,
            "negative case",
        )?;
        validate_unique_strings(&production.deviations, &production.name, "deviation")?;

        if !["full", "partial", "unsupported", "unassessed"].contains(&production.support.as_str())
        {
            return Err(format!(
                "{PRODUCTION_MAP}: invalid support for {}: {}",
                production.name, production.support
            ));
        }
        if !["direct", "alias", "none"].contains(&production.mapping.as_str()) {
            return Err(format!(
                "{PRODUCTION_MAP}: invalid mapping for {}: {}",
                production.name, production.mapping
            ));
        }

        match production.support.as_str() {
            "unassessed" => {
                if production.mapping != "none"
                    || !production.targets.is_empty()
                    || !production.positive_cases.is_empty()
                    || !production.negative_cases.is_empty()
                    || !production.note.is_empty()
                {
                    return Err(format!(
                        "{PRODUCTION_MAP}: unassessed production {} may only carry deviation references",
                        production.name
                    ));
                }
            }
            "unsupported" => {
                if production.mapping != "none" || !production.targets.is_empty() {
                    return Err(format!(
                        "{PRODUCTION_MAP}: unsupported production {} must use mapping = none",
                        production.name
                    ));
                }
                require_assessed_evidence(production)?;
                require_open_deviation(production, &traceability.deviations)?;
            }
            "partial" => {
                if production.mapping == "none" {
                    return Err(format!(
                        "{PRODUCTION_MAP}: partial production {} needs a direct or alias mapping",
                        production.name
                    ));
                }
                require_assessed_evidence(production)?;
                require_open_deviation(production, &traceability.deviations)?;
            }
            "full" => {
                if production.mapping == "none" {
                    return Err(format!(
                        "{PRODUCTION_MAP}: fully supported production {} needs a direct or alias mapping",
                        production.name
                    ));
                }
                require_assessed_evidence(production)?;
            }
            _ => unreachable!(),
        }

        match production.mapping.as_str() {
            "direct" => {
                if production.targets.is_empty() {
                    return Err(format!(
                        "{PRODUCTION_MAP}: direct production {} needs at least one target",
                        production.name
                    ));
                }
                for target in &production.targets {
                    symbols.validate_target(target)?;
                }
            }
            "alias" => {
                if production.targets.is_empty()
                    || production
                        .targets
                        .iter()
                        .any(|target| !target.starts_with("production:"))
                {
                    return Err(format!(
                        "{PRODUCTION_MAP}: alias production {} needs production: targets only",
                        production.name
                    ));
                }
                for target in &production.targets {
                    let target = target.trim_start_matches("production:");
                    if target == production.name || !production_names.contains(target) {
                        return Err(format!(
                            "{PRODUCTION_MAP}: invalid alias target for {}: {target}",
                            production.name
                        ));
                    }
                }
            }
            "none" => {
                if !production.targets.is_empty() {
                    return Err(format!(
                        "{PRODUCTION_MAP}: mapping = none requires empty targets for {}",
                        production.name
                    ));
                }
            }
            _ => unreachable!(),
        }

        for case in &production.positive_cases {
            let witness = traceability.witnesses.witnesses.get(case).ok_or_else(|| {
                format!(
                    "{PRODUCTION_MAP}: unknown positive case for {}: {case}",
                    production.name
                )
            })?;
            if witness.baseline != "accept" {
                return Err(format!(
                    "{PRODUCTION_MAP}: positive case {case} for {} is not baseline-accept",
                    production.name
                ));
            }
            validate_production_witness(production, witness, true)?;
            used_cases.insert(case.as_str());
        }
        for case in &production.negative_cases {
            let witness = traceability.witnesses.witnesses.get(case).ok_or_else(|| {
                format!(
                    "{PRODUCTION_MAP}: unknown negative case for {}: {case}",
                    production.name
                )
            })?;
            if witness.baseline != "reject" {
                return Err(format!(
                    "{PRODUCTION_MAP}: negative case {case} for {} is not baseline-reject",
                    production.name
                ));
            }
            validate_production_witness(production, witness, false)?;
            used_cases.insert(case.as_str());
        }

        for deviation_id in &production.deviations {
            let deviation = traceability
                .deviations
                .deviations
                .get(deviation_id)
                .ok_or_else(|| {
                    format!(
                        "{PRODUCTION_MAP}: unknown deviation for {}: {deviation_id}",
                        production.name
                    )
                })?;
            if !deviation.productions.contains(&production.name) {
                return Err(format!(
                    "{PRODUCTION_MAP}: deviation {deviation_id} does not refer back to {}",
                    production.name
                ));
            }
        }
    }

    validate_aliases(&by_name)?;

    let mut deviation_cases = BTreeSet::new();
    for deviation in traceability.deviations.deviations.values() {
        if deviation.productions.is_empty() {
            return Err(format!(
                "{DEVIATIONS}: {} must reference at least one production",
                deviation.id
            ));
        }
        for production_name in &deviation.productions {
            let production = by_name.get(production_name.as_str()).ok_or_else(|| {
                format!(
                    "{DEVIATIONS}: {} references unknown production {production_name}",
                    deviation.id
                )
            })?;
            if !production.deviations.contains(&deviation.id) {
                return Err(format!(
                    "{DEVIATIONS}: {} is not referenced back by production {production_name}",
                    deviation.id
                ));
            }
        }
        let positive = traceability
            .witnesses
            .witnesses
            .get(&deviation.positive_case)
            .ok_or_else(|| {
                format!(
                    "{DEVIATIONS}: {} references unknown case {}",
                    deviation.id, deviation.positive_case
                )
            })?;
        if positive.entrypoint != "parser" || positive.baseline == positive.expect {
            return Err(format!(
                "{DEVIATIONS}: {} positive case {} must be a parser behavior divergence",
                deviation.id, deviation.positive_case
            ));
        }
        let negative = traceability
            .witnesses
            .witnesses
            .get(&deviation.negative_case)
            .ok_or_else(|| {
                format!(
                    "{DEVIATIONS}: {} references unknown case {}",
                    deviation.id, deviation.negative_case
                )
            })?;
        if negative.entrypoint != "parser" || negative.baseline != negative.expect {
            return Err(format!(
                "{DEVIATIONS}: {} negative case {} must be a same-direction parser boundary",
                deviation.id, deviation.negative_case
            ));
        }
        for case in [&deviation.positive_case, &deviation.negative_case] {
            used_cases.insert(case.as_str());
            deviation_cases.insert(case.as_str());
        }
    }

    for witness in traceability.witnesses.witnesses.values() {
        let differs = witness.entrypoint == "parser"
            && ((witness.baseline == "accept" && witness.expect == "reject")
                || (witness.baseline == "reject" && witness.expect == "accept"));
        if differs && !deviation_cases.contains(witness.id.as_str()) {
            return Err(format!(
                "{WITNESSES}: case {} differs from the baseline without a deviation",
                witness.id
            ));
        }
    }

    let orphaned: Vec<_> = traceability
        .witnesses
        .witnesses
        .keys()
        .filter(|case| !used_cases.contains(case.as_str()))
        .cloned()
        .collect();
    if !orphaned.is_empty() {
        return Err(format!(
            "{WITNESSES}: unreferenced cases: {}",
            orphaned.join(", ")
        ));
    }

    if table_bool(&traceability.production_map.values, "assessment_complete")?
        && productions
            .iter()
            .any(|production| production.support == "unassessed")
    {
        return Err(format!(
            "{PRODUCTION_MAP}: assessment_complete is true but unassessed productions remain"
        ));
    }
    Ok(())
}

fn validate_production_witness(
    production: &Production,
    witness: &Witness,
    positive: bool,
) -> Result<(), String> {
    match (positive, witness.entrypoint.as_str()) {
        (true, "parser") if witness.expect != "accept" => Err(format!(
            "{PRODUCTION_MAP}: positive parser case {} for {} must expect accept",
            witness.id, production.name
        )),
        (false, "parser") if witness.expect != "reject" => Err(format!(
            "{PRODUCTION_MAP}: negative parser case {} for {} must expect reject",
            witness.id, production.name
        )),
        (true, "lexer")
            if witness.expect != "tokens"
                || witness.tokens.is_empty()
                || !witness.diagnostics.is_empty()
                || witness.tokens.iter().any(|token| token == "Invalid") =>
        {
            Err(format!(
                "{PRODUCTION_MAP}: positive lexer case {} for {} must declare non-invalid exact tokens and no diagnostics",
                witness.id, production.name
            ))
        }
        (false, "lexer")
            if witness.expect != "tokens"
                || (witness.tokens.is_empty() && witness.diagnostics.is_empty()) =>
        {
            Err(format!(
                "{PRODUCTION_MAP}: negative lexer case {} for {} must declare exact near-miss tokens or diagnostics",
                witness.id, production.name
            ))
        }
        (_, "parser" | "lexer") => Ok(()),
        _ => unreachable!("witness entrypoint was validated"),
    }
}

fn require_assessed_evidence(production: &Production) -> Result<(), String> {
    if production.positive_cases.is_empty() || production.negative_cases.is_empty() {
        return Err(format!(
            "{PRODUCTION_MAP}: assessed production {} needs positive and negative cases",
            production.name
        ));
    }
    if matches!(production.support.as_str(), "partial" | "unsupported")
        && production.note.is_empty()
    {
        return Err(format!(
            "{PRODUCTION_MAP}: {} production {} needs a note",
            production.support, production.name
        ));
    }
    Ok(())
}

fn require_open_deviation(
    production: &Production,
    deviations: &DeviationLedger,
) -> Result<(), String> {
    if production.deviations.iter().any(|id| {
        deviations
            .deviations
            .get(id)
            .is_some_and(|deviation| deviation.status == "open")
    }) {
        Ok(())
    } else {
        Err(format!(
            "{PRODUCTION_MAP}: {} production {} needs an open deviation",
            production.support, production.name
        ))
    }
}

fn validate_unique_strings(values: &[String], production: &str, kind: &str) -> Result<(), String> {
    let unique: BTreeSet<_> = values.iter().collect();
    if unique.len() == values.len() {
        Ok(())
    } else {
        Err(format!(
            "{PRODUCTION_MAP}: duplicate {kind} for production {production}"
        ))
    }
}

fn validate_witnesses(catalog: &WitnessCatalog) -> Result<(), String> {
    for witness in catalog.witnesses.values() {
        if witness.description.is_empty() {
            return Err(format!("{WITNESSES}: {} has no description", witness.id));
        }
        if !["accept", "reject"].contains(&witness.baseline.as_str()) {
            return Err(format!(
                "{WITNESSES}: invalid baseline for {}: {}",
                witness.id, witness.baseline
            ));
        }
        match witness.entrypoint.as_str() {
            "parser" => {
                if !["accept", "reject"].contains(&witness.expect.as_str()) {
                    return Err(format!(
                        "{WITNESSES}: parser case {} must expect accept or reject",
                        witness.id
                    ));
                }
                if !witness.tokens.is_empty() {
                    return Err(format!(
                        "{WITNESSES}: parser case {} may not define tokens",
                        witness.id
                    ));
                }
            }
            "lexer" => {
                if witness.expect != "tokens" {
                    return Err(format!(
                        "{WITNESSES}: lexer case {} must expect tokens",
                        witness.id
                    ));
                }
            }
            entrypoint => {
                return Err(format!(
                    "{WITNESSES}: invalid entrypoint for {}: {entrypoint}",
                    witness.id
                ));
            }
        }
    }
    Ok(())
}

fn validate_deviation_evidence(root: &Path, ledger: &DeviationLedger) -> Result<(), String> {
    let values = read_toml(root, DEVIATIONS)?;
    let deviations = values
        .get("deviation")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{DEVIATIONS}: missing [[deviation]] records"))?;
    let allowed_statuses = ["accepted", "open"];
    for deviation in deviations {
        let id = table_string(deviation, "id", DEVIATIONS)?;
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
        let evidence_contents = fs::read_to_string(&evidence_path)
            .map_err(|error| format!("{DEVIATIONS}: could not read {path}: {error}"))?;
        if let Some(fragment) = fragment {
            if fragment.is_empty() {
                return Err(format!("{DEVIATIONS}: empty evidence fragment for {id}"));
            }
            if !evidence_fragment_exists(&evidence_contents, fragment) {
                return Err(format!(
                    "{DEVIATIONS}: evidence fragment does not exist for {id}: #{fragment}"
                ));
            }
        }
        if path.ends_with(".feature") {
            let scenario = table_string(deviation, "evidence_scenario", DEVIATIONS)?;
            if !evidence_contents
                .lines()
                .any(|line| line.trim() == scenario)
            {
                return Err(format!(
                    "{DEVIATIONS}: evidence scenario does not exist for {id}: {scenario}"
                ));
            }
        }
        for key in [
            "kind",
            "summary",
            "baseline",
            "rationale",
            "positive_case",
            "negative_case",
        ] {
            if table_string(deviation, key, DEVIATIONS)?.is_empty() {
                return Err(format!("{DEVIATIONS}: {id} has an empty {key}"));
            }
        }
    }
    if ledger.deviations.len() != deviations.len() {
        return Err(format!("{DEVIATIONS}: duplicate deviation id"));
    }
    Ok(())
}

struct SourceSymbols {
    lalrpop: BTreeSet<String>,
    raw_tokens: BTreeSet<String>,
    token_kinds: BTreeSet<String>,
    keywords: BTreeSet<String>,
    parser_functions: BTreeSet<String>,
    lexer_functions: BTreeSet<String>,
}

impl SourceSymbols {
    fn read(root: &Path) -> Result<Self, String> {
        let grammar = fs::read_to_string(root.join("grammar/cypher.lalrpop"))
            .map_err(|error| format!("grammar/cypher.lalrpop: {error}"))?;
        let lexer = fs::read_to_string(root.join("src/lexer.rs"))
            .map_err(|error| format!("src/lexer.rs: {error}"))?;
        let parser = fs::read_to_string(root.join("src/parser.rs"))
            .map_err(|error| format!("src/parser.rs: {error}"))?;
        let tokens = fs::read_to_string(root.join("src/token.rs"))
            .map_err(|error| format!("src/token.rs: {error}"))?;
        Ok(Self {
            lalrpop: lalrpop_rules(&grammar),
            raw_tokens: enum_variants(&lexer, "RawToken"),
            token_kinds: enum_variants(&tokens, "TokenKind"),
            keywords: enum_variants(&tokens, "Keyword"),
            parser_functions: rust_functions(&parser),
            lexer_functions: rust_functions(&lexer),
        })
    }

    fn validate_target(&self, target: &str) -> Result<(), String> {
        let exists = if let Some(symbol) = target.strip_prefix("lalrpop:") {
            self.lalrpop.contains(symbol)
        } else if let Some(symbol) = target.strip_prefix("logos:RawToken::") {
            self.raw_tokens.contains(symbol)
        } else if let Some(symbol) = target.strip_prefix("token:TokenKind::") {
            self.token_kinds.contains(symbol)
        } else if let Some(symbol) = target.strip_prefix("token:Keyword::") {
            self.keywords.contains(symbol)
        } else if let Some(symbol) = target.strip_prefix("rust:parser::") {
            self.parser_functions.contains(symbol)
        } else if let Some(symbol) = target.strip_prefix("rust:lexer::") {
            self.lexer_functions.contains(symbol)
        } else {
            return Err(format!(
                "{PRODUCTION_MAP}: unknown target namespace: {target}"
            ));
        };
        if exists {
            Ok(())
        } else {
            Err(format!(
                "{PRODUCTION_MAP}: target does not resolve: {target}"
            ))
        }
    }
}

fn lalrpop_rules(source: &str) -> BTreeSet<String> {
    source
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace))
        .filter_map(|line| {
            let line = line.strip_prefix("pub ").unwrap_or(line);
            let (candidate, _) = line.split_once(':')?;
            (!candidate.is_empty()
                && candidate
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'))
            .then(|| candidate.to_owned())
        })
        .collect()
}

fn rust_functions(source: &str) -> BTreeSet<String> {
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let rest = line
                .strip_prefix("pub(crate) fn ")
                .or_else(|| line.strip_prefix("pub fn "))
                .or_else(|| line.strip_prefix("fn "))?;
            let name: String = rest
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

fn enum_variants(source: &str, enum_name: &str) -> BTreeSet<String> {
    let declarations = [
        format!("enum {enum_name} {{"),
        format!("pub enum {enum_name} {{"),
    ];
    let mut inside = false;
    let mut variants = BTreeSet::new();
    for line in source.lines() {
        let line = line.trim();
        if !inside {
            if declarations.iter().any(|declaration| line == declaration) {
                inside = true;
            }
            continue;
        }
        if line == "}" {
            break;
        }
        if line.is_empty() || line.starts_with("///") || line.starts_with("#[") {
            continue;
        }
        let name: String = line
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        if !name.is_empty() {
            variants.insert(name);
        }
    }
    variants
}

fn validate_aliases(productions: &BTreeMap<&str, &Production>) -> Result<(), String> {
    let mut resolved = BTreeMap::<&str, bool>::new();
    for production in productions.values() {
        if production.mapping != "alias" {
            continue;
        }
        let mut visiting = BTreeSet::new();
        if !alias_reaches_direct(
            production.name.as_str(),
            productions,
            &mut visiting,
            &mut resolved,
        )? {
            return Err(format!(
                "{PRODUCTION_MAP}: alias {} has no direct implementation endpoint",
                production.name
            ));
        }

        let child_support: Vec<_> = production
            .targets
            .iter()
            .map(|target| {
                let name = target.trim_start_matches("production:");
                productions
                    .get(name)
                    .expect("alias target was validated")
                    .support
                    .as_str()
            })
            .collect();
        let expected = if child_support.contains(&"unassessed") {
            "unassessed"
        } else if child_support.iter().all(|status| *status == "full") {
            "full"
        } else if child_support.iter().all(|status| *status == "unsupported") {
            "unsupported"
        } else {
            "partial"
        };
        if production.support != expected {
            return Err(format!(
                "{PRODUCTION_MAP}: alias {} must have support {expected}, found {}",
                production.name, production.support
            ));
        }
    }
    Ok(())
}

fn alias_reaches_direct<'a>(
    name: &'a str,
    productions: &BTreeMap<&'a str, &'a Production>,
    visiting: &mut BTreeSet<&'a str>,
    resolved: &mut BTreeMap<&'a str, bool>,
) -> Result<bool, String> {
    if let Some(result) = resolved.get(name) {
        return Ok(*result);
    }
    if !visiting.insert(name) {
        return Err(format!("{PRODUCTION_MAP}: alias cycle contains {name}"));
    }
    let production = productions
        .get(name)
        .ok_or_else(|| format!("{PRODUCTION_MAP}: unknown alias production {name}"))?;
    let result = match production.mapping.as_str() {
        "direct" => true,
        "none" => false,
        "alias" => {
            let mut any = false;
            for target in &production.targets {
                let target = target.trim_start_matches("production:");
                any |= alias_reaches_direct(target, productions, visiting, resolved)?;
            }
            any
        }
        _ => false,
    };
    visiting.remove(name);
    resolved.insert(name, result);
    Ok(result)
}

fn execute_witnesses(catalog: &WitnessCatalog) -> Result<(), String> {
    for witness in catalog.witnesses.values() {
        match witness.entrypoint.as_str() {
            "parser" => execute_parser_witness(witness)?,
            "lexer" => execute_lexer_witness(witness)?,
            _ => unreachable!("entrypoint was validated"),
        }
    }
    Ok(())
}

fn execute_parser_witness(witness: &Witness) -> Result<(), String> {
    match (witness.expect.as_str(), open_cypher::parse(&witness.source)) {
        ("accept", Ok(_)) => Ok(()),
        ("accept", Err(errors)) => Err(format!(
            "{WITNESSES}: parser case {} unexpectedly failed: {errors}",
            witness.id
        )),
        ("reject", Ok(_)) => Err(format!(
            "{WITNESSES}: parser case {} unexpectedly parsed",
            witness.id
        )),
        ("reject", Err(errors)) => {
            let actual: Vec<_> = errors
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect();
            if actual.contains(&"OCY-P999") {
                return Err(format!(
                    "{WITNESSES}: parser case {} produced an internal diagnostic",
                    witness.id
                ));
            }
            for required in &witness.diagnostics {
                if !actual.contains(&required.as_str()) {
                    return Err(format!(
                        "{WITNESSES}: parser case {} needs diagnostic {required}, found [{}]",
                        witness.id,
                        actual.join(", ")
                    ));
                }
            }
            Ok(())
        }
        _ => unreachable!("parser expectation was validated"),
    }
}

fn execute_lexer_witness(witness: &Witness) -> Result<(), String> {
    let outcome = open_cypher::lex(&witness.source);
    let tokens: Vec<_> = outcome
        .tokens
        .iter()
        .map(|token| format!("{:?}", token.kind))
        .collect();
    if tokens != witness.tokens {
        return Err(format!(
            "{WITNESSES}: lexer case {} expected tokens {:?}, found {:?}",
            witness.id, witness.tokens, tokens
        ));
    }
    let diagnostics: Vec<_> = outcome
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_owned())
        .collect();
    if diagnostics != witness.diagnostics {
        return Err(format!(
            "{WITNESSES}: lexer case {} expected diagnostics {:?}, found {:?}",
            witness.id, witness.diagnostics, diagnostics
        ));
    }
    if diagnostics.iter().any(|code| code == "OCY-P999") {
        return Err(format!(
            "{WITNESSES}: lexer case {} produced an internal diagnostic",
            witness.id
        ));
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
    use super::{
        Deviation, DeviationLedger, Production, ProductionMap, SourceSymbols, Traceability,
        Witness, WitnessCatalog, enum_variants, evidence_fragment_exists, execute_lexer_witness,
        execute_parser_witness, lalrpop_rules, production_names, release_blockers, rust_functions,
        validate_aliases, validate_deviation_evidence, validate_production_witness,
        validate_relative_path, validate_traceability,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use toml::Value;

    static TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    fn production(name: &str, support: &str, mapping: &str, targets: &[&str]) -> Production {
        Production {
            name: name.into(),
            support: support.into(),
            mapping: mapping.into(),
            targets: targets.iter().map(|target| (*target).into()).collect(),
            positive_cases: vec!["yes".into()],
            negative_cases: vec!["no".into()],
            deviations: Vec::new(),
            note: String::new(),
        }
    }

    fn witness(id: &str, baseline: &str, source: &str, expect: &str) -> Witness {
        Witness {
            id: id.into(),
            description: format!("{id} test witness"),
            baseline: baseline.into(),
            entrypoint: "parser".into(),
            source: source.into(),
            expect: expect.into(),
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "open-cypher-spec-tests-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(path.join("grammar")).expect("create grammar fixture");
            fs::create_dir_all(path.join("src")).expect("create source fixture");
            fs::create_dir_all(path.join("spec")).expect("create spec fixture");
            fs::write(path.join("grammar/cypher.lalrpop"), "Rule: () = {};\n")
                .expect("write grammar fixture");
            fs::write(
                path.join("src/lexer.rs"),
                "enum RawToken { Word }\nfn lex() {}\n",
            )
            .expect("write lexer fixture");
            fs::write(path.join("src/parser.rs"), "fn parse_program() {}\n")
                .expect("write parser fixture");
            fs::write(
                path.join("src/token.rs"),
                "pub enum TokenKind { Identifier }\npub enum Keyword { Match }\n",
            )
            .expect("write token fixture");
            fs::write(path.join("spec/evidence.txt"), "traceability evidence\n")
                .expect("write evidence fixture");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn traceability_fixture(root: &Path) -> Traceability {
        let deviation_toml = r#"
            schema_version = 2
            [[deviation]]
            id = "D"
            status = "accepted"
            productions = ["p"]
            kind = "test"
            summary = "test deviation"
            baseline = "test baseline"
            rationale = "test rationale"
            positive_case = "diverge"
            negative_case = "boundary"
            evidence = "evidence.txt"
        "#;
        fs::write(root.join("spec/DEVIATIONS.toml"), deviation_toml)
            .expect("write deviation fixture");

        let mut mapped = production("p", "full", "direct", &["lalrpop:Rule"]);
        mapped.deviations.push("D".into());
        let mut witnesses = BTreeMap::new();
        witnesses.insert("yes".into(), witness("yes", "accept", "RETURN 1", "accept"));
        witnesses.insert("no".into(), witness("no", "reject", "RETURN", "reject"));
        witnesses.insert(
            "diverge".into(),
            witness("diverge", "reject", "RETURN 1", "accept"),
        );
        witnesses.insert(
            "boundary".into(),
            witness("boundary", "reject", "RETURN", "reject"),
        );
        let deviation = Deviation {
            id: "D".into(),
            status: "accepted".into(),
            productions: vec!["p".into()],
            positive_case: "diverge".into(),
            negative_case: "boundary".into(),
        };
        Traceability {
            production_map: ProductionMap {
                values: toml::from_str("assessment_complete = true")
                    .expect("valid production metadata"),
                productions: vec![mapped],
            },
            witnesses: WitnessCatalog { witnesses },
            deviations: DeviationLedger {
                deviations: BTreeMap::from([("D".into(), deviation)]),
            },
        }
    }

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

    #[test]
    fn requires_exact_tck_scenario_evidence() {
        let root = TempRoot::new();
        fs::write(
            root.path().join("spec/case.feature"),
            "Feature: evidence\n\n  Scenario: [1] Exact syntax boundary\n",
        )
        .expect("write feature evidence");
        let ledger = DeviationLedger {
            deviations: BTreeMap::from([(
                "D".into(),
                Deviation {
                    id: "D".into(),
                    status: "accepted".into(),
                    productions: vec!["p".into()],
                    positive_case: "diverge".into(),
                    negative_case: "boundary".into(),
                },
            )]),
        };
        let document = |scenario: &str| {
            format!(
                r#"
                schema_version = 2
                [[deviation]]
                id = "D"
                status = "accepted"
                productions = ["p"]
                kind = "test"
                summary = "test"
                baseline = "test"
                rationale = "test"
                positive_case = "diverge"
                negative_case = "boundary"
                evidence = "case.feature"
                evidence_scenario = "{scenario}"
                "#
            )
        };
        fs::write(
            root.path().join("spec/DEVIATIONS.toml"),
            document("Scenario: [1] Exact syntax boundary"),
        )
        .expect("write deviation ledger");
        assert!(validate_deviation_evidence(root.path(), &ledger).is_ok());

        fs::write(
            root.path().join("spec/DEVIATIONS.toml"),
            document("Scenario: [2] Missing"),
        )
        .expect("rewrite deviation ledger");
        assert!(
            validate_deviation_evidence(root.path(), &ledger)
                .unwrap_err()
                .contains("evidence scenario does not exist")
        );
    }

    #[test]
    fn extracts_stable_source_targets() {
        let grammar = "Program: () = {};\n  NotARule: ()\npub ExprFragment: () = {};\n";
        assert_eq!(
            lalrpop_rules(grammar).into_iter().collect::<Vec<_>>(),
            ["ExprFragment", "Program"]
        );

        let rust = "fn private_fn() {}\npub fn public_fn() {}\nfn\nnot_a_function";
        assert_eq!(
            rust_functions(rust).into_iter().collect::<Vec<_>>(),
            ["private_fn", "public_fn"]
        );

        let tokens = "pub enum TokenKind {\n    /// docs\n    Keyword(Keyword),\n    Integer,\n}\n";
        assert_eq!(
            enum_variants(tokens, "TokenKind")
                .into_iter()
                .collect::<Vec<_>>(),
            ["Integer", "Keyword"]
        );
    }

    #[test]
    fn resolves_every_target_namespace_and_reports_unknown_symbols() {
        let symbols = SourceSymbols {
            lalrpop: BTreeSet::from(["Rule".into()]),
            raw_tokens: BTreeSet::from(["Word".into()]),
            token_kinds: BTreeSet::from(["Identifier".into()]),
            keywords: BTreeSet::from(["Match".into()]),
            parser_functions: BTreeSet::from(["parse_program".into()]),
            lexer_functions: BTreeSet::from(["lex".into()]),
        };
        for target in [
            "lalrpop:Rule",
            "logos:RawToken::Word",
            "token:TokenKind::Identifier",
            "token:Keyword::Match",
            "rust:parser::parse_program",
            "rust:lexer::lex",
        ] {
            assert!(symbols.validate_target(target).is_ok(), "{target}");
        }
        assert!(
            symbols
                .validate_target("lalrpop:Missing")
                .unwrap_err()
                .contains("does not resolve")
        );
        assert!(
            symbols
                .validate_target("other:Rule")
                .unwrap_err()
                .contains("unknown target namespace")
        );
    }

    #[test]
    fn rejects_alias_cycles_and_aliases_without_direct_endpoints() {
        let cycle = [
            production("a", "full", "alias", &["production:b"]),
            production("b", "full", "alias", &["production:a"]),
        ];
        let cycle_by_name = cycle
            .iter()
            .map(|entry| (entry.name.as_str(), entry))
            .collect();
        assert!(
            validate_aliases(&cycle_by_name)
                .unwrap_err()
                .contains("cycle")
        );

        let no_endpoint = [
            production("a", "unsupported", "alias", &["production:b"]),
            production("b", "unsupported", "none", &[]),
        ];
        let no_endpoint_by_name = no_endpoint
            .iter()
            .map(|entry| (entry.name.as_str(), entry))
            .collect();
        assert!(
            validate_aliases(&no_endpoint_by_name)
                .unwrap_err()
                .contains("no direct implementation endpoint")
        );
    }

    #[test]
    fn derives_alias_support_from_children() {
        let wrong = [
            production(
                "wrapper",
                "full",
                "alias",
                &["production:implemented", "production:gap"],
            ),
            production("implemented", "full", "direct", &["lalrpop:Rule"]),
            production("gap", "partial", "direct", &["lalrpop:Rule"]),
        ];
        let wrong_by_name = wrong
            .iter()
            .map(|entry| (entry.name.as_str(), entry))
            .collect();
        assert!(
            validate_aliases(&wrong_by_name)
                .unwrap_err()
                .contains("must have support partial")
        );

        let mut right = wrong;
        right[0].support = "partial".into();
        let right_by_name = right
            .iter()
            .map(|entry| (entry.name.as_str(), entry))
            .collect();
        assert!(validate_aliases(&right_by_name).is_ok());
    }

    #[test]
    fn reports_missing_or_misclassified_and_orphan_witnesses() {
        let root = TempRoot::new();

        let mut missing = traceability_fixture(root.path());
        missing.production_map.productions[0].positive_cases = vec!["missing".into()];
        assert!(
            validate_traceability(root.path(), &["p".into()], &missing)
                .unwrap_err()
                .contains("unknown positive case")
        );

        let mut wrong_baseline = traceability_fixture(root.path());
        wrong_baseline
            .witnesses
            .witnesses
            .get_mut("yes")
            .expect("yes witness")
            .baseline = "reject".into();
        assert!(
            validate_traceability(root.path(), &["p".into()], &wrong_baseline)
                .unwrap_err()
                .contains("is not baseline-accept")
        );

        let mut orphan = traceability_fixture(root.path());
        orphan.witnesses.witnesses.insert(
            "orphan".into(),
            witness("orphan", "accept", "RETURN 2", "accept"),
        );
        assert!(
            validate_traceability(root.path(), &["p".into()], &orphan)
                .unwrap_err()
                .contains("unreferenced cases: orphan")
        );
    }

    #[test]
    fn rejects_production_witnesses_with_wrong_current_expectations() {
        let root = TempRoot::new();
        let mut positive = traceability_fixture(root.path());
        positive
            .witnesses
            .witnesses
            .get_mut("yes")
            .expect("yes witness")
            .expect = "reject".into();
        assert!(
            validate_traceability(root.path(), &["p".into()], &positive)
                .unwrap_err()
                .contains("positive parser case yes")
        );

        let mut negative = traceability_fixture(root.path());
        negative
            .witnesses
            .witnesses
            .get_mut("no")
            .expect("no witness")
            .expect = "accept".into();
        assert!(
            validate_traceability(root.path(), &["p".into()], &negative)
                .unwrap_err()
                .contains("negative parser case no")
        );

        let mapped = production("p", "full", "direct", &["lalrpop:Rule"]);
        let positive_lexer = Witness {
            id: "lex-positive".into(),
            description: "lexer positive".into(),
            baseline: "accept".into(),
            entrypoint: "lexer".into(),
            source: "@".into(),
            expect: "tokens".into(),
            tokens: vec!["Invalid".into()],
            diagnostics: vec!["OCY-L001".into()],
        };
        assert!(
            validate_production_witness(&mapped, &positive_lexer, true)
                .unwrap_err()
                .contains("positive lexer case")
        );

        let mut negative_lexer = positive_lexer;
        negative_lexer.id = "lex-negative".into();
        negative_lexer.baseline = "reject".into();
        negative_lexer.tokens.clear();
        negative_lexer.diagnostics.clear();
        assert!(
            validate_production_witness(&mapped, &negative_lexer, false)
                .unwrap_err()
                .contains("negative lexer case")
        );
    }

    #[test]
    fn requires_reciprocal_deviation_links() {
        let root = TempRoot::new();
        let mut traceability = traceability_fixture(root.path());
        traceability.production_map.productions[0]
            .deviations
            .clear();
        assert!(
            validate_traceability(root.path(), &["p".into()], &traceability)
                .unwrap_err()
                .contains("is not referenced back")
        );
    }

    #[test]
    fn requires_deviation_divergence_and_same_direction_boundary() {
        let root = TempRoot::new();
        let mut no_divergence = traceability_fixture(root.path());
        no_divergence
            .witnesses
            .witnesses
            .get_mut("diverge")
            .expect("divergence witness")
            .baseline = "accept".into();
        assert!(
            validate_traceability(root.path(), &["p".into()], &no_divergence)
                .unwrap_err()
                .contains("must be a parser behavior divergence")
        );

        let mut reversed_boundary = traceability_fixture(root.path());
        reversed_boundary
            .witnesses
            .witnesses
            .get_mut("boundary")
            .expect("boundary witness")
            .expect = "accept".into();
        assert!(
            validate_traceability(root.path(), &["p".into()], &reversed_boundary)
                .unwrap_err()
                .contains("must be a same-direction parser boundary")
        );
    }

    #[test]
    fn executes_parser_and_lexer_witnesses_with_diagnostics() {
        assert!(execute_parser_witness(&witness("ok", "accept", "RETURN 1", "accept")).is_ok());
        assert!(execute_parser_witness(&witness("bad", "reject", "RETURN", "reject")).is_ok());
        assert!(
            execute_parser_witness(&witness("wrong", "accept", "RETURN", "accept"))
                .unwrap_err()
                .contains("unexpectedly failed")
        );

        let lexical = Witness {
            id: "invalid".into(),
            description: "invalid token".into(),
            baseline: "reject".into(),
            entrypoint: "lexer".into(),
            source: "@".into(),
            expect: "tokens".into(),
            tokens: vec!["Invalid".into()],
            diagnostics: vec!["OCY-L001".into()],
        };
        assert!(execute_lexer_witness(&lexical).is_ok());
        let mut wrong_diagnostic = lexical;
        wrong_diagnostic.diagnostics = vec!["OCY-L002".into()];
        assert!(
            execute_lexer_witness(&wrong_diagnostic)
                .unwrap_err()
                .contains("expected diagnostics")
        );
    }

    #[test]
    fn release_blockers_cover_assessment_support_and_open_deviations() {
        let root = TempRoot::new();
        let mut traceability = traceability_fixture(root.path());
        let blockers = release_blockers(&traceability).expect("one production is not 377");
        assert!(blockers.contains("1/377"));

        traceability.production_map.values =
            toml::from_str("assessment_complete = false").expect("valid production metadata");
        traceability
            .deviations
            .deviations
            .get_mut("D")
            .expect("deviation")
            .status = "open".into();
        let blockers = release_blockers(&traceability).expect("multiple blockers");
        assert!(blockers.contains("assessment_complete is false"));
        assert!(blockers.contains("1 deviations remain open"));

        let values: Value =
            toml::from_str("assessment_complete = true").expect("valid production metadata");
        traceability.production_map.values = values;
        traceability.production_map.productions = (0..377)
            .map(|index| production(&format!("p{index}"), "full", "direct", &["lalrpop:Rule"]))
            .collect();
        traceability.deviations.deviations.clear();
        assert!(release_blockers(&traceability).is_none());
    }
}
