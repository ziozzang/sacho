-- Probe the actual gateway route, including Host-based routing to Dorang.
local response = sacho.http({
  url = "https://gateway.example.com/health/liveliness",
})
assert(response.status == 200, "Hangang upstream routing failed")
assert(sacho.json_decode(response.body).status == "alive", "Dorang through Hangang is not live")
return sacho.json_encode({value = "gateway → dorang", latency_ms = response.latency_ms})
