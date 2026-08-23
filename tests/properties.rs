use open_cypher::{DiagnosticCode, lex, parse, parse_recovering};
use proptest::prelude::*;

fn identifier_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-zA-Z0-9_]{0,15}".prop_map(|suffix| format!("v_{suffix}"))
}

prop_compose! {
    fn simple_query_strategy()
        (name in identifier_strategy(), value in -1_000_000i64..1_000_000)
        -> String
    {
        format!(
            "MATCH ({name}:Generated {{value: {value}}}) WHERE {name}.value >= {value} RETURN {name}"
        )
    }
}

proptest! {
    #[test]
    fn generated_valid_queries_parse(source in simple_query_strategy()) {
        prop_assert!(parse(&source).is_ok(), "generated query failed: {source}");
    }

    #[test]
    fn arbitrary_input_never_panics(source in any::<String>()) {
        let _ = lex(&source);
        let _ = parse(&source);
        let recovered = parse_recovering(&source);
        prop_assert!(recovered.diagnostics.len() <= 32);
        prop_assert!(
            recovered
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != DiagnosticCode::Internal),
            "user input surfaced an internal parser diagnostic: {source:?}"
        );
    }

    #[test]
    fn legal_whitespace_does_not_change_acceptance(
        left_space in "[ \\t\\r\\n]{1,8}",
        right_space in "[ \\t\\r\\n]{1,8}",
    ) {
        let compact = "MATCH (n) RETURN n";
        let expanded = format!("MATCH{left_space}(n){right_space}RETURN{left_space}n");
        prop_assert!(parse(compact).is_ok());
        prop_assert!(parse(&expanded).is_ok(), "expanded query failed: {expanded:?}");
    }

    #[test]
    fn lex_and_recovery_are_deterministic(source in any::<String>()) {
        let first_lex = lex(&source);
        let second_lex = lex(&source);
        prop_assert_eq!(first_lex.tokens, second_lex.tokens);
        prop_assert_eq!(first_lex.diagnostics, second_lex.diagnostics);

        let first_parse = parse_recovering(&source);
        let second_parse = parse_recovering(&source);
        prop_assert_eq!(first_parse.diagnostics, second_parse.diagnostics);
    }
}
