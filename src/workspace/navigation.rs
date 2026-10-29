//! Per-window, in-memory destination history (ADR 0081).

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Stop {
    Home,
    Tab(u64),
    Case(String),
}

#[derive(Default)]
pub(super) struct History {
    stops: Vec<Stop>,
    cursor: usize,
}

impl History {
    pub(super) fn visit(&mut self, stop: Stop) {
        if self.stops.get(self.cursor) == Some(&stop) {
            return;
        }
        self.stops.truncate(self.cursor + 1);
        self.stops.push(stop);
        if self.stops.len() > 50 {
            self.stops.remove(0);
        }
        self.cursor = self.stops.len() - 1;
    }

    fn target_index(&self, backwards: bool, live: impl Fn(&Stop) -> bool) -> Option<usize> {
        if backwards {
            (0..self.cursor).rev().find(|&i| live(&self.stops[i]))
        } else {
            (self.cursor + 1..self.stops.len()).find(|&i| live(&self.stops[i]))
        }
    }

    pub(super) fn target(&self, backwards: bool, live: impl Fn(&Stop) -> bool) -> Option<&Stop> {
        self.target_index(backwards, live).map(|i| &self.stops[i])
    }

    pub(super) fn travel(&mut self, backwards: bool, live: impl Fn(&Stop) -> bool) -> Option<Stop> {
        self.cursor = self.target_index(backwards, live)?;
        Some(self.stops[self.cursor].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_replay_without_recording_and_new_destinations_clear_forward() {
        let mut history = History::default();
        for stop in [
            Stop::Home,
            Stop::Tab(1),
            Stop::Case("raman".into()),
            Stop::Tab(2),
        ] {
            history.visit(stop);
        }
        assert_eq!(
            history.travel(true, |_| true),
            Some(Stop::Case("raman".into()))
        );
        assert_eq!(history.travel(true, |_| true), Some(Stop::Tab(1)));
        assert_eq!(
            history.travel(false, |_| true),
            Some(Stop::Case("raman".into()))
        );
        history.visit(Stop::Home);
        assert_eq!(history.target(false, |_| true), None);
        // Re-selecting the current place doesn't add a stop or destroy Forward.
        history.travel(true, |_| true);
        history.visit(Stop::Case("raman".into()));
        assert_eq!(history.travel(false, |_| true), Some(Stop::Home));
    }

    #[test]
    fn skips_closed_tabs_in_both_directions_and_keeps_the_cursor_at_the_end() {
        let mut history = History::default();
        for stop in [Stop::Home, Stop::Tab(1), Stop::Tab(2), Stop::Tab(3)] {
            history.visit(stop);
        }
        let live = |stop: &Stop| !matches!(stop, Stop::Tab(1 | 2));
        assert_eq!(history.target(true, live), Some(&Stop::Home));
        assert_eq!(history.travel(true, live), Some(Stop::Home));
        assert_eq!(history.travel(true, live), None);
        assert_eq!(history.travel(false, live), Some(Stop::Tab(3)));
        assert_eq!(history.travel(false, live), None);
    }

    #[test]
    fn caps_history_at_fifty_stops_and_a_new_window_starts_empty() {
        let mut history = History::default();
        for id in 0..75 {
            history.visit(Stop::Tab(id));
        }
        let mut visited = Vec::new();
        while let Some(stop) = history.travel(true, |_| true) {
            visited.push(stop);
        }
        assert_eq!(visited.len(), 49);
        assert_eq!(visited.last(), Some(&Stop::Tab(25)));
        assert_eq!(History::default().target(true, |_| true), None);
    }
}
