use open_cypher::parse;

fn main() {
    let code = "Hello, World;";
    match parse(code) {
        Ok(tree) => println!("TREE={:?}", tree),
        Err(err) => eprintln!("ERROR={}", err),
    }
}
