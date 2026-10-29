//! Per-window, in-memory destination history (ADR 0081).

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Stop {
    Home,
    Tab(u64),
    Case(String),
}

/// How the history records arriving at a place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Arrival {
    /// The user went there: a new stop, and Forward is discarded.
    Visit,
    /// The current stop closed or was deleted, and this place took its page:
    /// it replaces that stop, and Forward is kept.
    Replace,
    /// A close or quit prompt brought it to the front: not a stop.
    Unrecorded,
}

#[derive(Default)]
pub(super) struct History {
    stops: Vec<Stop>,
    cursor: usize,
}

impl History {
    pub(super) fn current(&self) -> Option<&Stop> {
        self.stops.get(self.cursor)
    }

    pub(super) fn arrive(&mut self, stop: Stop, arrival: Arrival) {
        match arrival {
            Arrival::Visit => self.visit(stop),
            Arrival::Replace if !self.stops.is_empty() => {
                self.stops[self.cursor] = stop;
                self.normalize(|_| true);
            }
            Arrival::Replace => self.visit(stop),
            Arrival::Unrecorded => {}
        }
    }

    fn visit(&mut self, stop: Stop) {
        if self.current() == Some(&stop) {
            return;
        }
        self.stops.truncate(self.cursor + 1);
        self.stops.push(stop);
        if self.stops.len() > 50 {
            self.stops.remove(0);
        }
        self.cursor = self.stops.len() - 1;
    }

    /// A tab closed or a case was deleted: drop its stops. The cursor stays
    /// on the stop it was on, or on the one before it when that one went.
    pub(super) fn forget(&mut self, gone: &Stop) {
        self.normalize(|stop| stop != gone);
    }

    /// Keep the stops `keep` accepts, merging neighbours that became equal.
    fn normalize(&mut self, keep: impl Fn(&Stop) -> bool) {
        let mut kept: Vec<Stop> = Vec::with_capacity(self.stops.len());
        let mut cursor = 0;
        for (index, stop) in std::mem::take(&mut self.stops).into_iter().enumerate() {
            if keep(&stop) && kept.last() != Some(&stop) {
                kept.push(stop);
            }
            if index == self.cursor {
                cursor = kept.len().saturating_sub(1);
            }
        }
        self.stops = kept;
        self.cursor = cursor;
    }

    /// The nearest live stop that isn't `here`. When an unrecorded arrival
    /// left the current stop, Back can return to it.
    pub(super) fn target_index(
        &self,
        backwards: bool,
        here: &Stop,
        live: impl Fn(&Stop) -> bool,
    ) -> Option<usize> {
        let wanted = |&i: &usize| &self.stops[i] != here && live(&self.stops[i]);
        if backwards {
            (0..(self.cursor + 1).min(self.stops.len()))
                .rev()
                .find(wanted)
        } else {
            (self.cursor + 1..self.stops.len()).find(wanted)
        }
    }

    pub(super) fn target(
        &self,
        backwards: bool,
        here: &Stop,
        live: impl Fn(&Stop) -> bool,
    ) -> Option<&Stop> {
        self.target_index(backwards, here, live)
            .map(|i| &self.stops[i])
    }

