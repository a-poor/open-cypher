MATCH path = (start:Person)-[:KNOWS|WORKS_WITH*1..8]->(candidate:Person)
WHERE start.name = $name
  AND all(node IN nodes(path) WHERE node.active = true)
RETURN path, candidate, length(path) AS distance
ORDER BY distance, candidate.name
LIMIT 100
