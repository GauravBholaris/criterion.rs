use std::cmp;
use std::fmt::Show;
use std::io::Command;
use std::num;

use bencher::Bencher;
use fs;
use outliers::Outliers;
use plot;
use statistics::{Estimate,Estimates,Mean,Median,MedianAbsDev,Sample,StdDev};
use stream::Stream;
use target::{Function,Program,Target};
use time::Time;
use time::prefix::{Mili,Nano};
use time::traits::{Milisecond,Nanosecond,Prefix,Second};
use time::types::Ns;
use time::unit;
use time;

// FIXME Sorry! This module is a mess :/

/// The "criterion" for the benchmark, which is also the benchmark "builder"
#[experimental]
pub struct Criterion {
    confidence_level: f64,
    measurement_time: Ns<u64>,
    noise_tolerance: f64,
    nresamples: uint,
    sample_size: uint,
    warm_up_time: Ns<u64>,
}

impl Criterion {
    /// This is the default criterion:
    ///
    /// * Confidence level: 0.95
    ///
    /// * Measurement time: 10 ms
    ///
    /// * Noise tolerance: 0.01 (1%)
    ///
    /// * Bootstrap with 100 000 resamples
    ///
    /// * Sample size: 100 measurements
    ///
    /// * Warm-up time: 1 s
    #[experimental]
    pub fn default() -> Criterion {
        Criterion {
            confidence_level: 0.95,
            measurement_time: 10.ms().to::<Nano>(),
            noise_tolerance: 0.01,
            nresamples: 100_000,
            sample_size: 100,
            warm_up_time: 1.s().to::<Nano>(),
        }
    }

