//! A forward-only cache for "the first place at or after a position where something holds",
//! which keeps the hand-written matchers linear when they ask from many starts (review of
//! #200, F1 and F3).

/// The last answer, which covers every position from its query up to the place it found,
/// since nothing holds in between.
#[derive(Debug, Clone, Copy, Default)]
pub(super) enum Next {
    /// Nothing asked yet.
    #[default]
    Unknown,
    /// The first place at or after `from`, if any.
    Found {
        /// The position asked about.
        from: usize,
        /// What `find` gave for it.
        at: Option<usize>,
    },
}

impl Next {
    /// The first place at or after `pos`, through `find` when the cache does not cover it.
    /// Asked in increasing order, every byte is searched once.
    pub(super) fn at_or_after(
        &mut self,
        pos: usize,
        find: impl FnOnce(usize) -> Option<usize>,
    ) -> Option<usize> {
        match *self {
            Self::Found { from, at } if from <= pos && at.is_none_or(|at| pos <= at) => at,
            Self::Unknown | Self::Found { .. } => {
                let at = find(pos);
                *self = Self::Found { from: pos, at };
                at
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Next;

    /// The first multiple of 10 at or after `from`, counting the calls.
    fn tens(calls: &mut usize) -> impl FnOnce(usize) -> Option<usize> + '_ {
        move |from| {
            *calls = calls.saturating_add(1);
            (from..100).find(|n| n % 10 == 0)
        }
    }

    #[test]
    fn an_answer_covers_the_positions_up_to_it_and_none_covers_the_rest() {
        let mut next = Next::default();
        let mut calls = 0;
        let got: Vec<Option<usize>> = [3, 5, 10, 11, 11, 95, 99, 4]
            .into_iter()
            .map(|pos| next.at_or_after(pos, tens(&mut calls)))
            .collect();
        assert_eq!(
            got,
            [
                Some(10),
                Some(10),
                Some(10),
                Some(20),
                Some(20),
                None,
                None,
                Some(10)
            ]
        );
        // 3 (covers 5 and 10), 11 (covers 11), 95 (None covers 99), and 4 (behind the cache).
        assert_eq!(calls, 4);
    }
}
