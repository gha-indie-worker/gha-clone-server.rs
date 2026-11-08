//! Exhaustive finite temporal model for clone lifecycle admission.
//! Source-only model: does not claim production clone IO execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Requested,
    Prepared,
    Cloned,
    Verified,
}

#[derive(Clone, Copy, Debug)]
enum Event {
    Prepare,
    Clone,
    Verify,
    Noop,
}

fn step(state: State, event: Event) -> Option<State> {
    match (state, event) {
        (State::Requested, Event::Prepare) => Some(State::Prepared),
        (State::Prepared, Event::Clone) => Some(State::Cloned),
        (State::Cloned, Event::Verify) => Some(State::Verified),
        (state, Event::Noop) => Some(state),
        _ => None,
    }
}

fn verify_with(mut next: impl FnMut(State, Event) -> Option<State>) -> Result<(usize, usize), &'static str> {
    let events = [Event::Prepare, Event::Clone, Event::Verify, Event::Noop];
    let mut reached_verified = false;
    let mut rejected_illegal = false;
    let mut legal_traces = 0;
    for mut encoded in 0..256_u16 {
        let mut state = State::Requested;
        let mut history = vec![state];
        let mut legal = true;
        for _ in 0..4 {
            let event = events[usize::from(encoded % 4)];
            encoded /= 4;
            match next(state, event) {
                Some(after) => {
                    if after < state {
                        return Err("clone lifecycle regressed");
                    }
                    state = after;
                    history.push(state);
                }
                None => {
                    rejected_illegal = true;
                    legal = false;
                    break;
                }
            }
        }
        if !legal {
            continue;
        }
        legal_traces += 1;
        if state == State::Verified {
            reached_verified = true;
            let compressed: Vec<_> = history
                .iter()
                .copied()
                .fold(Vec::<State>::new(), |mut seen, s| {
                    if seen.last() != Some(&s) {
                        seen.push(s);
                    }
                    seen
                });
            if compressed
                != [
                    State::Requested,
                    State::Prepared,
                    State::Cloned,
                    State::Verified,
                ]
            {
                return Err("verification skipped or reordered stages");
            }
            let verified_at = history
                .iter()
                .position(|s| *s == State::Verified)
                .ok_or("verified state missing")?;
            if history[verified_at..]
                .iter()
                .any(|s| *s != State::Verified)
            {
                return Err("verified state not terminal");
            }
        }
    }
    if !reached_verified {
        return Err("verified clone state unreachable");
    }
    if !rejected_illegal {
        return Err("illegal transitions never rejected");
    }
    Ok((legal_traces, 256))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_256_four_event_traces_are_explored() {
        let (legal, total) = verify_with(step).expect("canonical model");
        assert_eq!(total, 256);
        assert!(legal > 0 && legal < total);
    }

    #[test]
    fn skip_prepared_mutation_must_be_detected() {
        let mutated = |state, event| match (state, event) {
            (State::Requested, Event::Prepare) => Some(State::Cloned),
            other => step(other.0, other.1),
        };
        assert!(verify_with(mutated).is_err());
    }

    #[test]
    fn regression_from_verified_must_be_detected() {
        let mutated = |state, event| match (state, event) {
            (State::Verified, Event::Noop) => Some(State::Requested),
            other => step(other.0, other.1),
        };
        assert!(verify_with(mutated).is_err());
    }

    #[test]
    fn no_illegal_transition_rejection_is_not_a_proof() {
        let mutated = |state, event| step(state, event).or(Some(state));
        assert!(verify_with(mutated).is_err());
    }

    #[test]
    fn removing_all_verify_transitions_is_not_a_proof() {
        let mutated = |state, event| match event {
            Event::Verify => None,
            _ => step(state, event),
        };
        assert!(verify_with(mutated).is_err());
    }
}

fn main() {
    match verify_with(step) {
        Ok((legal, total)) => {
            println!("CLONE_TEMPORAL_MODEL_PASS legal={legal} traces={total}");
        }
        Err(reason) => {
            eprintln!("CLONE_TEMPORAL_MODEL_FAIL {reason}");
            std::process::exit(1);
        }
    }
}