    #[experimental]
    pub fn confidence_level<'a>(&'a mut self, cl: f64) -> &'a mut Criterion {
        assert!(cl > 0.0 && cl < 1.0);

        self.confidence_level = cl;
        self
    }

    #[allow(visible_private_types)]
    #[experimental]
    pub fn measurement_time<'a,
                            P: Prefix>(
                            &'a mut self,
                            t: Time<P, unit::Second, u64>)
                            -> &'a mut Criterion {
        self.measurement_time = t.to::<Nano>();
        self
    }

    #[experimental]
    pub fn noise_tolerance<'a>(&'a mut self, nt: f64) -> &'a mut Criterion {
        assert!(nt >= 0.0);

        self.noise_tolerance = nt;
        self
    }

    #[experimental]
    pub fn nresamples<'a>(&'a mut self, n: uint) -> &'a mut Criterion {
        self.nresamples = n;
        self
    }

    #[experimental]
    pub fn sample_size<'a>(&'a mut self, n: uint) -> &'a mut Criterion {
        self.sample_size = n;
        self
    }

    #[allow(visible_private_types)]
    #[experimental]
    pub fn warm_up_time<'a,
                        P: Prefix>(
                        &'a mut self,
                        t: Time<P, unit::Second, u64>)
                        -> &'a mut Criterion {
        self.warm_up_time = t.to::<Nano>();
        self
    }

    /// Benchmark a function. See `Bench::iter()` for an example of how `fun` should look
    #[experimental]
    pub fn bench<'a,
                 I: Str>(
                 &'a mut self,
                 id: I,
                 fun: |&mut Bencher|)
                 -> &'a mut Criterion {
        let id = id.as_slice();

        local_data_key!(clock: Ns<f64>);

        if clock.get().is_none() {
            clock.replace(Some(clock_cost(self)));
        }

        // TODO Use clock cost to set a minimum `measurement_time`

        bench(id, Function(Some(fun)), self);

        println!("");

        self
    }

    /// Benchmark a family of functions
    ///
    /// `fun` will be benchmarked under each input
    ///
    /// For example, if you want to benchmark `Vec::from_elem` with different size, use these
    /// arguments:
    ///
    ///     let fun = |b, n| Vec::from_elem(n, 0u);
    ///     let inputs = [100, 10_000, 1_000_000];
    ///
    /// This is equivalent to calling `bench` on each of the following functions:
    ///
    ///     let fun1 = |b| Vec::from_elem(100, 0u);
    ///     let fun2 = |b| Vec::from_elem(10_000, 0u);
    ///     let fun3 = |b| Vec::from_elem(1_000_000, 0u);
    #[experimental]
    pub fn bench_family<'a,
                        I: Show,
                        S: Str>(
                        &'a mut self,
                        id: S,
                        fun: |&mut Bencher, &I|,
                        inputs: &[I])
                        -> &'a mut Criterion {
        let id = id.as_slice();

        for input in inputs.iter() {
            self.bench(format!("{}/{}", id, input), |b| fun(b, input));
        }

        print!("Summarizing results of {}... ", id);
        plot::summarize(&Path::new(".criterion").join(id));
        println!("DONE\n");

        self
    }

    /// Benchmark an external program
    ///
    /// The program must conform to the following specification:
    ///
    ///     extern crate time;
    ///
    ///     fn main() {
    ///         // Optional: Get the program arguments
    ///         let args = std::os::args();
    ///
    ///         for line in std::io::stdio::stdin().lines() {
    ///             // Get number of iterations to do
    ///             let iters: u64 = from_str(line.unwrap().as_slice().trim()).unwrap();
    ///
    ///             // Setup
    ///
    ///             // (For best results, use a monotonic timer)
    ///             let start = time::precise_time_ns();
    ///             for _ in range(0, iters) {
    ///                 // Routine to benchmark goes here
    ///             }
    ///             let end = time::precise_time_ns();
    ///
    ///             // Teardown
    ///
    ///             // Report back the time (in nanoseconds) required to execute the routine
    ///             // `iters` times
    ///             println!("{}", end - start);
    ///         }
    ///     }
    ///
    /// For example, to benchmark a python script use the following command
    ///
    ///     let cmd = Command::new("python3").args(["-O", "clock.py"]);
    #[experimental]
    pub fn bench_prog<'a,
                      S: Str>(
                      &'a mut self,
                      id: S,
                      prog: &Command)
                      -> &'a mut Criterion {
        let id = id.as_slice();

        bench(id, Program(Stream::spawn(prog)), self);

        println!("");

        self
    }

    /// Benchmark an external program under various inputs
    ///
    /// For example, to benchmark a python script under various inputs, use this combination:
    ///
    ///     let cmd = Command::new("python3").args(["-O", "fib.py"]);
    ///     let inputs = [5u, 10, 15];
    ///
    /// This is equivalent to calling `bench_prog` on each of the following commands:
    ///
    ///     let cmd1 = Command::new("python3").args(["-O", "fib.py", "5"]);
    ///     let cmd2 = Command::new("python3").args(["-O", "fib.py", "10"]);
    ///     let cmd2 = Command::new("python3").args(["-O", "fib.py", "15"]);
    #[experimental]
    pub fn bench_prog_family<'a,
                             I: Show,
                             S: Str>(
                             &'a mut self,
                             id: S,
                             prog: &Command,
                             inputs: &[I])
                             -> &'a mut Criterion {
        let id = id.as_slice();

        for input in inputs.iter() {
            self.bench_prog(format!("{}/{}", id, input), prog.clone().arg(format!("{}", input)));
        }

        print!("Summarizing results of {}... ", id);
        plot::summarize(&Path::new(".criterion").join(id));
        println!("DONE\n");

        self
    }
}

