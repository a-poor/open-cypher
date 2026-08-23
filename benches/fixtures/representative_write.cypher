MERGE (person:Person {name: $name})
ON CREATE SET person.created = true, person.visits = 1
ON MATCH SET person.visits = person.visits + 1
WITH person
MERGE (person)-[:VISITED]->(place:Place {name: $place})
RETURN person
