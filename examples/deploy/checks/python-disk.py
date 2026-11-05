# Runs with the service account's permissions; needs python3 on the executing node.
# Prints one JSON object. Exit status 0 means the check ran.
import json
import shutil

usage = shutil.disk_usage("/")
free = round(usage.free / usage.total * 100, 1)
print(json.dumps({"free_percent": free, "display": f"{free}% free"}))
