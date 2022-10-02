# open-cypher

_created by Austin Poor_

Parse [openCypher](https://opencypher.org/) queries using Rust.

`open-cypher` uses the [pest](https://pest.rs/) library to parse cypher queries using the pestfile, `src/cypher.pest`, based on the openCypher EBNF file ([link](https://opencypher.org/resources/) or [file](assets/cypher.ebnf)).


## Project Status

This library is still in a very early stage. The repo includes a pest grammar file for defining the cyper language, based on the ebnf file from the openCypher site.

My goal is to finish the pest definition and possibly generate a cleaner AST based on what pest parses.

For reference, the following other projects use pest: https://github.com/pest-parser/pest#projects-using-pest


## Project Structure

- `src/`
  - `cypher.pest`: The [pest](https://pest.rs/) grammar file for generating a rust parser and types, based on the [ebnf file](./assets/cypher.ebnf) from the [openCypher site](https://opencypher.org/).
  - `main.rs`: Currently the main directory for quick tests (will be removed)
  - `parser.rs`: Contains functions for parsing and viewing parsed cypher queries
  - `ast.rs`: Will contain code for _potentially_ exposing a cleaner cypher AST, than is created by pest

## Contributing

Contributions of any size are more than welcome! Please feel free to submit issues or PRs.

I'm also open to any suggestions regarding overall project direction.


## To Do

- [ ] Add tests
- [ ] Add examples
- [ ] Add documentation
