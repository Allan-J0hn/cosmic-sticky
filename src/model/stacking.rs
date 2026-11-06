// SPDX-License-Identifier: GPL-3.0-only

//! Keeping a desk surface above other clients' surfaces on the same layer.
//!
//! cosmic-comp stacks the surfaces of one layer by the order they were mapped, newest on top,
//! and gives a click to the topmost one whose input region contains it. cosmic-files puts a
//! transparent full-output surface that takes all input on the Bottom layer of every output,
//! so when it maps after us (at login, for instance) the notes stay visible but every click on
//! them goes to it. Recreating our surface is the only way back on top.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

/// A client that also restacks itself would take turns with us forever.
const MAX_RESTACKS: usize = 4;
const RESTACK_WINDOW: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Already known: it was there before our current surface.
    Keep,
    Restack,
    /// Restacked too often lately.
    GiveUp,
}

/// The other clients' same-layer surfaces seen overlapping one desk.
#[derive(Debug, Default)]
pub struct Stacking {
    known: HashSet<String>,
    restacks: VecDeque<Instant>,
}

impl Stacking {
    /// Another client's surface on our layer reported as overlapping the desk. The compositor
    /// also reports every existing one to a newly created surface, which is why known
    /// surfaces are remembered across restacks.
    pub fn competitor(&mut self, identifier: &str, now: Instant) -> Verdict {
        if !self.known.insert(identifier.to_owned()) {
            return Verdict::Keep;
        }
        while self
            .restacks
            .front()
            .is_some_and(|t| now.duration_since(*t) >= RESTACK_WINDOW)
        {
            self.restacks.pop_front();
        }
        if self.restacks.len() >= MAX_RESTACKS {
            return Verdict::GiveUp;
        }
        self.restacks.push_back(now);
        Verdict::Restack
    }

    /// Identifiers are reused once a surface is gone, so it is forgotten.
    pub fn gone(&mut self, identifier: &str) {
        self.known.remove(identifier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESKTOP: &str = "wl_surface@40[7]";

    #[test]
    fn a_new_surface_restacks_once() {
        let now = Instant::now();
        let mut s = Stacking::default();
        assert_eq!(s.competitor(DESKTOP, now), Verdict::Restack);
        assert_eq!(s.competitor(DESKTOP, now), Verdict::Keep);
        assert_eq!(
            s.competitor(DESKTOP, now + Duration::from_secs(60)),
            Verdict::Keep
        );
    }

    #[test]
    fn a_recreated_surface_with_a_reused_identifier_restacks_again() {
        let now = Instant::now();
        let mut s = Stacking::default();
        assert_eq!(s.competitor(DESKTOP, now), Verdict::Restack);
        s.gone(DESKTOP);
        assert_eq!(s.competitor(DESKTOP, now), Verdict::Restack);
    }

    #[test]
    fn forgetting_an_unknown_surface_is_harmless() {
        let now = Instant::now();
        let mut s = Stacking::default();
        s.gone(DESKTOP);
        assert_eq!(s.competitor(DESKTOP, now), Verdict::Restack);
    }

    #[test]
    fn surfaces_are_tracked_separately() {
        let now = Instant::now();
        let mut s = Stacking::default();
        assert_eq!(s.competitor(DESKTOP, now), Verdict::Restack);
        assert_eq!(s.competitor("wl_surface@12[9]", now), Verdict::Restack);
        s.gone("wl_surface@12[9]");
        assert_eq!(s.competitor(DESKTOP, now), Verdict::Keep);
    }

    #[test]
    fn gives_up_when_restacking_too_often_then_recovers() {
        let start = Instant::now();
        let mut s = Stacking::default();
        for i in 0..MAX_RESTACKS {
            let at = start + Duration::from_secs(i as u64);
            assert_eq!(s.competitor(&format!("a{i}"), at), Verdict::Restack);
        }
        let at = start + Duration::from_secs(MAX_RESTACKS as u64);
        assert_eq!(s.competitor("late", at), Verdict::GiveUp);
        assert_eq!(s.competitor("late", at), Verdict::Keep);

        assert_eq!(
            s.competitor("later", start + RESTACK_WINDOW),
            Verdict::Restack
        );
        let after = start + RESTACK_WINDOW + Duration::from_secs(MAX_RESTACKS as u64);
        assert_eq!(s.competitor("much later", after), Verdict::Restack);
    }
}
