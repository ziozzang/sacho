# python3 is required on the executing node; Lua needs no external interpreter.
# Read allowed inputs with os.environ["NAME"].
import json
import platform

print(json.dumps({"healthy": 1, "display": platform.python_version()}))
