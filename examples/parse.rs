use open_cypher::parse;

fn main() {
    let source = "MATCH (person:Person) RETURN person.name AS name";

    match parse(source) {
        Ok(parsed) => {
            println!("parsed {} tokens", parsed.tokens.len());
            println!("{:#?}", parsed.program);
        }
        Err(errors) => {
            eprintln!("query is not valid openCypher:\n{errors:#?}");
            std::process::exit(1);
        }
    }
}