fn bench(id: &str, mut target: Target, criterion: &Criterion) {
    println!("Benchmarking {}", id);

    rename_new_dir_to_base(id);
    build_directory_skeleton(id);

    let root = Path::new(".criterion").join(id);
    let base_dir = root.join("base");
    let change_dir = root.join("change");
    let new_dir = root.join("new");
    let sample = take_sample(&mut target, criterion).unwrap();
    sample.save(&new_dir.join("sample.json"));

    plot::sample(&sample, new_dir.join("points.png"));

    let outliers = Outliers::classify(sample.as_slice());
    outliers.report();
    outliers.save(&new_dir.join("outliers/classification.json"));
    plot::outliers(&outliers, new_dir.join("outliers/boxplot.png"));

    println!("> Estimating the statistics of the sample");
    let nresamples = criterion.nresamples;
    let cl = criterion.confidence_level;
    println!("  > Bootstrapping the sample with {} resamples", nresamples);
    let (estimates, distributions) =
        sample.bootstrap([Mean, Median, StdDev, MedianAbsDev], nresamples, cl);
    estimates.save(&new_dir.join("bootstrap/estimates.json"));

    report_time(&estimates);
    plot::pdf(&sample, &estimates, new_dir.join("pdf.png"));
    plot::time_distributions(&distributions,
                             &estimates,
                             &new_dir.join("bootstrap/distribution"));

    if !base_dir.exists() {
        return;
    }

    println!("Comparing with previous sample");
    let base_sample = Sample::<Vec<f64>>::load(&base_dir.join("sample.json"));

    let both_dir = root.join("both");
    plot::both::pdfs(&base_sample, &sample, both_dir.join("pdfs.png"));
    plot::both::points(&base_sample, &sample, both_dir.join("points.png"));

    let nresamples_sqrt = (nresamples as f64).sqrt().ceil() as uint;
    let nresamples = nresamples_sqrt * nresamples_sqrt;

    println!("> Bootstrapping with {} resamples", nresamples);
    let (estimates, distributions) =
        sample.bootstrap_compare(&base_sample, [Mean, Median], nresamples_sqrt, cl);
    estimates.save(&change_dir.join("bootstrap/estimates.json"));

    report_change(&estimates);
    plot::ratio_distributions(&distributions,
                              &estimates,
                              &change_dir.join("bootstrap/distribution"));

    let noise = criterion.noise_tolerance;
    let mut regressed = vec!();
    for &statistic in [Mean, Median].iter() {
        let estimate = estimates.get(statistic);
        let result = compare_to_noise(estimate, noise);

        let p = estimate.point_estimate();
        match result {
            Improved => {
                println!("  > {} has improved by {:.2}%", statistic, -100.0 * p);
                regressed.push(false);
            },
            Regressed => {
                println!("  > {} has regressed by {:.2}%", statistic, 100.0 * p);
                regressed.push(true);
            },
            WithinNoise => {
                println!("  > {} is within noise levels", statistic);
                regressed.push(false);
            },
        }
    }
    if regressed.iter().all(|&x| x) {
        fail!("regression");
    }
}

fn extrapolate_iters(iters: u64, took: Ns<u64>, want: Ns<u64>) -> (Ns<f64>, u64) {
    let e_iters = cmp::max(want * iters / took, 1);
    let e_time = (took * e_iters).cast::<f64>() / iters as f64;

    (e_time, e_iters)
}

fn clock_cost(criterion: &Criterion) -> Ns<f64> {
    println!("Estimating the cost of `precise_time_ns`");

    let mut f = Function(Some(|b: &mut Bencher| b.iter(|| time::now())));

    let sample = take_sample(&mut f, criterion);

    let median = sample.unwrap().compute(Mean).ns();

    println!("> Median: {}\n", median);
    median
}

fn take_sample(t: &mut Target, criterion: &Criterion) -> Ns<Sample<Vec<f64>>> {
    let wu_time = criterion.warm_up_time;
    println!("> Warming up for {}", wu_time.to::<Mili>())
    let (took, iters) = t.warm_up(wu_time);

    let m_time = criterion.measurement_time;
    let (m_time, m_iters) = extrapolate_iters(iters, took, m_time);

    let sample_size = criterion.sample_size;
    println!("> Collecting {} measurements, {} iters each in estimated {}",
             sample_size,
             m_iters,
             format_time((m_time * sample_size as f64).unwrap()));

    let sample = t.bench(sample_size, m_iters).unwrap();

    sample.ns()
}

