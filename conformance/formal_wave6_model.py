#!/usr/bin/env python3
from itertools import product

# Bounded temporal trace check: requested -> prepared -> cloned -> verified.
STAGES = (0, 1, 2, 3)

def valid_trace(trace):
    return trace[0] == 0 and all(b == a or b == a + 1 for a, b in zip(trace, trace[1:]))

seen_terminal = False
for trace in product(STAGES, repeat=4):
    if not valid_trace(trace):
        continue
    assert all(b >= a for a, b in zip(trace, trace[1:])), "clone lifecycle regressed"
    if trace[-1] == 3:
        seen_terminal = True
        assert 1 in trace and 2 in trace, "verification skipped prepare/clone stages"
assert seen_terminal, "verified clone state is unreachable"
print("temporal clone lifecycle model: ok")
