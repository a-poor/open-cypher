use open_cypher::parse;

fn assert_parses(source: &str) {
    assert!(
        parse(source).is_ok(),
        "historical regression failed to parse: {source}"
    );
}

#[test]
fn addition_is_not_shadowed_by_unary_plus() {
    assert_parses("RETURN 1 + 2");
}

#[test]
fn multi_character_comparisons_are_not_split() {
    assert_parses("RETURN 1 <= 2, 2 >= 1, 1 <> 2");
}

#[test]
fn decimal_exponents_accept_an_explicit_sign() {
    assert_parses("RETURN 1.25e+3, 1.25E-3");
}

#[test]
fn pattern_comprehensions_use_the_pattern_grammar() {
    assert_parses("MATCH (n) RETURN [(n)-->(m) | m]");
}

#[test]
fn keyword_prefixes_do_not_split_symbolic_names() {
    assert_parses("MATCH (notable) RETURN notable");
    assert_parses("MATCH (containsValue) RETURN containsValue");
    assert_parses("MATCH (returning) RETURN returning");
}

#[test]
fn contextual_keywords_can_be_symbolic_names() {
    assert_parses("MATCH (match) RETURN match");
    assert_parses("MATCH (return) RETURN return");
}
