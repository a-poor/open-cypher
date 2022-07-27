extern crate pest;
extern crate pest_derive;

use pest::Parser;
use pest::error::Error;
use pest::iterators::{Pairs, Pair};
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "cypher.pest"]
pub struct CypherParser;

pub fn parse(code: &str) -> Result<Pairs<Rule>, Error<Rule>> {
    CypherParser::parse(Rule::Cypher, code)
}

pub fn parse_string_literal(code: &str) -> Result<Pairs<Rule>, Error<Rule>> {
    CypherParser::parse(Rule::UnescapedSymbolicName, code)
}





pub fn print_pairs(pairs: Pairs<Rule>) {
    let p = pairs
        .into_iter()
        .next()
        .expect("An error!");
    _print_pairs(p, 0);
}

fn _print_pairs(pair: Pair<Rule>, depth: usize) {
    let pad = " ".repeat(depth);
    println!("{}- {:?} {:?}", pad, pair.as_rule(), pair.as_str());

    pair
        .into_inner()
        .map(|p: Pair<Rule>| _print_pairs(p, depth + 1))
        .count();
}