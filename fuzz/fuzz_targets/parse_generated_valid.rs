#![no_main]

use libfuzzer_sys::fuzz_target;
use open_cypher::parse;

fn identifier(byte: u8) -> String {
    format!("v_{}", byte as usize)
}

fn generated_query(data: &[u8]) -> String {
    let first = data.first().copied().unwrap_or_default();
    let second = data.get(1).copied().unwrap_or_default();
    let variable = identifier(first);
    let value = i16::from(second) - 128;
    let list = data
        .iter()
        .skip(2)
        .take(16)
        .map(|byte| i16::from(*byte).saturating_sub(128).to_string())
        .collect::<Vec<_>>();
    let list = if list.is_empty() {
        "0".to_owned()
    } else {
        list.join(",")
    };

    match first % 10 {
        0 => format!("RETURN {value} AS {variable}"),
        1 => format!("MATCH ({variable}) RETURN {variable}"),
        2 => format!(
            "MATCH ({variable}:Generated {{value: {value}}}) WHERE {variable}.value >= {value} RETURN {variable}"
        ),
        3 => format!("UNWIND [{list}] AS {variable} RETURN {variable}"),
        4 => format!("CREATE ({variable}:Generated {{value: {value}}}) RETURN {variable}"),
        5 => format!("MATCH path = ANY SHORTEST ({variable})-[:LINK]->{{1,3}}(other) RETURN path"),
        6 => format!("MATCH ({variable}:Generated & !Excluded) RETURN {variable}"),
        7 => format!("RETURN [{list}] AS {variable}"),
        8 => format!("RETURN ({value} + 2) * 3 >= 0 AS {variable}"),
        _ => format!("MATCH path = ({variable})-[:LINK*1..3]->(other) RETURN path, other"),
    }
}

fuzz_target!(|data: &[u8]| {
    let source = generated_query(data);
    if let Err(errors) = parse(&source) {
        panic!("grammar-aware generator produced a rejected query: {source}\n{errors:#?}");
    }
});
