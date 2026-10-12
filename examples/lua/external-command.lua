-- Provider-specific logic belongs in your own external script.
-- Supply the interpreter and script as separate arguments (no shell expansion).
local result = sacho.exec({"python3", "/opt/checks/health.py"})
assert(result.exit_code == 0, "External scenario failed")
local data = sacho.json_decode(result.stdout)
return sacho.json_encode(data)
