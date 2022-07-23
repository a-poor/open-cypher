extern crate pest;
extern crate pest_derive;

use pest::Parser;
use pest::error::Error;
use pest::iterators::Pairs;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "cypher.pest"]
pub struct CypherParser;

pub fn parse(code: &str) -> Result<Pairs<Rule>, Error<Rule>> {
    CypherParser::parse(Rule::Cypher, code)
}
