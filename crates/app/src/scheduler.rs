use std::time::{Duration, Instant};

const BLINK: Duration = Duration::from_millis(500);
const RETRY: Duration = Duration::from_millis(50);

#[derive(Default)]
pub(crate) struct Scheduler {
    dirty: bool,
    pending: bool,
    presentable: bool,
    surface_occluded: bool,
    blink_at: Option<Instant>,
    retry_at: Option<Instant>,
    cursor_hidden: bool,
}

impl Scheduler {
    pub(crate) fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub(crate) fn set_presentable(&mut self, presentable: bool) {
        self.presentable = presentable;
        self.surface_occluded = false;
        self.retry_at = None;
        if presentable {
            self.invalidate();
        } else {
            self.pending = false;
            self.blink_at = None;
            self.cursor_hidden = false;
        }
    }

    pub(crate) fn request_frame(&mut self, now: Instant) -> bool {
        if self.presentable && !self.surface_occluded {
            if self.blink_at.is_some_and(|deadline| deadline <= now) {
                self.cursor_hidden = !self.cursor_hidden;
                self.blink_at = Some(now + BLINK);
                self.invalidate();
            }
            if self.retry_at.is_some_and(|deadline| deadline <= now) {
                self.retry_at = None;
            }
            if self.dirty && !self.pending && self.retry_at.is_none() {
                self.pending = true;
                return true;
            }
        }
        false
    }

    /// A system redraw also indicates that an occluded surface may be visible again.
    pub(crate) fn begin_frame(&mut self) -> bool {
        self.pending = false;
        if !self.presentable || self.retry_at.is_some() {
            return false;
        }
        self.surface_occluded = false;
        self.dirty = false;
        true
    }

    pub(crate) fn cursor_visible(&mut self, now: Instant, blinking: bool) -> bool {
        if blinking {
            self.blink_at.get_or_insert(now + BLINK);
        } else {
            self.blink_at = None;
            self.cursor_hidden = false;
        }
        !self.cursor_hidden
    }

    pub(crate) fn retry(&mut self, now: Instant) {
        self.invalidate();
        self.retry_at = Some(now + RETRY);
    }

    pub(crate) fn surface_occluded(&mut self) {
        self.surface_occluded = true;
        self.blink_at = None;
        self.cursor_hidden = false;
        self.invalidate();
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.blink_at.into_iter().chain(self.retry_at).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_redraws_and_does_not_poll_nonblinking_idle() {
        let now = Instant::now();
        let mut scheduler = Scheduler::default();
        scheduler.set_presentable(true);
        assert!(scheduler.request_frame(now));
        scheduler.invalidate();
        assert!(!scheduler.request_frame(now));
        assert!(scheduler.begin_frame());
        scheduler.invalidate();
        assert!(scheduler.request_frame(now));
        assert!(scheduler.begin_frame());
        assert!(scheduler.cursor_visible(now, false));
        assert!(!scheduler.request_frame(now + Duration::from_secs(3)));
        assert_eq!(scheduler.deadline(), None);
    }

    #[test]
    fn blink_and_recovery_use_deadlines() {
        let now = Instant::now();
        let mut scheduler = Scheduler::default();
        scheduler.set_presentable(true);
        assert!(scheduler.begin_frame());
        assert!(scheduler.cursor_visible(now, true));
        assert!(!scheduler.request_frame(now));
        assert_eq!(scheduler.deadline(), Some(now + BLINK));
        assert!(scheduler.request_frame(now + BLINK));
        assert!(scheduler.begin_frame());
        assert!(!scheduler.cursor_visible(now + BLINK, true));
        scheduler.retry(now + BLINK);
        assert!(!scheduler.request_frame(now + BLINK));
        assert!(scheduler.request_frame(now + BLINK + RETRY));
    }

    #[test]
    fn suspension_retains_the_need_to_redraw_without_timers() {
        let now = Instant::now();
        let mut scheduler = Scheduler::default();
        scheduler.set_presentable(true);
        assert!(scheduler.begin_frame());
        scheduler.cursor_visible(now, true);
        scheduler.set_presentable(false);
        scheduler.invalidate();
        assert!(!scheduler.request_frame(now + BLINK));
        assert_eq!(scheduler.deadline(), None);
        scheduler.set_presentable(true);
        assert!(scheduler.request_frame(now));
        assert!(scheduler.begin_frame());
        scheduler.surface_occluded();
        assert!(!scheduler.request_frame(now + BLINK));
        assert!(scheduler.begin_frame());
    }
}
