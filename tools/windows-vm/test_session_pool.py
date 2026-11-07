import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from session_pool import Pool, fair_shares


class FairShareTests(unittest.TestCase):
    def test_a_small_run_keeps_all_it_needs_and_the_large_run_takes_the_rest(self):
        self.assertEqual(fair_shares({"round": 400, "check": 3}, 30), {"check": 3, "round": 27})

    def test_equal_large_runs_split_capacity_without_exceeding_it(self):
        shares = fair_shares({"a": 100, "b": 100, "c": 100}, 30)
        self.assertEqual(sorted(shares.values()), [10, 10, 10])
        shares = fair_shares({"a": 100, "b": 100}, 15)
        self.assertEqual(sum(shares.values()), 15)
        self.assertLessEqual(max(shares.values()) - min(shares.values()), 1)

    def test_unused_demand_never_receives_capacity(self):
        self.assertEqual(fair_shares({"a": 0, "b": 2}, 30), {"a": 0, "b": 2})
        self.assertEqual(fair_shares({"a": 5}, 0), {"a": 0})
        self.assertEqual(fair_shares({}, 30), {})

    def test_runs_beyond_capacity_starve_the_largest_demand_first(self):
        self.assertEqual(fair_shares({"round": 200, "check": 1}, 1), {"check": 1, "round": 0})
        shares = fair_shares({f"run-{index}": 100 for index in range(31)}, 30)
        self.assertEqual(sum(shares.values()), 30)
        self.assertEqual(sorted(shares.values()), [0] + [1] * 30)

    def test_every_run_computes_the_same_division(self):
        demands = {"a": 9, "b": 9, "c": 1}
        self.assertEqual(fair_shares(demands, 10), fair_shares(dict(reversed(list(demands.items()))), 10))


class PoolTests(unittest.TestCase):
    def setUp(self):
        directory = TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.pool = Pool(Path(directory.name) / "sessions")

    def test_a_session_has_one_holder_until_its_lease_closes(self):
        first = self.pool.claim("vm", "capy01", "run-a", "/worktree-a")
        self.assertIsNone(self.pool.claim("vm", "capy01", "run-b", "/worktree-b"))
        self.assertEqual(self.pool.holders("vm", ["capy01", "capy02"]),
                         {"capy01": {"run": "run-a", "owner": "/worktree-a"}})
        first.close()
        self.assertEqual(self.pool.holders("vm", ["capy01", "capy02"]), {})
        self.pool.claim("vm", "capy01", "run-b", "/worktree-b").close()

    def test_shutdown_gate_refuses_while_any_session_is_leased_and_blocks_new_leases(self):
        lease = self.pool.claim("vm", "capy01", "run", "/worktree")
        with self.pool.closed("vm", ["capy01", "capy02"]) as closed:
            self.assertFalse(closed)
        lease.close()
        with self.pool.closed("vm", ["capy01", "capy02"]) as closed:
            self.assertTrue(closed)
            self.assertIsNone(self.pool.claim("vm", "capy02", "run", "/worktree"))
        self.pool.claim("vm", "capy02", "run", "/worktree").close()

    def test_runs_publish_demand_and_leave_when_they_end_or_die(self):
        with self.pool.register("run-a", "/worktree") as update:
            self.assertEqual(self.pool.runs()["run-a"]["demand"], 1)
            update(12)
            self.assertEqual(self.pool.runs()["run-a"]["demand"], 12)
            self.assertEqual(self.pool.runs()["run-a"]["owner"], "/worktree")
        self.assertEqual(self.pool.runs(), {})
        dead = self.pool.directory / "runs" / "dead.json"
        dead.write_text(json.dumps({"demand": 9}))
        self.assertEqual(self.pool.runs(), {})


if __name__ == "__main__":
    unittest.main()
