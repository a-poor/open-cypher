# open-cypher

_created by Austin Poor_

Parse [openCypher](https://opencypher.org/) queries using Rust.

`open-cypher` uses the [pest](https://pest.rs/) library to parse cypher queries using the pestfile, `src/cypher.pest`, based on the openCypher EBNF file ([link](https://opencypher.org/resources/) or [file](assets/cypher.ebnf)).
