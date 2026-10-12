-- A provider quota read from Dorang, using an allowlisted environment variable.
-- probe_env = ["SACHO_DORANG_BEARER"] in sacho.toml.
-- SACHO_DORANG_BEARER holds the full "Bearer ..." header outside the database.
local response = sacho.http({
  url = "http://127.0.0.1:4000/admin/quota",
  headers = {Authorization = sacho.env("SACHO_DORANG_BEARER")},
})
assert(response.status == 200, "Quota endpoint failed")
local data = sacho.json_decode(response.body)
local remaining = nil
for _, window in ipairs(data.windows) do
  if window.provider_id == "anthropic" and window.source ~= "local" and not window.stale then
    remaining = math.min(remaining or 100, 100 - window.used_pct)
  end
end
assert(remaining ~= nil, "No fresh provider quota window reported")
return sacho.json_encode({remaining_percent = remaining})
