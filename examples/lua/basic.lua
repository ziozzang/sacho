-- Lua is embedded in Sacho. Read allowed inputs with sacho.env("NAME").
-- Return JSON to expose one selected value without affecting health.
return sacho.json_encode({healthy = 1, display = _VERSION})
