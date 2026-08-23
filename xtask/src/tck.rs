// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deterministic syntax-only projection of the pinned openCypher TCK.
//!
//! The upstream TCK describes execution semantics. This module deliberately
//! projects only the Cypher-bearing steps, expands scenario outlines, resolves
//! named graph setup scripts, and assigns a reviewed parser expectation.

use gherkin::{Background, Feature, GherkinEnv, Rule, Scenario, Step, StepType};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

const PINNED_RELEASE: &str = "2024.3";
const PINNED_COMMIT: &str = "677cbafabb8c3c5eed458fd3b1ec0daec8d67d23";
const FEATURES_ROOT: &str = "spec/vendor/openCypher-2024.3/tck/features";
const GRAPHS_ROOT: &str = "spec/vendor/openCypher-2024.3/tck/graphs";
const EXPECTATIONS_FILE: &str = "spec/TCK_EXPECTATIONS.toml";
const EXCLUSIONS_FILE: &str = "spec/TCK_EXCLUSIONS.toml";
const GENERATED_FILE: &str = "spec/generated/TCK_SYNTAX.jsonl";
const REPORT_FILE: &str = "target/tck-syntax-report.json";

const SCHEMA_VERSION: u32 = 1;
const EXPECTED_FEATURE_FILES: usize = 220;
const EXPECTED_SCENARIO_TEMPLATES: usize = 1_615;
const EXPECTED_SCENARIO_OUTLINES: usize = 276;
const EXPECTED_SCENARIOS: usize = 3_897;
const EXPECTED_OCCURRENCES: usize = 4_882;
const EXPECTED_ACTIVE_OCCURRENCES: usize = 4_880;
const EXPECTED_SUPPLEMENTAL_OCCURRENCES: usize = 2;
const EXPECTED_ACCEPT_OCCURRENCES: usize = 4_860;
const EXPECTED_REJECT_OCCURRENCES: usize = 22;
const EXPECTED_REVIEWS: usize = 156;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum Expectation {
    Accept,
    Reject,
}

impl Expectation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Reject => "reject",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum QueryRole {
    Background,
    NamedGraph,
    Setup,
    Primary,
    Control,
}

