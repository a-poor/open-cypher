use open_cypher::lex;

fn main() {
    let outcome = lex("MATCH (n) WHERE n.score >= 10 RETURN n");

    for token in outcome.tokens {
        println!("{:?} at {:?}", token.kind, token.span);
    }
    for diagnostic in outcome.diagnostics {
        eprintln!("{:?}: {}", diagnostic.code, diagnostic.message);
    }
}
