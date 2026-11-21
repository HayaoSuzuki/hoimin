"""Controlled evaluation specimens; not production library implementations."""


def nonnegative(values: list[int]) -> set[int]:
    """Return distinct nonnegative integers, including zero."""
    return {value for value in values if value >= 0}


class Cache:
    """Reads observe the most recent write, including after a cached read."""

    def __init__(self):
        self.storage = {}
        self.cached = {}

    def put(self, key, value):
        self.storage[key] = value
        self.cached.pop(key, None)

    def get(self, key):
        if key not in self.cached:
            self.cached[key] = self.storage[key]
        return self.cached[key]

    def delete(self, key):
        self.storage.pop(key, None)
        self.cached.pop(key, None)


def publish(backend, value):
    """Try at most three writes, roll back exhaustion and always close."""
    previous = backend.value
    try:
        for attempt in range(3):
            try:
                backend.write(value)
                return
            except OSError:
                if attempt == 2:
                    raise
    except OSError:
        backend.value = previous
        raise
    finally:
        backend.close()
