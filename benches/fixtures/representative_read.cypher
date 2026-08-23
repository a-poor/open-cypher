MATCH (person:Person)-[:KNOWS*1..3]->(friend:Person)
WHERE person.active = true AND friend.score >= 1.25e+3
WITH person, friend, friend.score AS score
ORDER BY score DESC
SKIP 10
LIMIT 25
RETURN person.name AS person, collect(friend.name) AS friends
