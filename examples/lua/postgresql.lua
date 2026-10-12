-- Requires external psql. Inject PGHOST, PGPORT, PGDATABASE, PGUSER,
-- PGPASSFILE (or PGPASSWORD), PGCONNECT_TIMEOUT and PGSQL_QUERY via probe_env.
-- PGSQL_QUERY must be a SELECT returning one JSON object, e.g.:
-- SELECT json_build_object('value',1,'version',version())
-- Use a read-only database role. Do not place credentials in SQL or argv.
assert(tonumber(sacho.env("PGCONNECT_TIMEOUT")) and tonumber(sacho.env("PGCONNECT_TIMEOUT")) >= 2, "Set a bounded PGCONNECT_TIMEOUT")
local query = sacho.env("PGSQL_QUERY")
local result = sacho.exec({
  "psql", "-X", "-w", "-q", "-t", "-A", "-v", "ON_ERROR_STOP=1", "-c",
  "BEGIN READ ONLY; SET LOCAL statement_timeout='5s'; " .. query .. "; COMMIT;",
})
assert(result.exit_code == 0, "PostgreSQL query failed; inspect connectivity and read-only query privately")
local value = sacho.json_decode(result.stdout)
assert(type(value) == "table" and (type(value.value) == "number" or type(value.value) == "string" or type(value.value) == "boolean"), "SQL must return a single JSON object with a scalar value field")
return sacho.json_encode(value)
