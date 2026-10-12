-- Inputs on the executing node: DOCKER_SOCKET, DOCKER_CONTAINER in probe_env.
-- Monitor condition: /healthy gte 1; display pointer: /display;
-- optional public message pointer: /message. No Docker CLI required.
local function encode(text)
  return (text:gsub("([^%w%-_%.~])", function(c)
    return string.format("%%%02X", string.byte(c))
  end))
end
local socket = sacho.env("DOCKER_SOCKET")
local name = sacho.env("DOCKER_CONTAINER")
assert(#name > 0 and #name <= 256, "Container name required")
local r = sacho.http({unix_socket=socket,
  url="http://localhost/containers/" .. encode(name) .. "/json"})
if r.status == 404 then
  return sacho.json_encode({healthy=0, display="not found", message="Configured container was not found"})
end
assert(r.status == 200, "Docker inspect request failed")
local d = sacho.json_decode(r.body)
local state = d.State
assert(type(state) == "table", "Container state missing")
local health = state.Health and state.Health.Status
local healthy = state.Running == true and (not health or health == "healthy")
return sacho.json_encode({healthy=healthy and 1 or 0,
  display=(state.Status or "unknown") .. (health and " / " .. health or ""),
  restart_count=d.RestartCount, message=healthy and "Container is running" or "Container is not ready"})
