-- Inputs: PROMETHEUS_URL and PROMETHEUS_QUERY in probe_env.
-- To add auth, inject PROMETHEUS_BEARER and explicitly read it into headers.
local function encode(text)
  return (text:gsub("([^%w%-_%.~])", function(c)
    return string.format("%%%02X", string.byte(c))
  end))
end
local base = sacho.env("PROMETHEUS_URL"):gsub("/+$", "")
local query = sacho.env("PROMETHEUS_QUERY")
local headers = {}
local authorized, bearer = pcall(sacho.env, "PROMETHEUS_BEARER")
if authorized then headers.Authorization = bearer end
local response = sacho.http({url = base .. "/api/v1/query?query=" .. encode(query) .. "&timeout=5s", headers = headers})
assert(response.status == 200, "Prometheus query request failed")
local body = sacho.json_decode(response.body)
assert(body.status == "success" and body.data, "Prometheus query did not succeed")
local data = body.data
local sample
if data.resultType == "scalar" then
  sample = data.result
elseif data.resultType == "vector" then
  assert(#data.result == 1, "Query must return exactly one series; select or aggregate explicitly")
  sample = data.result[1].value
else
  error("Expected scalar or one float vector sample")
end
assert(type(sample) == "table", "No numeric sample (histograms are not supported by this example)")
local value = tonumber(sample[2])
assert(value and value == value and value ~= math.huge and value ~= -math.huge, "Sample is not finite")
return sacho.json_encode({value = value, evaluated_at = sample[1]})