fn rename_new_dir_to_base(id: &str) {
    let root_dir = Path::new(".criterion").join(id);
    let base_dir = root_dir.join("base");
    let new_dir = root_dir.join("new");

    if base_dir.exists() { fs::rmrf(&base_dir) }
    if new_dir.exists() { fs::mv(&new_dir, &base_dir) };
}

fn build_directory_skeleton(id: &str) {
    let root = Path::new(".criterion").join(id);
    fs::mkdirp(&root.join("both"));
    fs::mkdirp(&root.join("change/bootstrap/distribution"));
    fs::mkdirp(&root.join("new/bootstrap/distribution"));
    fs::mkdirp(&root.join("new/outliers"));
}

fn format_short(n: f64) -> String {
    if n < 10.0 { format!("{:.4}", n) }
    else if n < 100.0 { format!("{:.3}", n) }
    else if n < 1000.0 { format!("{:.2}", n) }
    else { format!("{}", n) }
}

fn format_signed_short(n: f64) -> String {
    let n_abs = n.abs();

    if n_abs < 10.0 { format!("{:+.4}", n) }
    else if n_abs < 100.0 { format!("{:+.3}", n) }
    else if n_abs < 1000.0 { format!("{:+.2}", n) }
    else { format!("{:+}", n) }
}

fn report_time(estimates: &Estimates) {
    for &statistic in [Mean, Median, StdDev, MedianAbsDev].iter() {
        let estimate = estimates.get(statistic);
        let p = format_time(estimate.point_estimate());
        let ci = estimate.confidence_interval();
        let lb = format_time(ci.lower_bound());
        let ub = format_time(ci.upper_bound());
        let se = format_time(estimate.standard_error());
        let cl = ci.confidence_level();

        println!("  > {:<7} {} ± {} [{} {}] {}% CI", statistic, p, se, lb, ub, cl * 100.0);
    }
}

fn format_time(ns: f64) -> String {
    if ns < 1.0 {
        format!("{:>6} ps", format_short(ns * 1e3))
    } else if ns < num::pow(10.0, 3) {
        format!("{:>6} ns", format_short(ns))
    } else if ns < num::pow(10.0, 6) {
        format!("{:>6} us", format_short(ns / 1e3))
    } else if ns < num::pow(10.0, 9) {
        format!("{:>6} ms", format_short(ns / 1e6))
    } else {
        format!("{:>6} s", format_short(ns / 1e9))
    }
}

fn report_change(estimates: &Estimates) {
    for &statistic in [Mean, Median].iter() {
        let estimate = estimates.get(statistic);
        let p = format_change(estimate.point_estimate(), true);
        let ci = estimate.confidence_interval();
        let lb = format_change(ci.lower_bound(), true);
        let ub = format_change(ci.upper_bound(), true);
        let se = format_change(estimate.standard_error(), false);
        let cl = ci.confidence_level();

        println!("  > {:<7} {} ± {} [{} {}] {}% CI", statistic, p, se, lb, ub, cl * 100.0);
    }
}

fn format_change(pct: f64, signed: bool) -> String {
    if signed {
        format!("{:>+6}%", format_signed_short(pct * 1e2))
    } else {
        format!("{:>6}%", format_short(pct * 1e2))
    }
}

enum ComparisonResult {
    Improved,
    Regressed,
    WithinNoise,
}

fn compare_to_noise(estimate: &Estimate, noise: f64) -> ComparisonResult {
    let ci = estimate.confidence_interval();
    let lb = ci.lower_bound();
    let ub = ci.upper_bound();

    if lb < -noise && ub < -noise {
        Improved
    } else if lb > noise && ub > noise {
        Regressed
    } else {
        WithinNoise
    }
}
