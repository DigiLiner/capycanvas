"""Leases on shared GPU VM sessions, divided fairly between concurrent fixture runs."""

from contextlib import contextmanager
from datetime import datetime, timezone
import json
from pathlib import Path

from cpu_pool import lock_file


def fair_shares(demands, capacity):
    shares = {}
    order = sorted(demands, key=lambda run: (demands[run], run))
    for index, run in enumerate(order):
        shares[run] = min(demands[run], -(-capacity // (len(order) - index)))
        capacity -= shares[run]
    return shares


class Pool:
    def __init__(self, directory):
        self.directory = Path(directory)

    def slot(self, vm, account):
        return self.directory / f"{vm}-{account}"

    def claim(self, vm, account, run, owner):
        self.directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        lock = lock_file(self.slot(vm, account))
        if lock is not None:
            lock.seek(0)
            lock.truncate()
            json.dump({"run": run, "owner": str(owner)}, lock)
            lock.flush()
        return lock

    def holders(self, vm, accounts):
        self.directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        rows = {}
        for account in accounts:
            lock = lock_file(self.slot(vm, account))
            if lock is not None:
                lock.close()
                continue
            try:
                rows[account] = json.loads(self.slot(vm, account).read_text())
            except (OSError, ValueError):
                rows[account] = {}
        return rows

    @contextmanager
    def closed(self, vm, accounts):
        self.directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        locks = [lock_file(self.slot(vm, account)) for account in accounts]
        try:
            yield all(lock is not None for lock in locks)
        finally:
            for lock in locks:
                if lock is not None:
                    lock.close()

    @contextmanager
    def register(self, run, owner):
        runs = self.directory / "runs"
        runs.mkdir(mode=0o700, parents=True, exist_ok=True)
        path = runs / f"{run}.json"
        lock = lock_file(path, blocking=True)
        started = datetime.now(timezone.utc).isoformat()

        def update(demand):
            lock.seek(0)
            lock.truncate()
            json.dump({"owner": str(owner), "started": started, "demand": demand}, lock)
            lock.flush()

        update(1)
        try:
            yield update
        finally:
            path.unlink(missing_ok=True)
            lock.close()

    def runs(self):
        rows = {}
        for path in sorted((self.directory / "runs").glob("*.json")):
            lock = lock_file(path)
            if lock is not None:
                lock.close()
                continue
            try:
                rows[path.stem] = json.loads(path.read_text())
            except (OSError, ValueError):
                rows[path.stem] = {"demand": 1}
        return rows
