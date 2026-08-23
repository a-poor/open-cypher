UNWIND [1, 2, 3] AS value
WITH value, value * 2 + 1 AS projected
WHERE projected >= 3
RETURN value, projected
ORDER BY projected DESC
