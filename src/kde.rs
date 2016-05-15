//! Kernel density estimation

use std::iter::AdditiveIterator;
use std::ops::Fn;
use std::{cmp, comm, mem, os, ptr, raw};

use std_dev;

/// Univariate Kernel Density Estimator
#[experimental]
pub struct Kde<'a> {
    bandwidth: f64,
    kernel: fn(f64) -> f64,
    sample: &'a [f64],
}

impl<'a> Kde<'a> {
    /// Creates a new univariate kernel density estimator
    ///
    /// * Bandwidth: Estimated using Silverman's rule of thumb
    /// * Kernel: Gaussian
    // TODO bandwidth estimator should be configurable
    // TODO kernel should be configurable
    #[experimental]
    pub fn new(sample: &[f64]) -> Kde {
        Kde {
            bandwidth: silverman(sample),
            kernel: gaussian,
            sample: sample,
        }
    }

    /// Returns the bandwidth used by the estimator
    #[experimental]
    pub fn bandwidth(&self) -> f64 {
        self.bandwidth
    }

    /// Returns the sample used by the estimator
    #[experimental]
    pub fn sample(&self) -> &[f64] {
        self.sample
    }

    /// Sweeps the `[a, b]` range collecting `n` points of the estimated PDF
    #[experimental]
    pub fn sweep(&self, (a, b): (f64, f64), n: uint) -> Vec<(f64, f64)> {
        assert!(a < b);
        assert!(n > 1);

        let dx = (b - a) / (n - 1) as f64;
        let ncpus = os::num_cpus();

        // TODO Under what conditions should multi thread by favored?
        if ncpus > 1 {
            let chunk_size = n / ncpus + 1;
            let (tx, rx) = comm::channel();

            let mut pdf = Vec::with_capacity(n);
            unsafe { pdf.set_len(n) }
            let pdf_ptr = pdf.as_mut_ptr();

            // FIXME (when available) Use a safe fork-join API
            let &Kde { bandwidth: bw, kernel: k, sample: sample } = self;
            let raw::Slice { data: ptr, len: len } =
                unsafe { mem::transmute::<&[f64], raw::Slice<f64>>(sample) };

            for i in range(0, ncpus) {
                let tx = tx.clone();

                spawn(proc() {
                    // NB This task will finish before this slice becomes invalid
                    let sample: &[f64] =
                        unsafe { mem::transmute(raw::Slice { data: ptr, len: len }) };

                    let kde = Kde { bandwidth: bw, kernel: k, sample: sample };

                    let start = cmp::min(i * chunk_size, n) as int;
                    let end = cmp::min((i + 1) * chunk_size, n) as int;

                    let mut x = a + start as f64 * dx;
                    for j in range(start, end) {
                        unsafe { ptr::write(pdf_ptr.offset(j), (x, kde(x))) }
                        x += dx;
                    }

                    tx.send(());
                });
            }

            for _ in range(0, ncpus) {
                rx.recv();
            }

            pdf
        } else {
            let mut pdf = Vec::with_capacity(n);

            let mut x = a;
            for _ in range(0, n) {
                pdf.push((x, self(x)));

                x += dx;
            }

            pdf
        }
    }
}

impl<'a> Fn<(f64,), f64> for Kde<'a> {
    /// Estimates the probability *density* that the random variable takes the value `x`
    // XXX Can this be SIMD accelerated?
    #[experimental]
    extern "rust-call" fn call(&self, (x,): (f64,)) -> f64 {
        let frac_1_h = self.bandwidth.recip();
        let n = self.sample.len() as f64;
        let k = self.kernel;

        self.sample.iter().map(|&x_i| {
            k((x - x_i) * frac_1_h)
        }).sum() * frac_1_h / n
    }
}

/// Estimates the bandwidth using Silverman's rule of thumb
#[experimental]
fn silverman(x: &[f64]) -> f64 {
    static FACTOR: f64 = 4. / 3.;
    static EXPONENT: f64 = 1. / 5.;

    let n = x.len() as f64;
    let sigma = std_dev(x);

    sigma * (FACTOR / n).powf(EXPONENT)
}

/// The gaussian kernel
///
/// Equivalent to the Probability Density Function of a normally distributed random variable with
/// mean 0 and variance 1
#[experimental]
fn gaussian(x: f64) -> f64 {
    x.powi(2).exp().mul(&::std::f64::consts::PI_2).sqrt().recip()
}

#[cfg(test)]
mod test {
    use quickcheck::TestResult;
    use std::rand::{Rng, mod};
    use test::stats::Stats;

    use kde::Kde;
    use tol::is_close;

    mod gaussian {
        use quickcheck::TestResult;

        use super::super::gaussian;
        use tol::{TOLERANCE, is_close};

        #[quickcheck]
        fn symmetric(x: f64) -> bool {
            is_close(gaussian(-x), gaussian(x))
        }

        // Any [a b] integral should be in the range [0 1]
        #[quickcheck]
        fn integral(a: f64, b: f64) -> TestResult {
            static dX: f64 = 1e-3;

            if a > b {
                TestResult::discard()
            } else {
                let mut acc = 0.;
                let mut x = a;
                let mut y = gaussian(a);

                while x < b {
                    acc += dX * y / 2.;

                    x += dX;
                    y = gaussian(x);

                    acc += dX * y / 2.;
                }

                TestResult::from_bool(acc >= -TOLERANCE && acc <= 1. + TOLERANCE)
            }
        }
    }

    // The [-inf inf] integral of the estimated PDF should be one
    #[quickcheck]
    fn integral(sample_size: uint) -> TestResult {
        static dX: f64 = 1e-3;

        let data = if sample_size > 1 {
            let mut rng = rand::task_rng();

            Vec::from_fn(sample_size, |_| rng.gen::<f64>())
        } else {
            return TestResult::discard();
        };

        let data = data.as_slice();

        let kde = Kde::new(data);
        let h = kde.bandwidth();
        // NB Obviously a [-inf inf] integral is not feasible, but this range works quite well
        let (a, b) = (data.min() - 5. * h, data.max() + 5. * h);

        let mut acc = 0.;
        let mut x = a;
        let mut y = kde(a);

        while x < b {
            acc += dX * y / 2.;

            x += dX;
            y = kde(x);

            acc += dX * y / 2.;
        }

        TestResult::from_bool(is_close(acc, 1.))
    }
}

#[cfg(test)]
mod bench {
    use test::Bencher;
    use std::rand::{Rng, mod};

    use kde::Kde;

    static KDE_POINTS: uint = 500;
    static SAMPLE_SIZE: uint = 100_000;

    #[bench]
    fn sweep(b: &mut Bencher) {
        let mut rng = rand::task_rng();
        let data = Vec::from_fn(SAMPLE_SIZE, |_| rng.gen::<f64>());
        let kde = Kde::new(data.as_slice());

        b.iter(|| {
            kde.sweep((0., 1.), KDE_POINTS)
        })
    }
}