impl QueryRole {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::NamedGraph => "named_graph",
            Self::Setup => "setup",
            Self::Primary => "primary",
            Self::Control => "control",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum QueryForm {
    Docstring,
    Inline,
    GraphScript,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct UpstreamError {
    error_type: String,
    phase: String,
    detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct HeaderRecord {
    record: &'static str,
    schema_version: u32,
    spdx_license: &'static str,
    release: &'static str,
    commit: &'static str,
    corpus_sha256: String,
    feature_files: usize,
    scenario_templates: usize,
    scenario_outlines: usize,
    scenarios: usize,
    unique_queries: usize,
    occurrences: usize,
    active_occurrences: usize,
    supplemental_occurrences: usize,
    accept_occurrences: usize,
    reject_occurrences: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct QueryRecord {
    record: &'static str,
    id: String,
    sha256: String,
    source: String,
    expectation: Expectation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct OccurrenceRecord {
    record: &'static str,
    id: String,
    query_id: String,
    feature_path: String,
    feature: String,
    rule: Option<String>,
    scenario_template: String,
    scenario: String,
    scenario_line: usize,
    example_index: Option<usize>,
    example_line: Option<usize>,
    sequence: usize,
    role: QueryRole,
    form: QueryForm,
    source_path: String,
    source_line: usize,
    tags: Vec<String>,
    supplemental: bool,
    expectation: Expectation,
    upstream_error: Option<UpstreamError>,
}

#[derive(Debug)]
struct Projection {
    header: HeaderRecord,
    queries: Vec<QueryRecord>,
    occurrences: Vec<OccurrenceRecord>,
}

#[derive(Debug, Deserialize)]
struct ExpectationsLedger {
    schema_version: u32,
    release: String,
    commit: String,
    entry_count: usize,
    #[serde(default)]
    expectation: Vec<ExpectationReview>,
}

#[derive(Clone, Debug, Deserialize)]
struct ExpectationReview {
    feature: String,
    scenario_line: usize,
    scenario: String,
    error_type: String,
    phase: String,
    detail: String,
    expect: Expectation,
    reason: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReviewKey {
    feature: String,
    scenario_line: usize,
    error_type: String,
    phase: String,
    detail: String,
}

#[derive(Debug, Deserialize)]
struct ExclusionsLedger {
    schema_version: u32,
    release: String,
    commit: String,
    exclusion_count: usize,
    #[serde(default)]
    exclusion: Vec<Exclusion>,
}

#[derive(Clone, Debug, Deserialize)]
struct Exclusion {
    occurrence_id: String,
    direction: String,
    issue: String,
    rationale: String,
}

#[derive(Debug, Deserialize)]
struct GraphMetadata {
    name: String,
    scripts: Vec<String>,
}

#[derive(Clone, Debug)]
struct GraphStatement {
    source_path: String,
    source_line: usize,
    source: String,
}

#[derive(Clone, Debug)]
struct GraphFixture {
    statements: Vec<GraphStatement>,
}

#[derive(Default)]
struct ProjectionBuilder {
    occurrences: Vec<PendingOccurrence>,
    feature_files: usize,
    scenario_templates: usize,
    scenario_outlines: usize,
    scenarios: usize,
    seen_reviews: BTreeSet<ReviewKey>,
}

#[derive(Clone, Debug)]
struct PendingOccurrence {
    id: String,
    feature_path: String,
    feature: String,
    rule: Option<String>,
    scenario_template: String,
    scenario: String,
    scenario_line: usize,
    example_index: Option<usize>,
    example_line: Option<usize>,
    sequence: usize,
    role: QueryRole,
    form: QueryForm,
    source_path: String,
    source_line: usize,
    tags: Vec<String>,
    expectation: Expectation,
    upstream_error: Option<UpstreamError>,
    source: String,
}

#[derive(Debug)]
struct ExampleInstance {
    index: Option<usize>,
    line: Option<usize>,
    values: BTreeMap<String, String>,
    tags: Vec<String>,
}

#[derive(Clone, Debug)]
struct ScenarioExpectation {
    expectation: Expectation,
    upstream_error: Option<UpstreamError>,
}

#[derive(Debug)]
struct QueryStep {
    role: QueryRole,
    form: QueryForm,
    source_path: String,
    source_line: usize,
    source: String,
}

#[derive(Clone, Debug, Serialize)]
struct ParserDiagnostic {
    code: String,
    message: String,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, Serialize)]
struct ConformanceFailure {
    occurrence_id: String,
    query_id: String,
    feature_path: String,
    scenario_line: usize,
    role: QueryRole,
    expectation: Expectation,
    supplemental: bool,
    direction: String,
    excluded: bool,
    exclusion_issue: Option<String>,
    source: String,
    diagnostics: Vec<ParserDiagnostic>,
}

#[derive(Debug, Serialize)]
struct ReportCounts {
    occurrences: usize,
    active_occurrences: usize,
    supplemental_occurrences: usize,
    expected_accept: usize,
    expected_reject: usize,
    matched: usize,
    active_matched: usize,
    supplemental_matched: usize,
    excluded: usize,
    unexpected: usize,
    active_unexpected: usize,
    supplemental_unexpected: usize,
    stale_exclusions: usize,
}

#[derive(Debug, Serialize)]
struct FailureGroup {
    direction: String,
    diagnostic_code: String,
    count: usize,
}

#[derive(Debug, Serialize)]
struct ConformanceReport {
    schema_version: u32,
    spdx_license: &'static str,
    release: &'static str,
    commit: &'static str,
    corpus_sha256: String,
    success: bool,
    counts: ReportCounts,
    groups: Vec<FailureGroup>,
    failures: Vec<ConformanceFailure>,
    stale_exclusions: Vec<String>,
}

pub fn generate(root: &Path) -> Result<(), String> {
    let projection = build_projection(root)?;
    let rendered = render_projection(&projection)?;
    let destination = root.join(GENERATED_FILE);
    let parent = destination
        .parent()
        .ok_or_else(|| format!("{GENERATED_FILE} has no parent directory"))?;
    fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    fs::write(&destination, rendered)
        .map_err(|error| format!("{}: {error}", destination.display()))?;
    println!(
        "generated {GENERATED_FILE}: {} unique queries, {} occurrences ({} accept, {} reject)",
        projection.queries.len(),
        projection.occurrences.len(),
        projection.header.accept_occurrences,
        projection.header.reject_occurrences
    );
    Ok(())
}

pub fn verify(root: &Path) -> Result<(), String> {
    let projection = build_projection(root)?;
    verify_rendered(root, &projection)?;
    println!(
        "verified {GENERATED_FILE}: {} unique queries, {} occurrences",
        projection.queries.len(),
        projection.occurrences.len()
    );
    Ok(())
}

pub fn check(root: &Path) -> Result<(), String> {
    let projection = build_projection(root)?;
    verify_rendered(root, &projection)?;
    let report = evaluate(root, &projection)?;
    print_report(&report);
    gate_report(&report)
}

pub fn report(root: &Path) -> Result<(), String> {
    let projection = build_projection(root)?;
    verify_rendered(root, &projection)?;
    let report = evaluate(root, &projection)?;
    let destination = root.join(REPORT_FILE);
    let parent = destination
        .parent()
        .ok_or_else(|| format!("{REPORT_FILE} has no parent directory"))?;
    fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    let mut rendered =
        serde_json::to_string_pretty(&report).map_err(|error| format!("report JSON: {error}"))?;
    rendered.push('\n');
    fs::write(&destination, rendered)
        .map_err(|error| format!("{}: {error}", destination.display()))?;
    print_report(&report);
    println!("wrote {REPORT_FILE}");
    gate_report(&report)
}

fn gate_report(report: &ConformanceReport) -> Result<(), String> {
    if report.success {
        Ok(())
    } else {
        Err(format!(
            "TCK syntax projection has {} unexpected failure(s) and {} stale exclusion(s)",
            report.counts.unexpected, report.counts.stale_exclusions
        ))
    }
}

fn build_projection(root: &Path) -> Result<Projection, String> {
    let reviews = read_expectations(root)?;
    let graphs = read_graphs(root)?;
    let feature_paths = files_with_extension(&root.join(FEATURES_ROOT), "feature")?;
    let corpus_sha256 = corpus_sha256(root, &feature_paths)?;
    let mut builder = ProjectionBuilder::default();

    for path in &feature_paths {
        let feature_source =
            fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let feature_lines: Vec<_> = feature_source.lines().collect();
        let parse_source = gherkin_compatible_source(&feature_source);
        let feature = Feature::parse(&parse_source, GherkinEnv::default())
            .map_err(|error| format!("{}: {error:?}", path.display()))?;
        let feature_path = spec_relative(root, path)?;
        builder.feature_files += 1;
        process_feature(
            &mut builder,
            &feature,
            &feature_path,
            &reviews,
            &graphs,
            &feature_lines,
        )?;
    }

    validate_seen_reviews(&reviews, &builder.seen_reviews)?;
    validate_projection_counts(&builder)?;
    finish_projection(builder, corpus_sha256)
}

// openCypher 2024.3 predates gherkin 0.16's strict table escaping. Its result
// tables contain `\'`, which older Gherkin implementations treated literally.
// Escape only otherwise-invalid backslashes on table rows; Cypher docstrings
// and all source line numbers remain byte-for-byte semantically unchanged.
fn gherkin_compatible_source(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        if line.trim_start().starts_with('|') {
            let mut chars = line.chars().peekable();
            while let Some(character) = chars.next() {
                if character == '\\' {
                    match chars.peek().copied() {
                        Some('n' | '|' | '\\') => {
                            output.push('\\');
                            output.push(chars.next().expect("peeked table escape"));
                            continue;
                        }
                        Some(_) => output.push_str("\\\\"),
                        None => output.push_str("\\\\"),
                    }
                } else {
                    output.push(character);
                }
            }
        } else {
            output.push_str(line);
        }
    }
    output
}

fn process_feature(
    builder: &mut ProjectionBuilder,
    feature: &Feature,
    feature_path: &str,
    reviews: &BTreeMap<ReviewKey, ExpectationReview>,
    graphs: &BTreeMap<String, GraphFixture>,
    feature_lines: &[&str],
) -> Result<(), String> {
    for scenario in &feature.scenarios {
        process_scenario(
            builder,
            feature,
            None,
            scenario,
            feature.background.as_ref(),
            None,
            feature_path,
            reviews,
            graphs,
            feature_lines,
        )?;
    }

    for rule in &feature.rules {
        for scenario in &rule.scenarios {
            process_scenario(
                builder,
                feature,
                Some(rule),
                scenario,
                feature.background.as_ref(),
                rule.background.as_ref(),
                feature_path,
                reviews,
                graphs,
                feature_lines,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_scenario(
    builder: &mut ProjectionBuilder,
    feature: &Feature,
    rule: Option<&Rule>,
    scenario: &Scenario,
    feature_background: Option<&Background>,
    rule_background: Option<&Background>,
    feature_path: &str,
    reviews: &BTreeMap<ReviewKey, ExpectationReview>,
    graphs: &BTreeMap<String, GraphFixture>,
    feature_lines: &[&str],
) -> Result<(), String> {
    builder.scenario_templates += 1;
    if !scenario.examples.is_empty() {
        builder.scenario_outlines += 1;
    }

    let scenario_expectation =
        scenario_expectation(feature_path, scenario, reviews, &mut builder.seen_reviews)?;
    let instances = expand_examples(scenario, feature_path, feature_lines)?;
    let base_tags = combined_tags(
        feature.tags.iter(),
        rule.into_iter().flat_map(|rule| rule.tags.iter()),
        scenario.tags.iter(),
    );

    for instance in instances {
        builder.scenarios += 1;
        let mut tags = base_tags.clone();
        tags.extend(instance.tags.iter().map(|tag| normalize_tag(tag)));
        tags.sort();
        tags.dedup();

        let scenario_name = substitute(&scenario.name, &instance.values).map_err(|error| {
            format!(
                "{feature_path}:{} scenario name: {error}",
                scenario.position.line
            )
        })?;
        let context = ScenarioContext {
            feature_path,
            feature_name: &feature.name,
            rule_name: rule.map(|rule| rule.name.as_str()),
            scenario,
            scenario_name: &scenario_name,
            example_index: instance.index,
            example_line: instance.line,
            values: &instance.values,
            tags: &tags,
            expectation: &scenario_expectation,
        };
        let mut sequence = 0;
        if let Some(background) = feature_background {
            extract_steps(
                builder,
                &context,
                &background.steps,
                StepContext::Background,
                graphs,
                &mut sequence,
            )?;
        }
        if let Some(background) = rule_background {
            extract_steps(
                builder,
                &context,
                &background.steps,
                StepContext::Background,
                graphs,
                &mut sequence,
            )?;
        }
        extract_steps(
            builder,
            &context,
            &scenario.steps,
            StepContext::Scenario,
            graphs,
            &mut sequence,
        )?;

        let primary_count = builder
            .occurrences
            .iter()
            .rev()
            .take(sequence)
            .filter(|occurrence| occurrence.role == QueryRole::Primary)
            .count();
        if primary_count != 1 {
            return Err(format!(
                "{feature_path}:{} `{}` materialized {} primary queries; expected exactly one",
                scenario.position.line, scenario_name, primary_count
            ));
        }
    }
    Ok(())
}

struct ScenarioContext<'a> {
    feature_path: &'a str,
    feature_name: &'a str,
    rule_name: Option<&'a str>,
    scenario: &'a Scenario,
    scenario_name: &'a str,
    example_index: Option<usize>,
    example_line: Option<usize>,
    values: &'a BTreeMap<String, String>,
    tags: &'a [String],
    expectation: &'a ScenarioExpectation,
}

#[derive(Clone, Copy)]
enum StepContext {
    Background,
    Scenario,
}

fn extract_steps(
    builder: &mut ProjectionBuilder,
    context: &ScenarioContext<'_>,
    steps: &[Step],
    step_context: StepContext,
    graphs: &BTreeMap<String, GraphFixture>,
    sequence: &mut usize,
) -> Result<(), String> {
    for step in steps {
        match classify_step(step, step_context, context.feature_path, context.values)? {
            ClassifiedStep::None => {}
            ClassifiedStep::Query(query) => {
                push_occurrence(builder, context, query, sequence);
            }
            ClassifiedStep::NamedGraph(name) => {
                let fixture = graphs.get(&name).ok_or_else(|| {
                    format!(
                        "{}:{} references unknown named graph `{name}`",
                        context.feature_path, step.position.line
                    )
                })?;
                for statement in &fixture.statements {
                    push_occurrence(
                        builder,
                        context,
                        QueryStep {
                            role: QueryRole::NamedGraph,
                            form: QueryForm::GraphScript,
                            source_path: statement.source_path.clone(),
                            source_line: statement.source_line,
                            source: statement.source.clone(),
                        },
                        sequence,
                    );
                }
            }
        }
    }
    Ok(())
}

fn push_occurrence(
    builder: &mut ProjectionBuilder,
    context: &ScenarioContext<'_>,
    query: QueryStep,
    sequence: &mut usize,
) {
    *sequence += 1;
    let expectation = if query.role == QueryRole::Primary {
        context.expectation.expectation
    } else {
        Expectation::Accept
    };
    let upstream_error = if query.role == QueryRole::Primary {
        context.expectation.upstream_error.clone()
    } else {
        None
    };
    let id = occurrence_id(
        context.feature_path,
        context.scenario.position.line,
        context.example_line,
        context.example_index,
        *sequence,
    );
    builder.occurrences.push(PendingOccurrence {
        id,
        feature_path: context.feature_path.to_owned(),
        feature: context.feature_name.to_owned(),
        rule: context.rule_name.map(str::to_owned),
        scenario_template: context.scenario.name.clone(),
        scenario: context.scenario_name.to_owned(),
        scenario_line: context.scenario.position.line,
        example_index: context.example_index,
        example_line: context.example_line,
        sequence: *sequence,
        role: query.role,
        form: query.form,
        source_path: query.source_path,
        source_line: query.source_line,
        tags: context.tags.to_vec(),
        expectation,
        upstream_error,
        source: normalize_source(&query.source),
    });
}

fn occurrence_id(
    feature_path: &str,
    scenario_line: usize,
    example_line: Option<usize>,
    example_index: Option<usize>,
    sequence: usize,
) -> String {
    format!(
        "occ:{feature_path}:{scenario_line}:{}:{}:{sequence}",
        example_line.unwrap_or(0),
        example_index.unwrap_or(0)
    )
}

enum ClassifiedStep {
    None,
    Query(QueryStep),
    NamedGraph(String),
}

fn classify_step(
    step: &Step,
    context: StepContext,
    feature_path: &str,
    values: &BTreeMap<String, String>,
) -> Result<ClassifiedStep, String> {
    let value = substitute(&step.value, values)
        .map_err(|error| format!("{feature_path}:{} step text: {error}", step.position.line))?;
    let location = || format!("{feature_path}:{} `{value}`", step.position.line);

    let query_kind = if value == "having executed:" {
        Some((
            match context {
                StepContext::Background => QueryRole::Background,
                StepContext::Scenario => QueryRole::Setup,
            },
            None,
        ))
    } else if value == "executing query:" {
        Some((QueryRole::Primary, None))
    } else if value == "executing control query:" {
        Some((QueryRole::Control, None))
    } else if let Some(source) = value.strip_prefix("having executed: ") {
        Some((
            match context {
                StepContext::Background => QueryRole::Background,
                StepContext::Scenario => QueryRole::Setup,
            },
            Some(source),
        ))
    } else if let Some(source) = value.strip_prefix("executing query: ") {
        Some((QueryRole::Primary, Some(source)))
    } else {
        value
            .strip_prefix("executing control query: ")
            .map(|source| (QueryRole::Control, Some(source)))
    };

    if let Some((role, inline)) = query_kind {
        if step.table.is_some() {
            return Err(format!("{} has an unexpected data table", location()));
        }
        let (form, source) = match (&step.docstring, inline) {
            (Some(_), Some(_)) => {
                return Err(format!(
                    "{} has both an inline query and a docstring",
                    location()
                ));
            }
            (Some(docstring), None) => (QueryForm::Docstring, docstring.as_str()),
            (None, Some(inline)) if !inline.trim().is_empty() => (QueryForm::Inline, inline),
            (None, _) => return Err(format!("{} has no query body", location())),
        };
        let source = substitute(source, values)
            .map_err(|error| format!("{} query body: {error}", location()))?;
        return Ok(ClassifiedStep::Query(QueryStep {
            role,
            form,
            source_path: feature_path.to_owned(),
            source_line: step.position.line,
            source,
        }));
    }

    if step.docstring.is_some() {
        return Err(format!(
            "{} has a docstring but is not a recognized query step",
            location()
        ));
    }

    if step.ty == StepType::Given
        && let Some(name) = value
            .strip_prefix("the ")
            .and_then(|value| value.strip_suffix(" graph"))
    {
        if name.is_empty() || step.table.is_some() {
            return Err(format!("{} is not a valid named graph step", location()));
        }
        return Ok(ClassifiedStep::NamedGraph(name.to_owned()));
    }

    let table_expected = value == "parameters are:"
        || value == "the result should be, in any order:"
        || value == "the result should be, in order:"
        || value == "the result should be (ignoring element order for lists):"
        || value == "the result should be, in order (ignoring element order for lists):"
        || value == "the side effects should be:"
        || (value.starts_with("there exists a procedure ") && value.ends_with(':'));
    if table_expected {
        if step.table.is_none() {
            return Err(format!("{} requires a data table", location()));
        }
        return Ok(ClassifiedStep::None);
    }

    let no_argument = value == "an empty graph"
        || value == "any graph"
        || value == "the result should be empty"
        || value == "no side effects"
        || parse_error_step(&value).is_some();
    if no_argument {
        if step.table.is_some() {
            return Err(format!("{} has an unexpected data table", location()));
        }
        return Ok(ClassifiedStep::None);
    }

    Err(format!(
        "{} is an unrecognized TCK step; update the explicit projection allowlist",
        location()
    ))
}

fn scenario_expectation(
    feature_path: &str,
    scenario: &Scenario,
    reviews: &BTreeMap<ReviewKey, ExpectationReview>,
    seen_reviews: &mut BTreeSet<ReviewKey>,
) -> Result<ScenarioExpectation, String> {
    let errors: Vec<_> = scenario
        .steps
        .iter()
        .filter_map(|step| parse_error_step(&step.value))
        .collect();
    if errors.len() > 1 {
        return Err(format!(
            "{feature_path}:{} `{}` has more than one error expectation",
            scenario.position.line, scenario.name
        ));
    }
    let upstream_error = errors.into_iter().next();
    let mut expectation = Expectation::Accept;

    if let Some(error) = &upstream_error
        && error.error_type == "SyntaxError"
        && error.phase == "compile time"
    {
        let key = ReviewKey {
            feature: feature_path.to_owned(),
            scenario_line: scenario.position.line,
            error_type: error.error_type.clone(),
            phase: error.phase.clone(),
            detail: error.detail.clone(),
        };
        let review = reviews.get(&key).ok_or_else(|| {
            format!(
                "{feature_path}:{} `{}` has an unreviewed compile-time SyntaxError `{}`",
                scenario.position.line, scenario.name, error.detail
            )
        })?;
        if review.scenario != scenario.name {
            return Err(format!(
                "{EXPECTATIONS_FILE}: reviewed scenario name mismatch for {feature_path}:{}: expected `{}`, found `{}`",
                scenario.position.line, review.scenario, scenario.name
            ));
        }
        let skip_grammar = scenario
            .tags
            .iter()
            .any(|tag| normalize_tag(tag) == "skipGrammarCheck");
        match (review.expect, review.reason.as_str(), skip_grammar) {
            (Expectation::Accept, "static_semantic", false)
            | (Expectation::Reject, "grammar_negative", true) => {}
            (Expectation::Accept, _, _) => {
                return Err(format!(
                    "{EXPECTATIONS_FILE}: accepted review for {feature_path}:{} must use reason `static_semantic` and must not carry @skipGrammarCheck",
                    scenario.position.line
                ));
            }
            (Expectation::Reject, _, _) => {
                return Err(format!(
                    "{EXPECTATIONS_FILE}: rejected review for {feature_path}:{} must use reason `grammar_negative` and carry @skipGrammarCheck",
                    scenario.position.line
                ));
            }
        }
        expectation = review.expect;
        seen_reviews.insert(key);
    }

    if scenario
        .tags
        .iter()
        .any(|tag| normalize_tag(tag) == "skipGrammarCheck")
        && expectation != Expectation::Reject
    {
        return Err(format!(
            "{feature_path}:{} @skipGrammarCheck is not backed by an explicit rejected compile-time SyntaxError review",
            scenario.position.line
        ));
    }

    Ok(ScenarioExpectation {
        expectation,
        upstream_error,
    })
}

fn parse_error_step(value: &str) -> Option<UpstreamError> {
    let rest = value.strip_prefix("a ")?;
    let (error_type, rest) = rest.split_once(" should be raised at ")?;
    let (phase, detail) = rest.split_once(": ")?;
    if error_type.is_empty()
        || detail.is_empty()
        || !matches!(phase, "compile time" | "runtime" | "any time")
    {
        return None;
    }
    Some(UpstreamError {
        error_type: error_type.to_owned(),
        phase: phase.to_owned(),
        detail: detail.to_owned(),
    })
}

fn expand_examples(
    scenario: &Scenario,
    feature_path: &str,
    feature_lines: &[&str],
) -> Result<Vec<ExampleInstance>, String> {
    if scenario.examples.is_empty() {
        if scenario.keyword.contains("Outline") || scenario.keyword.contains("Template") {
            return Err(format!(
                "{feature_path}:{} outline `{}` has no Examples table",
                scenario.position.line, scenario.name
            ));
        }
        return Ok(vec![ExampleInstance {
            index: None,
            line: None,
            values: BTreeMap::new(),
            tags: Vec::new(),
        }]);
    }

    let mut expanded = Vec::new();
    let mut index = 0;
    for examples in &scenario.examples {
        let table = examples.table.as_ref().ok_or_else(|| {
            format!(
                "{feature_path}:{} Examples for `{}` has no table",
                examples.position.line, scenario.name
            )
        })?;
        let (headers, rows) = table.rows.split_first().ok_or_else(|| {
            format!(
                "{feature_path}:{} Examples for `{}` has an empty table",
                examples.position.line, scenario.name
            )
        })?;
        if headers.is_empty() {
            return Err(format!(
                "{feature_path}:{} Examples for `{}` has no columns",
                examples.position.line, scenario.name
            ));
        }
        validate_example_headers(headers).map_err(|error| {
            format!(
                "{feature_path}:{} Examples for `{}`: {error}",
                examples.position.line, scenario.name
            )
        })?;
        if rows.is_empty() {
            return Err(format!(
                "{feature_path}:{} Examples for `{}` has no data rows",
                examples.position.line, scenario.name
            ));
        }
        let row_lines = table_row_lines(table.position.line, table.rows.len(), feature_lines)
            .map_err(|error| {
                format!(
                    "{feature_path}:{} Examples for `{}`: {error}",
                    examples.position.line, scenario.name
                )
            })?;
        for (row, line) in rows.iter().zip(row_lines.into_iter().skip(1)) {
            index += 1;
            let values = headers.iter().cloned().zip(row.iter().cloned()).collect();
            expanded.push(ExampleInstance {
                index: Some(index),
                line: Some(line),
                values,
                tags: examples.tags.clone(),
            });
        }
    }
    Ok(expanded)
}

fn validate_example_headers(headers: &[String]) -> Result<(), String> {
    let unique: BTreeSet<_> = headers.iter().collect();
    if unique.len() == headers.len() {
        Ok(())
    } else {
        Err("duplicate Examples headers".into())
    }
}

fn table_row_lines(
    first_line: usize,
    expected_rows: usize,
    feature_lines: &[&str],
) -> Result<Vec<usize>, String> {
    let mut lines = Vec::with_capacity(expected_rows);
    for (offset, source) in feature_lines
        .iter()
        .enumerate()
        .skip(first_line.saturating_sub(1))
    {
        if source.trim_start().starts_with('|') {
            lines.push(offset + 1);
            if lines.len() == expected_rows {
                return Ok(lines);
            }
        }
    }
    Err(format!(
        "could not locate all {expected_rows} source rows beginning at line {first_line}"
    ))
}

fn substitute(template: &str, values: &BTreeMap<String, String>) -> Result<String, String> {
    let mut output = String::with_capacity(template.len());
    let mut cursor = 0;
    while cursor < template.len() {
        let remaining = &template[cursor..];
        let replacement = values.iter().find_map(|(name, value)| {
            let placeholder = format!("<{name}>");
            remaining
                .starts_with(&placeholder)
                .then_some((placeholder.len(), value))
        });
        if let Some((length, value)) = replacement {
            output.push_str(value);
            cursor += length;
        } else {
            if let Some(rest) = remaining.strip_prefix('<')
                && let Some(end) = rest.find('>')
            {
                let name = &rest[..end];
                if is_placeholder_name(name) {
                    return Err(format!("unresolved Examples placeholder `<{name}>`"));
                }
            }
            let character = remaining
                .chars()
                .next()
                .expect("cursor is before the string end");
            output.push(character);
            cursor += character.len_utf8();
        }
    }
    Ok(output)
}

fn is_placeholder_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(byte) if byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn combined_tags<'a>(
    feature: impl Iterator<Item = &'a String>,
    rule: impl Iterator<Item = &'a String>,
    scenario: impl Iterator<Item = &'a String>,
) -> Vec<String> {
    let mut tags: Vec<_> = feature
        .chain(rule)
        .chain(scenario)
        .map(|tag| normalize_tag(tag))
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

fn normalize_tag(tag: &str) -> String {
    tag.trim().trim_start_matches('@').to_owned()
}

fn normalize_source(source: &str) -> String {
    source
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_owned()
}

fn read_expectations(root: &Path) -> Result<BTreeMap<ReviewKey, ExpectationReview>, String> {
    let ledger: ExpectationsLedger = read_toml(root, EXPECTATIONS_FILE)?;
    validate_ledger_header(
        EXPECTATIONS_FILE,
        ledger.schema_version,
        &ledger.release,
        &ledger.commit,
    )?;
    if ledger.entry_count != ledger.expectation.len() {
        return Err(format!(
            "{EXPECTATIONS_FILE}: entry_count is {}, but {} entries were found",
            ledger.entry_count,
            ledger.expectation.len()
        ));
    }
    if ledger.entry_count != EXPECTED_REVIEWS {
        return Err(format!(
            "{EXPECTATIONS_FILE}: expected {EXPECTED_REVIEWS} reviewed entries for the pinned corpus, found {}",
            ledger.entry_count
        ));
    }
    let mut reviews = BTreeMap::new();
    for review in ledger.expectation {
        validate_spec_relative(&review.feature)?;
        if review.scenario.trim().is_empty() || review.detail.trim().is_empty() {
            return Err(format!(
                "{EXPECTATIONS_FILE}: review at {}:{} has empty review fields",
                review.feature, review.scenario_line
            ));
        }
        if review.error_type != "SyntaxError" || review.phase != "compile time" {
            return Err(format!(
                "{EXPECTATIONS_FILE}: {}:{} is not a compile-time SyntaxError review",
                review.feature, review.scenario_line
            ));
        }
        let key = ReviewKey {
            feature: review.feature.clone(),
            scenario_line: review.scenario_line,
            error_type: review.error_type.clone(),
            phase: review.phase.clone(),
            detail: review.detail.clone(),
        };
        if reviews.insert(key, review).is_some() {
            return Err(format!(
                "{EXPECTATIONS_FILE}: duplicate reviewed expectation key"
            ));
        }
    }
    Ok(reviews)
}

fn validate_seen_reviews(
    reviews: &BTreeMap<ReviewKey, ExpectationReview>,
    seen: &BTreeSet<ReviewKey>,
) -> Result<(), String> {
    let expected: BTreeSet<_> = reviews.keys().cloned().collect();
    if expected == *seen {
        return Ok(());
    }
    let missing: Vec<_> = expected
        .difference(seen)
        .map(|key| format!("{}:{}:{}", key.feature, key.scenario_line, key.detail))
        .collect();
    let extra: Vec<_> = seen
        .difference(&expected)
        .map(|key| format!("{}:{}:{}", key.feature, key.scenario_line, key.detail))
        .collect();
    Err(format!(
        "{EXPECTATIONS_FILE} does not exactly cover the pinned compile-time SyntaxError templates; stale [{}]; unreviewed [{}]",
        missing.join(", "),
        extra.join(", ")
    ))
}

fn read_graphs(root: &Path) -> Result<BTreeMap<String, GraphFixture>, String> {
    let graph_root = root.join(GRAPHS_ROOT);
    let metadata_paths = files_with_extension(&graph_root, "json")?;
    let mut graphs = BTreeMap::new();
    let mut referenced_scripts = BTreeSet::new();

    for metadata_path in metadata_paths {
        let metadata_source = fs::read_to_string(&metadata_path)
            .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
        let metadata: GraphMetadata = serde_json::from_str(&metadata_source)
            .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
        if metadata.name.trim().is_empty() || metadata.scripts.is_empty() {
            return Err(format!(
                "{}: named graph must have a name and at least one script",
                metadata_path.display()
            ));
        }
        let directory = metadata_path
            .parent()
            .ok_or_else(|| format!("{} has no parent", metadata_path.display()))?;
        let mut statements = Vec::new();
        for script in metadata.scripts {
            validate_graph_script_name(&script)?;
            let filename = if script.ends_with(".cypher") {
                script
            } else {
                format!("{script}.cypher")
            };
            let path = directory.join(&filename);
            let relative = spec_relative(root, &path)?;
            let source = fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let split = split_cypher_script(&source)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if split.is_empty() {
                return Err(format!("{} contains no Cypher statements", path.display()));
            }
            referenced_scripts.insert(path.clone());
            statements.extend(
                split
                    .into_iter()
                    .map(|(source_line, source)| GraphStatement {
                        source_path: relative.clone(),
                        source_line,
                        source,
                    }),
            );
        }
        if graphs
            .insert(metadata.name.clone(), GraphFixture { statements })
            .is_some()
        {
            return Err(format!("duplicate named graph `{}`", metadata.name));
        }
    }

    let actual_scripts: BTreeSet<_> = files_with_extension(&graph_root, "cypher")?
        .into_iter()
        .collect();
    if actual_scripts != referenced_scripts {
        let unreferenced: Vec<_> = actual_scripts
            .difference(&referenced_scripts)
            .map(|path| path.display().to_string())
            .collect();
        let missing: Vec<_> = referenced_scripts
            .difference(&actual_scripts)
            .map(|path| path.display().to_string())
            .collect();
        return Err(format!(
            "named graph script inventory differs: unreferenced [{}]; missing [{}]",
            unreferenced.join(", "),
            missing.join(", ")
        ));
    }
    Ok(graphs)
}

fn validate_graph_script_name(name: &str) -> Result<(), String> {
    let path = Path::new(name);
    if name.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        Err(format!("invalid named graph script path `{name}`"))
    } else {
        Ok(())
    }
}

fn split_cypher_script(source: &str) -> Result<Vec<(usize, String)>, String> {
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum State {
        Normal,
        SingleQuote,
        DoubleQuote,
        Backtick,
        LineComment,
        BlockComment(usize),
    }

    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let bytes = normalized.as_bytes();
    let mut state = State::Normal;
    let mut statements = Vec::new();
    let mut start = 0;
    let mut start_line = 1;
    let mut line = 1;
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        match state {
            State::Normal => match (byte, next) {
                (b'/', Some(b'/')) => {
                    state = State::LineComment;
                    index += 2;
                    continue;
                }
                (b'/', Some(b'*')) => {
                    state = State::BlockComment(1);
                    index += 2;
                    continue;
                }
                (b'\'', _) => state = State::SingleQuote,
                (b'"', _) => state = State::DoubleQuote,
                (b'`', _) => state = State::Backtick,
                (b';', _) => {
                    if let Some(statement) = trimmed_statement(&normalized[start..index]) {
                        statements.push((start_line, statement));
                    }
                    start = index + 1;
                    start_line = line;
                }
                _ => {}
            },
            State::SingleQuote => {
                if byte == b'\\' {
                    index += usize::from(next.is_some());
                } else if byte == b'\'' {
                    state = State::Normal;
                }
            }
            State::DoubleQuote => {
                if byte == b'\\' {
                    index += usize::from(next.is_some());
                } else if byte == b'"' {
                    state = State::Normal;
                }
            }
            State::Backtick => {
                if byte == b'`' && next == Some(b'`') {
                    index += 1;
                } else if byte == b'`' {
                    state = State::Normal;
                }
            }
            State::LineComment => {
                if byte == b'\n' {
                    state = State::Normal;
                }
            }
            State::BlockComment(depth) => match (byte, next) {
                (b'/', Some(b'*')) => {
                    state = State::BlockComment(depth + 1);
                    index += 2;
                    continue;
                }
                (b'*', Some(b'/')) => {
                    state = if depth == 1 {
                        State::Normal
                    } else {
                        State::BlockComment(depth - 1)
                    };
                    index += 2;
                    continue;
                }
                _ => {}
            },
        }
        if byte == b'\n' {
            line += 1;
            if start == index + 1 {
                start_line = line;
            }
        }
        index += 1;
    }

    match state {
        State::SingleQuote => return Err("unterminated single-quoted string".into()),
        State::DoubleQuote => return Err("unterminated double-quoted string".into()),
        State::Backtick => return Err("unterminated backtick identifier".into()),
        State::BlockComment(_) => return Err("unterminated block comment".into()),
        State::Normal | State::LineComment => {}
    }
    if let Some(statement) = trimmed_statement(&normalized[start..]) {
        statements.push((start_line, statement));
    }
    Ok(statements)
}

fn trimmed_statement(source: &str) -> Option<String> {
    let trimmed = source.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn finish_projection(
    mut builder: ProjectionBuilder,
    corpus_sha256: String,
) -> Result<Projection, String> {
    sort_pending_occurrences(&mut builder.occurrences);

    let queries = query_records(&builder.occurrences)?;
    let query_ids: BTreeMap<_, _> = queries
        .iter()
        .map(|query| (query.sha256.clone(), query.id.clone()))
        .collect();
    let occurrences: Vec<_> = builder
        .occurrences
        .into_iter()
        .map(|occurrence| {
            let sha256 = sha256(occurrence.source.as_bytes());
            OccurrenceRecord {
                record: "occurrence",
                id: occurrence.id,
                query_id: query_ids
                    .get(&sha256)
                    .expect("every occurrence query was indexed")
                    .clone(),
                feature_path: occurrence.feature_path,
                feature: occurrence.feature,
                rule: occurrence.rule,
                scenario_template: occurrence.scenario_template,
                scenario: occurrence.scenario,
                scenario_line: occurrence.scenario_line,
                example_index: occurrence.example_index,
                example_line: occurrence.example_line,
                sequence: occurrence.sequence,
                role: occurrence.role,
                form: occurrence.form,
                source_path: occurrence.source_path,
                source_line: occurrence.source_line,
                supplemental: occurrence.tags.iter().any(|tag| tag == "ignore"),
                tags: occurrence.tags,
                expectation: occurrence.expectation,
                upstream_error: occurrence.upstream_error,
            }
        })
        .collect();
    let accept_occurrences = occurrences
        .iter()
        .filter(|occurrence| occurrence.expectation == Expectation::Accept)
        .count();
    let reject_occurrences = occurrences.len() - accept_occurrences;
    let supplemental_occurrences = occurrences
        .iter()
        .filter(|occurrence| occurrence.supplemental)
        .count();
    let active_occurrences = occurrences.len() - supplemental_occurrences;
    let header = HeaderRecord {
        record: "header",
        schema_version: SCHEMA_VERSION,
        spdx_license: "Apache-2.0",
        release: PINNED_RELEASE,
        commit: PINNED_COMMIT,
        corpus_sha256,
        feature_files: builder.feature_files,
        scenario_templates: builder.scenario_templates,
        scenario_outlines: builder.scenario_outlines,
        scenarios: builder.scenarios,
        unique_queries: queries.len(),
        occurrences: occurrences.len(),
        active_occurrences,
        supplemental_occurrences,
        accept_occurrences,
        reject_occurrences,
    };
    validate_final_counts(&header)?;
    Ok(Projection {
        header,
        queries,
        occurrences,
    })
}

fn sort_pending_occurrences(occurrences: &mut [PendingOccurrence]) {
    occurrences.sort_by(|left, right| {
        (
            &left.feature_path,
            left.scenario_line,
            left.example_index.unwrap_or(0),
            left.example_line.unwrap_or(0),
            left.sequence,
        )
            .cmp(&(
                &right.feature_path,
                right.scenario_line,
                right.example_index.unwrap_or(0),
                right.example_line.unwrap_or(0),
                right.sequence,
            ))
    });
}

fn query_records(occurrences: &[PendingOccurrence]) -> Result<Vec<QueryRecord>, String> {
    let mut unique = BTreeMap::<String, (String, Expectation)>::new();
    for occurrence in occurrences {
        let sha256 = sha256(occurrence.source.as_bytes());
        match unique.get(&sha256) {
            Some((source, _)) if source != &occurrence.source => {
                return Err(format!("SHA-256 collision for query {sha256}"));
            }
            Some((_, expectation)) if *expectation != occurrence.expectation => {
                return Err(format!(
                    "query {sha256} occurs with both accept and reject expectations"
                ));
            }
            _ => {
                unique.insert(sha256, (occurrence.source.clone(), occurrence.expectation));
            }
        }
    }

    Ok(unique
        .into_iter()
        .map(|(sha256, (source, expectation))| QueryRecord {
            record: "query",
            id: format!("query:{sha256}"),
            sha256,
            source,
            expectation,
        })
        .collect())
}

fn render_projection(projection: &Projection) -> Result<String, String> {
    let mut output = String::new();
    output.push_str(
        &serde_json::to_string(&projection.header)
            .map_err(|error| format!("projection header JSON: {error}"))?,
    );
    output.push('\n');
    for query in &projection.queries {
        output.push_str(
            &serde_json::to_string(query)
                .map_err(|error| format!("projection query JSON: {error}"))?,
        );
        output.push('\n');
    }
    for occurrence in &projection.occurrences {
        output.push_str(
            &serde_json::to_string(occurrence)
                .map_err(|error| format!("projection occurrence JSON: {error}"))?,
        );
        output.push('\n');
    }
    Ok(output)
}

fn verify_rendered(root: &Path, projection: &Projection) -> Result<(), String> {
    let expected = render_projection(projection)?;
    let path = root.join(GENERATED_FILE);
    let actual = fs::read_to_string(&path).map_err(|error| {
        format!("{GENERATED_FILE}: {error}; run `cargo run -p open-cypher-xtask -- tck generate`")
    })?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{GENERATED_FILE} is stale; run `cargo run -p open-cypher-xtask -- tck generate`"
        ))
    }
}

fn evaluate(root: &Path, projection: &Projection) -> Result<ConformanceReport, String> {
    let exclusions = read_exclusions(root)?;
    let queries_by_id: BTreeMap<_, _> = projection
        .queries
        .iter()
        .map(|query| (query.id.as_str(), query))
        .collect();
    let occurrences_by_id: BTreeMap<_, _> = projection
        .occurrences
        .iter()
        .map(|occurrence| (occurrence.id.as_str(), occurrence))
        .collect();
    for exclusion in exclusions.values() {
        if !occurrences_by_id.contains_key(exclusion.occurrence_id.as_str()) {
            return Err(format!(
                "{EXCLUSIONS_FILE}: exclusion `{}` does not identify a current occurrence",
                exclusion.occurrence_id
            ));
        }
    }

    let mut outcomes = BTreeMap::new();
    for query in &projection.queries {
        let outcome = match open_cypher::parse(&query.source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .diagnostics()
                .iter()
                .map(|diagnostic| ParserDiagnostic {
                    code: diagnostic.code.as_str().to_owned(),
                    message: diagnostic.message.clone(),
                    start: diagnostic.primary_span.start,
                    end: diagnostic.primary_span.end,
                })
                .collect(),
        };
        outcomes.insert(query.id.as_str(), outcome);
    }

    let mut failures = Vec::new();
    let mut stale_exclusions = Vec::new();
    let mut matched = 0;
    let mut active_matched = 0;
    let mut supplemental_matched = 0;
    for occurrence in &projection.occurrences {
        let diagnostics = outcomes
            .get(occurrence.query_id.as_str())
            .expect("every occurrence references a query");
        let accepted = diagnostics.is_empty();
        let matches = match occurrence.expectation {
            Expectation::Accept => accepted,
            Expectation::Reject => !accepted,
        };
        let exclusion = exclusions.get(&occurrence.id);
        if matches {
            matched += 1;
            if occurrence.supplemental {
                supplemental_matched += 1;
            } else {
                active_matched += 1;
            }
            if exclusion.is_some() {
                stale_exclusions.push(occurrence.id.clone());
            }
            continue;
        }

        let direction = mismatch_direction(occurrence.expectation).to_owned();
        if let Some(exclusion) = exclusion
            && exclusion.direction != direction
        {
            return Err(format!(
                "{EXCLUSIONS_FILE}: exclusion `{}` declares direction `{}` but current direction is `{direction}`",
                exclusion.occurrence_id, exclusion.direction
            ));
        }
        failures.push(ConformanceFailure {
            occurrence_id: occurrence.id.clone(),
            query_id: occurrence.query_id.clone(),
            feature_path: occurrence.feature_path.clone(),
            scenario_line: occurrence.scenario_line,
            role: occurrence.role,
            expectation: occurrence.expectation,
            supplemental: occurrence.supplemental,
            direction,
            excluded: exclusion.is_some(),
            exclusion_issue: exclusion.map(|entry| entry.issue.clone()),
            source: queries_by_id
                .get(occurrence.query_id.as_str())
                .expect("every occurrence references a query")
                .source
                .clone(),
            diagnostics: diagnostics.clone(),
        });
    }

    failures.sort_by(|left, right| left.occurrence_id.cmp(&right.occurrence_id));
    stale_exclusions.sort();
    let excluded = failures.iter().filter(|failure| failure.excluded).count();
    let unexpected = failures.len() - excluded;
    let supplemental_unexpected = failures
        .iter()
        .filter(|failure| !failure.excluded && failure.supplemental)
        .count();
    let active_unexpected = unexpected - supplemental_unexpected;
    let mut grouped = BTreeMap::<(String, String), usize>::new();
    for failure in &failures {
        if failure.excluded {
            continue;
        }
        let code = failure.diagnostics.first().map_or_else(
            || "accepted".to_owned(),
            |diagnostic| diagnostic.code.clone(),
        );
        *grouped
            .entry((failure.direction.clone(), code))
            .or_default() += 1;
    }
    let groups = grouped
        .into_iter()
        .map(|((direction, diagnostic_code), count)| FailureGroup {
            direction,
            diagnostic_code,
            count,
        })
        .collect();
    let expected_accept = projection.header.accept_occurrences;
    let expected_reject = projection.header.reject_occurrences;
    Ok(ConformanceReport {
        schema_version: SCHEMA_VERSION,
        spdx_license: "Apache-2.0",
        release: PINNED_RELEASE,
        commit: PINNED_COMMIT,
        corpus_sha256: projection.header.corpus_sha256.clone(),
        success: unexpected == 0 && stale_exclusions.is_empty(),
        counts: ReportCounts {
            occurrences: projection.occurrences.len(),
            active_occurrences: projection.header.active_occurrences,
            supplemental_occurrences: projection.header.supplemental_occurrences,
            expected_accept,
            expected_reject,
            matched,
            active_matched,
            supplemental_matched,
            excluded,
            unexpected,
            active_unexpected,
            supplemental_unexpected,
            stale_exclusions: stale_exclusions.len(),
        },
        groups,
        failures,
        stale_exclusions,
    })
}

fn mismatch_direction(expectation: Expectation) -> &'static str {
    match expectation {
        Expectation::Accept => "expected_accept_but_rejected",
        Expectation::Reject => "expected_reject_but_accepted",
    }
}

fn read_exclusions(root: &Path) -> Result<BTreeMap<String, Exclusion>, String> {
    let ledger: ExclusionsLedger = read_toml(root, EXCLUSIONS_FILE)?;
    validate_ledger_header(
        EXCLUSIONS_FILE,
        ledger.schema_version,
        &ledger.release,
        &ledger.commit,
    )?;
    if ledger.exclusion_count != ledger.exclusion.len() {
        return Err(format!(
            "{EXCLUSIONS_FILE}: exclusion_count is {}, but {} entries were found",
            ledger.exclusion_count,
            ledger.exclusion.len()
        ));
    }
    let mut exclusions = BTreeMap::new();
    for exclusion in ledger.exclusion {
        if !matches!(
            exclusion.direction.as_str(),
            "expected_accept_but_rejected" | "expected_reject_but_accepted"
        ) {
            return Err(format!(
                "{EXCLUSIONS_FILE}: exclusion `{}` has invalid direction `{}`",
                exclusion.occurrence_id, exclusion.direction
            ));
        }
        if exclusion.issue.trim().is_empty() || exclusion.rationale.trim().is_empty() {
            return Err(format!(
                "{EXCLUSIONS_FILE}: exclusion `{}` must have an issue and rationale",
                exclusion.occurrence_id
            ));
        }
        let id = exclusion.occurrence_id.clone();
        if exclusions.insert(id.clone(), exclusion).is_some() {
            return Err(format!(
                "{EXCLUSIONS_FILE}: duplicate occurrence exclusion `{id}`"
            ));
        }
    }
    Ok(exclusions)
}

fn print_report(report: &ConformanceReport) {
    println!(
        "TCK syntax projection: {} occurrences ({} active, {} @ignore supplemental; {} accept, {} reject)",
        report.counts.occurrences,
        report.counts.active_occurrences,
        report.counts.supplemental_occurrences,
        report.counts.expected_accept,
        report.counts.expected_reject
    );
    println!(
        "matched: {} ({} active, {} supplemental); excluded: {}; unexpected: {} ({} active, {} supplemental); stale exclusions: {}",
        report.counts.matched,
        report.counts.active_matched,
        report.counts.supplemental_matched,
        report.counts.excluded,
        report.counts.unexpected,
        report.counts.active_unexpected,
        report.counts.supplemental_unexpected,
        report.counts.stale_exclusions
    );
    if !report.groups.is_empty() {
        println!("unexpected failures by direction and first diagnostic:");
        for group in &report.groups {
            println!(
                "  {} / {}: {}",
                group.direction, group.diagnostic_code, group.count
            );
        }
    }
    let unexpected_failures: Vec<_> = report
        .failures
        .iter()
        .filter(|failure| !failure.excluded)
        .collect();
    for failure in unexpected_failures.iter().take(20) {
        let diagnostic = failure.diagnostics.first().map_or_else(
            || "parser accepted the query".to_owned(),
            |diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message),
        );
        println!(
            "FAIL {} {}:{} [{} / {}{}]\n  query: {}\n  diagnostic: {diagnostic}",
            failure.occurrence_id,
            failure.feature_path,
            failure.scenario_line,
            failure.role.as_str(),
            failure.expectation.as_str(),
            if failure.supplemental {
                " / supplemental"
            } else {
                ""
            },
            query_preview(&failure.source)
        );
    }
    if unexpected_failures.len() > 20 {
        println!(
            "... {} more unexpected failures; see {REPORT_FILE}",
            unexpected_failures.len() - 20
        );
    }
    for id in &report.stale_exclusions {
        println!("STALE EXCLUSION {id}");
    }
}

fn query_preview(source: &str) -> String {
    const LIMIT: usize = 180;
    let flattened = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ⏎ ");
    let mut preview: String = flattened.chars().take(LIMIT).collect();
    if flattened.chars().count() > LIMIT {
        preview.push('…');
    }
    preview
}

/// Strict release gate: the checked projection is current, every expectation
/// matches, and the reviewed exclusion ledger is empty.
pub fn release_check(root: &Path) -> Result<(), String> {
    let projection = build_projection(root)?;
    verify_rendered(root, &projection)?;
    let report = evaluate(root, &projection)?;
    print_report(&report);
    let exclusions = read_exclusions(root)?;
    if !exclusions.is_empty() {
        return Err(format!(
            "TCK syntax release gate requires zero exclusions; found {} in {EXCLUSIONS_FILE}",
            exclusions.len()
        ));
    }
    gate_report(&report)
}

fn validate_projection_counts(builder: &ProjectionBuilder) -> Result<(), String> {
    require_count(
        "feature files",
        EXPECTED_FEATURE_FILES,
        builder.feature_files,
    )?;
    require_count(
        "scenario templates",
        EXPECTED_SCENARIO_TEMPLATES,
        builder.scenario_templates,
    )?;
    require_count(
        "scenario outlines",
        EXPECTED_SCENARIO_OUTLINES,
        builder.scenario_outlines,
    )?;
    require_count("expanded scenarios", EXPECTED_SCENARIOS, builder.scenarios)
}

fn validate_final_counts(header: &HeaderRecord) -> Result<(), String> {
    require_count(
        "query occurrences",
        EXPECTED_OCCURRENCES,
        header.occurrences,
    )?;
    require_count(
        "active query occurrences",
        EXPECTED_ACTIVE_OCCURRENCES,
        header.active_occurrences,
    )?;
    require_count(
        "@ignore supplemental query occurrences",
        EXPECTED_SUPPLEMENTAL_OCCURRENCES,
        header.supplemental_occurrences,
    )?;
    require_count(
        "accepted occurrences",
        EXPECTED_ACCEPT_OCCURRENCES,
        header.accept_occurrences,
    )?;
    require_count(
        "rejected occurrences",
        EXPECTED_REJECT_OCCURRENCES,
        header.reject_occurrences,
    )
}

fn require_count(label: &str, expected: usize, actual: usize) -> Result<(), String> {
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "pinned TCK {label} changed: expected {expected}, found {actual}"
        ))
    }
}

