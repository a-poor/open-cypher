MATCH path = (start:Person)-[:KNOWS*1..3]->(friend:Person)
RETURN path, friend
