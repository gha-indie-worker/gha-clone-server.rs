#!/usr/bin/env python3
from itertools import product

REQUESTED, PREPARED, CLONED, VERIFIED = range(4)
STAGES = (REQUESTED, PREPARED, CLONED, VERIFIED)


def step(state, event):
    if event == "prepare" and state == REQUESTED:
        return PREPARED
    if event == "clone" and state == PREPARED:
        return CLONED
    if event == "verify" and state == CLONED:
        return VERIFIED
    if event == "noop":
        return state
    return None


EVENTS = ("prepare", "clone", "verify", "noop")
reached_verified = False
rejected_illegal = False
for events in product(EVENTS, repeat=4):
    state = REQUESTED
    history = [state]
    legal = True
    for event in events:
        nxt = step(state, event)
        if nxt is None:
            legal = False
            rejected_illegal = True
            break
        state = nxt
        history.append(state)
    if not legal:
        continue
    assert all(b >= a for a, b in zip(history, history[1:])), "clone lifecycle regressed"
    if state == VERIFIED:
        reached_verified = True
        assert history[:4] == [REQUESTED, PREPARED, CLONED, VERIFIED], "verification skipped required stages"
        assert all(s == VERIFIED for s in history[3:]), "verified clone state was not terminal"

assert reached_verified, "verified clone state is unreachable"
assert rejected_illegal, "model never exercised an illegal transition"
print("temporal clone lifecycle transition-system model: ok")