fn read_toml<T: serde::de::DeserializeOwned>(root: &Path, relative: &str) -> Result<T, String> {
    let path = root.join(relative);
    let source =
        fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    toml::from_str(&source).map_err(|error| format!("{}: {error}", path.display()))
}

fn validate_ledger_header(
    file: &str,
    schema_version: u32,
    release: &str,
    commit: &str,
) -> Result<(), String> {
    if schema_version != SCHEMA_VERSION {
        return Err(format!(
            "{file}: expected schema_version {SCHEMA_VERSION}, found {schema_version}"
        ));
    }
    if release != PINNED_RELEASE {
        return Err(format!(
            "{file}: expected release {PINNED_RELEASE}, found {release}"
        ));
    }
    if commit != PINNED_COMMIT {
        return Err(format!(
            "{file}: expected commit {PINNED_COMMIT}, found {commit}"
        ));
    }
    Ok(())
}

fn files_with_extension(root: &Path, extension: &str) -> Result<Vec<PathBuf>, String> {
    if !root.is_dir() {
        return Err(format!("directory is missing: {}", root.display()));
    }
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file()
                && path.extension().and_then(|value| value.to_str()) == Some(extension)
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn corpus_sha256(root: &Path, feature_paths: &[PathBuf]) -> Result<String, String> {
    let graph_root = root.join(GRAPHS_ROOT);
    let mut inputs = feature_paths.to_vec();
    inputs.extend(files_with_extension(&graph_root, "json")?);
    inputs.extend(files_with_extension(&graph_root, "cypher")?);
    inputs.sort();
    let mut digest = Sha256::new();
    for path in inputs {
        let relative = spec_relative(root, &path)?;
        let contents = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update((contents.len() as u64).to_le_bytes());
        digest.update(&contents);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn sha256(contents: &[u8]) -> String {
    format!("{:x}", Sha256::digest(contents))
}

fn spec_relative(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(root.join("spec")).map_err(|_| {
        format!(
            "{} is not below the repository spec directory",
            path.display()
        )
    })?;
    let value = relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_string_lossy().into_owned(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("/");
    validate_spec_relative(&value)?;
    Ok(value)
}

fn validate_spec_relative(path: &str) -> Result<(), String> {
    let path_value = Path::new(path);
    if path.is_empty()
        || path_value.is_absolute()
        || path_value
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || !path.starts_with("vendor/openCypher-2024.3/")
    {
        Err(format!("invalid spec-relative pinned TCK path `{path}`"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(
        feature_path: &str,
        scenario_line: usize,
        example_line: Option<usize>,
        example_index: Option<usize>,
        sequence: usize,
        expectation: Expectation,
        source: &str,
    ) -> PendingOccurrence {
        PendingOccurrence {
            id: occurrence_id(
                feature_path,
                scenario_line,
                example_line,
                example_index,
                sequence,
            ),
            feature_path: feature_path.into(),
            feature: "feature".into(),
            rule: None,
            scenario_template: "template".into(),
            scenario: "scenario".into(),
            scenario_line,
            example_index,
            example_line,
            sequence,
            role: QueryRole::Primary,
            form: QueryForm::Docstring,
            source_path: feature_path.into(),
            source_line: scenario_line + 1,
            tags: Vec::new(),
            expectation,
            upstream_error: None,
            source: source.into(),
        }
    }

    #[test]
    fn substitutes_examples_once_without_reinterpreting_values() {
        let values = BTreeMap::from([
            ("name".to_owned(), "n".to_owned()),
            ("value".to_owned(), "<name>".to_owned()),
        ]);
        assert_eq!(
            substitute("RETURN <value>", &values).unwrap(),
            "RETURN <name>"
        );
    }

    #[test]
    fn substitutes_duplicate_placeholders_and_rejects_unresolved_ones() {
        let values = BTreeMap::from([("value".to_owned(), "42".to_owned())]);
        assert_eq!(
            substitute("RETURN <value> + <value>", &values).unwrap(),
            "RETURN 42 + 42"
        );
        assert!(substitute("RETURN <missing>", &values).is_err());
    }

    #[test]
    fn substitution_preserves_arrows_and_less_than_operators() {
        let source = "MATCH (a)<--(b)-->(c) WHERE a.value < b.value RETURN c";
        assert_eq!(substitute(source, &BTreeMap::new()).unwrap(), source);
    }

    #[test]
    fn rejects_duplicate_example_headers() {
        let headers = vec!["value".to_owned(), "value".to_owned()];
        assert_eq!(
            validate_example_headers(&headers).unwrap_err(),
            "duplicate Examples headers"
        );
    }

    #[test]
    fn normalizes_crlf_and_surrounding_whitespace() {
        assert_eq!(normalize_source(" \r\nRETURN 1\r\n "), "RETURN 1");
    }

    #[test]
    fn rejects_unsafe_source_and_graph_paths() {
        assert!(validate_spec_relative("../outside.feature").is_err());
        assert!(validate_spec_relative("/absolute.feature").is_err());
        assert!(validate_graph_script_name("../outside").is_err());
        assert!(validate_graph_script_name("/absolute").is_err());
    }

    #[test]
    fn rejects_conflicting_expectations_for_the_same_query() {
        let occurrences = vec![
            pending(
                "vendor/openCypher-2024.3/a.feature",
                1,
                None,
                None,
                1,
                Expectation::Accept,
                "RETURN 1",
            ),
            pending(
                "vendor/openCypher-2024.3/b.feature",
                2,
                None,
                None,
                1,
                Expectation::Reject,
                "RETURN 1",
            ),
        ];
        assert!(
            query_records(&occurrences)
                .unwrap_err()
                .contains("both accept and reject")
        );
    }

    #[test]
    fn occurrence_ids_and_order_include_example_provenance() {
        let path_a = "vendor/openCypher-2024.3/a.feature";
        let path_b = "vendor/openCypher-2024.3/b.feature";
        let mut occurrences = vec![
            pending(
                path_b,
                2,
                Some(30),
                Some(2),
                1,
                Expectation::Accept,
                "RETURN 3",
            ),
            pending(
                path_a,
                4,
                Some(20),
                Some(2),
                1,
                Expectation::Accept,
                "RETURN 2",
            ),
            pending(
                path_a,
                4,
                Some(19),
                Some(1),
                2,
                Expectation::Accept,
                "RETURN 1",
            ),
        ];
        sort_pending_occurrences(&mut occurrences);
        assert_eq!(
            occurrences
                .iter()
                .map(|occurrence| occurrence.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "occ:vendor/openCypher-2024.3/a.feature:4:19:1:2",
                "occ:vendor/openCypher-2024.3/a.feature:4:20:2:1",
                "occ:vendor/openCypher-2024.3/b.feature:2:30:2:1",
            ]
        );
    }

    #[test]
    fn compatibility_rewrite_is_limited_to_table_rows() {
        let source = "Feature: strings\n  Scenario: one\n    When executing query:\n      \"\"\"\n      RETURN '\\\''\n      \"\"\"\n    Then the result should be:\n      | '\\\'' |\n";
        let rewritten = gherkin_compatible_source(source);
        assert!(rewritten.contains("RETURN '\\\''"));
        assert!(rewritten.contains("| '\\\\\'' |"));
    }

    #[test]
    fn splitter_ignores_semicolons_inside_cypher_literals_and_comments() {
        let script = "CREATE ({value: ';'}); // ;\nCREATE (`a;b`); /* ; */ RETURN 1";
        assert_eq!(
            split_cypher_script(script).unwrap(),
            vec![
                (1, "CREATE ({value: ';'})".to_owned()),
                (1, "// ;\nCREATE (`a;b`)".to_owned()),
                (2, "/* ; */ RETURN 1".to_owned()),
            ]
        );
    }

    #[test]
    fn parses_tck_error_expectations() {
        assert_eq!(
            parse_error_step(
                "a SyntaxError should be raised at compile time: InvalidNumberLiteral"
            ),
            Some(UpstreamError {
                error_type: "SyntaxError".into(),
                phase: "compile time".into(),
                detail: "InvalidNumberLiteral".into(),
            })
        );
    }
}
