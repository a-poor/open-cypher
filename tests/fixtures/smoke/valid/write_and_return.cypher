MERGE (person:Person {name: $name})
ON CREATE SET person.created = true
ON MATCH SET person.seen = true
RETURN person
