//! The system's file and folder pickers (#1251). GPUI draws no window for
//! them, so `Window::has_active_prompt` can't see one: each counts here,
//! app-wide, from the moment it is asked for until its answer comes back,
//! it is cancelled, or the task waiting for it is dropped. While one is up,
//! nothing opens by itself under it (`Workspace::modal_up`).

use gpui::{App, Global, PathPromptOptions};
use std::{
    cell::Cell,
    future::Future,
    path::{Path, PathBuf},
    rc::Rc,
};

/// How many pickers are up.
#[derive(Clone, Default)]
struct Pending(Rc<Cell<usize>>);

impl Global for Pending {}

/// One picker up: it stops counting when dropped, however its answer ends.
struct Up(Rc<Cell<usize>>);

impl Up {
    fn raise(cx: &mut App) -> Self {
        let count = cx.default_global::<Pending>().0.clone();
        count.set(count.get() + 1);
        Self(count)
    }
}

impl Drop for Up {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_sub(1));
    }
}

/// A system picker is up.
pub(crate) fn up(cx: &App) -> bool {
    cx.try_global::<Pending>()
        .is_some_and(|pending| pending.0.get() > 0)
}

/// `answer`, counted as one picker up until it resolves or is dropped. A
/// picker gone without an answer reads as cancelled.
async fn counted<T, E>(
    up: Up,
    answer: impl Future<Output = Result<gpui::Result<Option<T>>, E>>,
) -> gpui::Result<Option<T>> {
    let answer = answer.await;
    drop(up);
    answer.unwrap_or(Ok(None))
}

/// `App::prompt_for_paths`, counted: the paths chosen, `None` if cancelled.
pub(crate) fn paths(
    cx: &mut App,
    options: PathPromptOptions,
) -> impl Future<Output = gpui::Result<Option<Vec<PathBuf>>>> + use<> {
    let up = Up::raise(cx);
    counted(up, cx.prompt_for_paths(options))
}

/// `App::prompt_for_new_path`, counted: the path chosen, `None` if
/// cancelled.
pub(crate) fn new_path(
    cx: &mut App,
    directory: &Path,
    suggested_name: Option<&str>,
) -> impl Future<Output = gpui::Result<Option<PathBuf>>> + use<> {
    let up = Up::raise(cx);
    counted(up, cx.prompt_for_new_path(directory, suggested_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Task, TestAppContext};
    use std::cell::RefCell;

    type Slot<T> = Rc<RefCell<Option<T>>>;

    /// `answer` awaited on the app's executor; what it gave lands in the
    /// slot. Dropping the task drops the answer unawaited.
    fn wait_for<T: 'static>(
        cx: &mut TestAppContext,
        answer: impl FnOnce(&mut App) -> Box<dyn Future<Output = T> + Unpin>,
    ) -> (Task<()>, Slot<T>) {
        let slot = Rc::new(RefCell::new(None));
        let kept = slot.clone();
        let task = cx.update(|cx| {
            let answer = answer(cx);
            cx.spawn(async move |_| *kept.borrow_mut() = Some(answer.await))
        });
        cx.run_until_parked();
        (task, slot)
    }

    fn options() -> PathPromptOptions {
        PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        }
    }

    /// Done when: a picker counts while up and stops when answered,
    /// cancelled, or dropped unanswered.
    #[gpui::test]
    fn a_picker_counts_until_it_ends(cx: &mut TestAppContext) {
        let is_up = |cx: &mut TestAppContext| cx.update(|cx| up(cx));
        assert!(!is_up(cx));
        // Answered.
        let (_task, chosen) = wait_for(cx, |cx| Box::new(Box::pin(paths(cx, options()))));
        assert!(is_up(cx));
        cx.simulate_path_prompt_response(|_| Some(vec![PathBuf::from("a.md")]));
        cx.run_until_parked();
        let chosen = chosen.borrow_mut().take().expect("answered").unwrap();
        assert_eq!(chosen, Some(vec![PathBuf::from("a.md")]));
        assert!(!is_up(cx));
        // Two at once, one cancelled.
        let (_first, cancelled) = wait_for(cx, |cx| Box::new(Box::pin(paths(cx, options()))));
        let (second, _) = wait_for(cx, |cx| {
            Box::new(Box::pin(new_path(cx, Path::new("."), Some("b.md"))))
        });
        assert!(is_up(cx));
        cx.simulate_path_prompt_response(|_| None);
        cx.run_until_parked();
        let cancelled = cancelled.borrow_mut().take().expect("answered").unwrap();
        assert_eq!(cancelled, None);
        assert!(is_up(cx), "the other is still up");
        // Dropped unanswered: the task waiting for it went away.
        drop(second);
        cx.run_until_parked();
        assert!(!is_up(cx));
    }
}
