import json
import sys


def main() -> None:
    raw = sys.stdin.read().strip()
    value = json.loads(raw) if raw else {}
    print(json.dumps(value, separators=(",", ":")))


if __name__ == "__main__":
    main()
