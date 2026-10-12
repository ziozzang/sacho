-- Opt-in local subscription OAuth adapter, adapted from Dorang.
-- The endpoint is undocumented and can change or rate-limit requests.
local quota = sacho.usage("claude")
return sacho.json_encode(quota)
-- display_pointer = /display. Poll at 300 seconds or slower.
