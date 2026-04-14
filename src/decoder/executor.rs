#![forbid(unsafe_code)]
//! TileExecutor trait; sequential default.

/// Dispatches per-tile work to an executor.
///
/// M0 ships the sequential implementation only. Later milestones can add
/// threaded implementations under the same trait.
pub(crate) trait TileExecutor {
    fn for_each_tile<F>(&self, num_tiles: usize, f: F)
    where
        F: FnMut(usize);
}

pub(crate) struct Sequential;

impl TileExecutor for Sequential {
    fn for_each_tile<F>(&self, num_tiles: usize, f: F)
    where
        F: FnMut(usize),
    {
        let mut f = f;
        for i in 0..num_tiles {
            f(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn sequential_executor_visits_every_tile_in_order() {
        let exec = Sequential;
        let counter = AtomicUsize::new(0);
        exec.for_each_tile(4, |i| {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            assert_eq!(n, i);
        });
        assert_eq!(counter.load(Ordering::SeqCst), 4);
    }
}
