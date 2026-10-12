-- Requires ping and ICMP permission on the selected execution node.
-- Supply PING_HOST in the node environment or encrypted vault.
local result = sacho.exec({"ping", "-n", "-c", "1", "-W", "2", sacho.env("PING_HOST")})
assert(result.exit_code == 0, "ICMP reply was not received")
return sacho.json_encode({healthy = 1, display = "ICMP reply received"})
