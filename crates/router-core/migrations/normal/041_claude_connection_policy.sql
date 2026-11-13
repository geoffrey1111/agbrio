CREATE TABLE claude_connection_policy (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 duration_days INTEGER NOT NULL CHECK(duration_days IN (30,90,180,365,730))
);
