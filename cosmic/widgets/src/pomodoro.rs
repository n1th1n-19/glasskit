//! Pure 25/5 focus timer state machine.
pub const WORK: u32 = 25 * 60;
pub const BREAK: u32 = 5 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Work,
    Break,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub phase: Phase,
    pub remaining: u32,
    pub running: bool,
}

impl Phase {
    fn len(self) -> u32 {
        if self == Phase::Work { WORK } else { BREAK }
    }
    fn other(self) -> Phase {
        if self == Phase::Work { Phase::Break } else { Phase::Work }
    }
}

impl State {
    pub fn new() -> Self {
        State { phase: Phase::Work, remaining: WORK, running: false }
    }

    /// One second elapsed. Returns the new state and whether a phase just ended.
    pub fn tick(self) -> (State, bool) {
        if !self.running {
            return (self, false);
        }
        if self.remaining > 1 {
            return (State { remaining: self.remaining - 1, ..self }, false);
        }
        (self.skip(), true)
    }

    /// Jump to the next phase, keeping the running flag.
    pub fn skip(self) -> State {
        let phase = self.phase.other();
        State { phase, remaining: phase.len(), running: self.running }
    }

    pub fn toggle(self) -> State {
        State { running: !self.running, ..self }
    }

    pub fn reset(self) -> State {
        State::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_tick_is_noop() {
        assert_eq!(State::new().tick(), (State::new(), false));
    }

    #[test]
    fn work_to_break_to_work() {
        let s = State { phase: Phase::Work, remaining: 1, running: true };
        let (b, ended) = s.tick();
        assert!(ended);
        assert_eq!((b.phase, b.remaining, b.running), (Phase::Break, BREAK, true));
        let (w, ended) = State { remaining: 1, ..b }.tick();
        assert!(ended);
        assert_eq!((w.phase, w.remaining), (Phase::Work, WORK));
    }

    #[test]
    fn counts_down_skip_reset() {
        let s = State::new().toggle();
        assert_eq!(s.tick().0.remaining, WORK - 1);
        assert_eq!(s.skip().phase, Phase::Break);
        assert_eq!(s.skip().remaining, BREAK);
        assert_eq!(s.tick().0.reset(), State::new());
    }
}
