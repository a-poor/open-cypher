MATCH (person:Person {name: $name})
WHERE person.active = true
RETURN person.name AS name
