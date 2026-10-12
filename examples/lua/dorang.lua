-- Multi-step functional health scenario. No inference request or token spend.
local live = sacho.http("http://127.0.0.1:4000/health/liveliness")
assert(live.status == 200, "Dorang liveness failed")
assert(sacho.json_decode(live.body).status == "alive", "Dorang is not live")
local ready = sacho.http("http://127.0.0.1:4000/health/readiness")
assert(ready.status == 200, "Dorang readiness failed")
assert(sacho.json_decode(ready.body).status == "ready", "Dorang is not ready")
return sacho.json_encode({value = "live + ready", latency_ms = ready.latency_ms})
