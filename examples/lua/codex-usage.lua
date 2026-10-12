-- Official account/rateLimits/read, never an inference request.
local quota = sacho.usage("codex")
return sacho.json_encode(quota)
-- display_pointer = /display; optional numeric condition = /remaining_percent.
-- Leave threshold empty for display-only usage.
