use open_cypher::parse_recovering;

fn main() {
    let outcome = parse_recovering("MATCH (person RETURN person");

    for diagnostic in &outcome.diagnostics {
        eprintln!("{:?}: {}", diagnostic.code, diagnostic.message);
    }

    match outcome.value {
        Some(parsed) => println!("partial AST:\n{:#?}", parsed.program),
        None => println!("the parser could not construct a partial AST"),
    }
}
