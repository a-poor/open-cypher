MATCH path = ANY SHORTEST (start:Person)-[:KNOWS]->{1,8}(candidate:Person & !Suspended)
WHERE start.name = $name
RETURN path, candidate
