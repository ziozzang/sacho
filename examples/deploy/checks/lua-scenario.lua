-- Two requests with content checks; Lua is built in, nothing to install.
-- Change the URLs. Secrets come from the sacho.env function, and each name must be
-- listed in probe_env. Never put a credential in a script.
local first = sacho.http("http://127.0.0.1:8080/healthz")
assert(first.status == 200, "health endpoint failed")
local health = sacho.json_decode(first.body)
assert(health.status == "ok", "unexpected health state")

local second = sacho.http("http://127.0.0.1:8080/api/status")
assert(second.status == 200, "status API failed")

return sacho.json_encode({healthy = 1, display = "v" .. health.version})
