import json

with open("trace.json") as f:
    trace = json.load(f)

for key, value in trace.items():
    size = len(json.dumps(value).encode())
    print(f"{key:30} {size / 1024:10.1f} KB")