    /// Move to the stop `target_index` found.
    pub(super) fn go(&mut self, index: usize) -> Stop {
        self.cursor = index;
        self.stops[index].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Back or Forward from the current stop, as the workspace does.
    fn travel(
        history: &mut History,
        backwards: bool,
        live: impl Fn(&Stop) -> bool,
    ) -> Option<Stop> {
        let here = history.current()?.clone();
        let index = history.target_index(backwards, &here, live)?;
        Some(history.go(index))
    }

    fn target(history: &History, backwards: bool) -> Option<&Stop> {
        history.target(backwards, history.current()?, |_| true)
    }

    fn visited(stops: impl IntoIterator<Item = Stop>) -> History {
        let mut history = History::default();
        for stop in stops {
            history.arrive(stop, Arrival::Visit);
        }
        history
    }

    #[test]
    fn stops_replay_without_recording_and_new_destinations_clear_forward() {
        let mut history = visited([
            Stop::Home,
            Stop::Tab(1),
            Stop::Case("raman".into()),
            Stop::Tab(2),
        ]);
        assert_eq!(
            travel(&mut history, true, |_| true),
            Some(Stop::Case("raman".into()))
        );
        assert_eq!(travel(&mut history, true, |_| true), Some(Stop::Tab(1)));
        assert_eq!(
            travel(&mut history, false, |_| true),
            Some(Stop::Case("raman".into()))
        );
        history.arrive(Stop::Home, Arrival::Visit);
        assert_eq!(target(&history, false), None);
        // Re-selecting the current place doesn't add a stop or destroy Forward.
        travel(&mut history, true, |_| true);
        history.arrive(Stop::Case("raman".into()), Arrival::Visit);
        assert_eq!(travel(&mut history, false, |_| true), Some(Stop::Home));
    }

    #[test]
    fn skips_closed_tabs_in_both_directions_and_keeps_the_cursor_at_the_end() {
        let mut history = visited([Stop::Home, Stop::Tab(1), Stop::Tab(2), Stop::Tab(3)]);
        let live = |stop: &Stop| !matches!(stop, Stop::Tab(1 | 2));
        assert_eq!(history.target(true, &Stop::Tab(3), live), Some(&Stop::Home));
        assert_eq!(travel(&mut history, true, live), Some(Stop::Home));
        assert_eq!(travel(&mut history, true, live), None);
        assert_eq!(travel(&mut history, false, live), Some(Stop::Tab(3)));
        assert_eq!(travel(&mut history, false, live), None);
    }

    #[test]
    fn caps_history_at_fifty_stops_and_a_new_window_starts_empty() {
        let mut history = visited((0..75).map(Stop::Tab));
        let mut visited = Vec::new();
        while let Some(stop) = travel(&mut history, true, |_| true) {
            visited.push(stop);
        }
        assert_eq!(visited.len(), 49);
        assert_eq!(visited.last(), Some(&Stop::Tab(25)));
        assert_eq!(History::default().target(true, &Stop::Home, |_| true), None);
    }

    #[test]
    fn closing_the_current_tab_replaces_its_stop_and_merges_neighbours() {
        // Home → A → B, close B: A takes its page.
        let mut history = visited([Stop::Home, Stop::Tab(1), Stop::Tab(2)]);
        history.arrive(Stop::Tab(1), Arrival::Replace);
        history.forget(&Stop::Tab(2));
        assert_eq!(history.current(), Some(&Stop::Tab(1)));
        assert_eq!(target(&history, true), Some(&Stop::Home));
        assert_eq!(target(&history, false), None);
        // Home → A, close A (the last tab): Home shows, nothing either way.
        let mut history = visited([Stop::Home, Stop::Tab(1)]);
        history.arrive(Stop::Home, Arrival::Replace);
        history.forget(&Stop::Tab(1));
        assert_eq!(history.current(), Some(&Stop::Home));
        assert_eq!(
            (target(&history, true), target(&history, false)),
            (None, None)
        );
    }

    #[test]
    fn replacing_after_back_keeps_forward() {
        let mut history = visited([Stop::Home, Stop::Tab(1), Stop::Tab(2), Stop::Tab(3)]);
        travel(&mut history, true, |_| true);
        history.arrive(Stop::Tab(4), Arrival::Replace);
        history.forget(&Stop::Tab(2));
        assert_eq!(target(&history, true), Some(&Stop::Tab(1)));
        assert_eq!(target(&history, false), Some(&Stop::Tab(3)));
    }

    #[test]
    fn forgetting_drops_every_stop_of_a_place_and_keeps_the_cursor_on_its_stop() {
        let case = || Stop::Case("raman".into());
        let mut history = visited([
            Stop::Tab(1),
            case(),
            Stop::Tab(1),
            Stop::Home,
            case(),
            Stop::Tab(2),
        ]);
        travel(&mut history, true, |_| true);
        travel(&mut history, true, |_| true);
        assert_eq!(history.current(), Some(&Stop::Home));
        history.forget(&case());
        assert_eq!(history.stops, [Stop::Tab(1), Stop::Home, Stop::Tab(2)]);
        assert_eq!(history.current(), Some(&Stop::Home));
        // The cursor's own stop went: it rests on the one before.
        history.forget(&Stop::Home);
        assert_eq!(history.stops, [Stop::Tab(1), Stop::Tab(2)]);
        assert_eq!(history.current(), Some(&Stop::Tab(1)));
        history.forget(&Stop::Tab(1));
        history.forget(&Stop::Tab(2));
        assert_eq!(history.current(), None);
        assert_eq!(
            History::default().target(false, &Stop::Home, |_| true),
            None
        );
    }

    #[test]
    fn unrecorded_arrivals_keep_forward_and_never_target_the_page_shown() {
        let mut history = visited([Stop::Home, Stop::Tab(1), Stop::Tab(2), Stop::Tab(3)]);
        travel(&mut history, true, |_| true);
        // A close prompt brings tab 1 to the front, and the user cancels.
        history.arrive(Stop::Tab(1), Arrival::Unrecorded);
        let here = Stop::Tab(1);
        assert_eq!(history.target(false, &here, |_| true), Some(&Stop::Tab(3)));
        // Back returns to the stop the prompt left, never to tab 1 itself.
        assert_eq!(history.target(true, &here, |_| true), Some(&Stop::Tab(2)));
        let index = history.target_index(true, &here, |_| true).unwrap();
        assert_eq!(history.go(index), Stop::Tab(2));
        assert_eq!(
            history.target(true, &Stop::Tab(2), |_| true),
            Some(&Stop::Tab(1))
        );
    }
}
