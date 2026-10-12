-- Latest observed TPS across reported credentials; zero is valid idle traffic.
local r = sacho.http({
  url = "http://127.0.0.1:4000/admin/credentials/health",
  headers = {Authorization = sacho.env("SACHO_DORANG_BEARER")},
})
assert(r.status == 200, "Dorang credential health query failed")
local data = sacho.json_decode(r.body)
local tps = 0
for _, credential in ipairs(data.credentials) do
  tps = tps + (credential.tokens_per_sec or 0)
end
return sacho.json_encode({tps = math.floor(tps * 10 + 0.5) / 10})
-- display_pointer = /tps; metric_unit = " TPS"; no numeric threshold required.
