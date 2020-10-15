//! Mixed bootstrap

use float::Float;
use rayon::prelude::*;

use tuple::{Tuple, TupledDistributionsBuilder};
use univariate::resamples::Resamples;
use univariate::Sample;

/// Performs a *mixed* two-sample bootstrap
pub fn bootstrap<A, T, S>(
    a: &Sample<A>,
    b: &Sample<A>,
    nresamples: usize,
    statistic: S,
) -> T::Distributions
where
    A: Float,
    S: Fn(&Sample<A>, &Sample<A>) -> T + Sync,
    T: Tuple + Send,
    T::Distributions: Send,
    T::Builder: Send,
{
    let n_a = a.len();
    let n_b = b.len();
    let mut c = Vec::with_capacity(n_a + n_b);
    c.extend_from_slice(a);
    c.extend_from_slice(b);
    let c = Sample::new(&c);

    (0..nresamples)
        .into_par_iter()
        .map_init(
            || Resamples::new(c),
            |resamples, _| {
                let resample = resamples.next();
                let a: &Sample<A> = Sample::new(&resample[..n_a]);
                let b: &Sample<A> = Sample::new(&resample[n_a..]);

                statistic(a, b)
            },
        )
        .fold(
            || T::Builder::new(0),
            |mut sub_distributions, sample| {
                sub_distributions.push(sample);
                sub_distributions
            },
        )
        .reduce(
            || T::Builder::new(0),
            |mut a, mut b| {
                a.extend(&mut b);
                a
            },
        )
        .complete()
}
