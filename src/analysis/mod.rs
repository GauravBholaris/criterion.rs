use std::path::Path;

use crate::stats::bivariate::regression::Slope;
use crate::stats::bivariate::Data;
use crate::stats::univariate::outliers::tukey::{self, LabeledSample};
use crate::stats::univariate::Sample;
use crate::stats::{Distribution, Tails};

use crate::benchmark::BenchmarkConfig;
use crate::connection::{OutgoingMessage, SamplingMethod};
use crate::estimate::{
    build_estimates, ConfidenceInterval, Distributions, Estimate, Estimates, PointEstimates,
};
use crate::fs;
use crate::measurement::Measurement;
use crate::report::{BenchmarkId, ReportContext};
use crate::routine::Routine;
use crate::{Baseline, Criterion, Throughput};

macro_rules! elapsed {
    ($msg:expr, $block:expr) => {{
        let start = ::std::time::Instant::now();
        let out = $block;
        let elapsed = &start.elapsed();

        info!(
            "{} took {}",
            $msg,
            crate::format::time(crate::DurationExt::to_nanos(elapsed) as f64)
        );

        out
    }};
}

mod compare;

// Common analysis procedure
pub(crate) fn common<M: Measurement, T: ?Sized>(
    id: &BenchmarkId,
    routine: &mut dyn Routine<M, T>,
    config: &BenchmarkConfig,
    criterion: &Criterion<M>,
    report_context: &ReportContext,
    parameter: &T,
    throughput: Option<Throughput>,
) {
    if criterion.list_mode {
        println!("{}: bench", id);
        return;
    }
    criterion.report.benchmark_start(id, report_context);

    // In test mode, run the benchmark exactly once, then exit.
    if criterion.test_mode {
        routine.test(&criterion.measurement, parameter);
        criterion.report.terminated(id, report_context);
        return;
    }

    if let Baseline::Compare = criterion.baseline {
        if !base_dir_exists(
            id,
            &criterion.baseline_directory,
            &criterion.output_directory,
        ) {
            panic!(format!(
                "Baseline '{base}' must exist before comparison is allowed; try --save-baseline {base}",
                base=criterion.baseline_directory,
            ));
        }
    }

    // In profiling mode, skip all of the analysis.
    if let Some(time) = criterion.profile_time {
        routine.profile(
            &criterion.measurement,
            id,
            criterion,
            report_context,
            time,
            parameter,
        );
        return;
    }

    let (iters, times);
    if let Some(baseline) = &criterion.load_baseline {
        let mut sample_path = criterion.output_directory.clone();
        sample_path.push(id.as_directory_name());
        sample_path.push(baseline);
        sample_path.push("sample.json");
        let loaded = fs::load::<(Box<[f64]>, Box<[f64]>), _>(&sample_path);

        match loaded {
            Err(err) => panic!(
                "Baseline '{base}' must exist before it can be loaded; try --save-baseline {base}. Error: {err}",
                base = baseline, err = err
            ),
            Ok(samples) => {
                iters = samples.0;
                times = samples.1;
            }
        }
    } else {
        let sample = routine.sample(
            &criterion.measurement,
            id,
            config,
            criterion,
            report_context,
            parameter,
        );
        iters = sample.0;
        times = sample.1;

        if let Some(conn) = &criterion.connection {
            conn.send(&OutgoingMessage::MeasurementComplete {
                id: id.into(),
                iters: &iters,
                times: &times,
                plot_config: (&report_context.plot_config).into(),
                sampling_method: SamplingMethod::Linear,
                benchmark_config: config.into(),
            })
            .unwrap();

            conn.serve_value_formatter(criterion.measurement.formatter())
                .unwrap();
        }
    }

    criterion.report.analysis(id, report_context);

    let avg_times = iters
        .iter()
        .zip(times.iter())
        .map(|(&iters, &elapsed)| elapsed / iters)
        .collect::<Vec<f64>>();
    let avg_times = Sample::new(&avg_times);

    if criterion.load_baseline.is_none() {
        log_if_err!({
            let mut new_dir = criterion.output_directory.clone();
            new_dir.push(id.as_directory_name());
            new_dir.push("new");
            fs::mkdirp(&new_dir)
        });
    }

    let data = Data::new(&iters, &times);
    let labeled_sample = outliers(id, &criterion.output_directory, avg_times);
    let (distribution, slope) = regression(&data, config);
    let (mut distributions, mut estimates) = estimates(avg_times, config);

    estimates.slope = slope;
    distributions.slope = distribution;

    if criterion.load_baseline.is_none() {
        log_if_err!({
            let mut sample_file = criterion.output_directory.clone();
            sample_file.push(id.as_directory_name());
            sample_file.push("new");
            sample_file.push("sample.json");
            fs::save(&(data.x().as_ref(), data.y().as_ref()), &sample_file)
        });
        log_if_err!({
            let mut estimates_file = criterion.output_directory.clone();
            estimates_file.push(id.as_directory_name());
            estimates_file.push("new");
            estimates_file.push("estimates.json");
            fs::save(&estimates, &estimates_file)
        });
    }

    let compare_data = if base_dir_exists(
        id,
        &criterion.baseline_directory,
        &criterion.output_directory,
    ) {
        let result = compare::common(id, avg_times, config, criterion);
        match result {
            Ok((
                t_value,
                t_distribution,
                relative_estimates,
                relative_distributions,
                base_iter_counts,
                base_sample_times,
                base_avg_times,
                base_estimates,
            )) => {
                let p_value = t_distribution.p_value(t_value, &Tails::Two);
                Some(crate::report::ComparisonData {
                    p_value,
                    t_distribution,
                    t_value,
                    relative_estimates,
                    relative_distributions,
                    significance_threshold: config.significance_level,
                    noise_threshold: config.noise_threshold,
                    base_iter_counts,
                    base_sample_times,
                    base_avg_times,
                    base_estimates,
                })
            }
            Err(e) => {
                crate::error::log_error(&e);
                None
            }
        }
    } else {
        None
    };

    let measurement_data = crate::report::MeasurementData {
        data: Data::new(&*iters, &*times),
        avg_times: labeled_sample,
        absolute_estimates: estimates,
        distributions,
        comparison: compare_data,
        throughput,
    };

    criterion.report.measurement_complete(
        id,
        report_context,
        &measurement_data,
        criterion.measurement.formatter(),
    );

    if criterion.load_baseline.is_none() {
        log_if_err!({
            let mut benchmark_file = criterion.output_directory.clone();
            benchmark_file.push(id.as_directory_name());
            benchmark_file.push("new");
            benchmark_file.push("benchmark.json");
            fs::save(&id, &benchmark_file)
        });
    }

    if let Baseline::Save = criterion.baseline {
        copy_new_dir_to_base(
            id.as_directory_name(),
            &criterion.baseline_directory,
            &criterion.output_directory,
        );
    }
}

fn base_dir_exists(id: &BenchmarkId, baseline: &str, output_directory: &Path) -> bool {
    let mut base_dir = output_directory.to_owned();
    base_dir.push(id.as_directory_name());
    base_dir.push(baseline);
    base_dir.exists()
}

// Performs a simple linear regression on the sample
fn regression(
    data: &Data<'_, f64, f64>,
    config: &BenchmarkConfig,
) -> (Distribution<f64>, Estimate) {
    let cl = config.confidence_level;

    let distribution = elapsed!(
        "Bootstrapped linear regression",
        data.bootstrap(config.nresamples, |d| (Slope::fit(&d).0,))
    )
    .0;

    let point = Slope::fit(&data);
    let (lb, ub) = distribution.confidence_interval(config.confidence_level);
    let se = distribution.std_dev(None);

    (
        distribution,
        Estimate {
            confidence_interval: ConfidenceInterval {
                confidence_level: cl,
                lower_bound: lb,
                upper_bound: ub,
            },
            point_estimate: point.0,
            standard_error: se,
        },
    )
}

// Classifies the outliers in the sample
fn outliers<'a>(
    id: &BenchmarkId,
    output_directory: &Path,
    avg_times: &'a Sample<f64>,
) -> LabeledSample<'a, f64> {
    let sample = tukey::classify(avg_times);
    log_if_err!({
        let mut tukey_file = output_directory.to_owned();
        tukey_file.push(id.as_directory_name());
        tukey_file.push("new");
        tukey_file.push("tukey.json");
        fs::save(&sample.fences(), &tukey_file)
    });
    sample
}

// Estimates the statistics of the population from the sample
fn estimates(avg_times: &Sample<f64>, config: &BenchmarkConfig) -> (Distributions, Estimates) {
    fn stats(sample: &Sample<f64>) -> (f64, f64, f64, f64) {
        let mean = sample.mean();
        let std_dev = sample.std_dev(Some(mean));
        let median = sample.percentiles().median();
        let mad = sample.median_abs_dev(Some(median));

        (mean, std_dev, median, mad)
    }

    let cl = config.confidence_level;
    let nresamples = config.nresamples;

    let (mean, std_dev, median, mad) = stats(avg_times);
    let points = PointEstimates {
        mean,
        median,
        std_dev,
        slope: mean,
        median_abs_dev: mad,
    };

    let (dist_mean, dist_stddev, dist_median, dist_mad) = elapsed!(
        "Bootstrapping the absolute statistics.",
        avg_times.bootstrap(nresamples, stats)
    );

    let distributions = Distributions {
        mean: dist_mean.clone(),
        slope: dist_mean,
        median: dist_median,
        median_abs_dev: dist_mad,
        std_dev: dist_stddev,
    };

    let estimates = build_estimates(&distributions, &points, cl);

    (distributions, estimates)
}

fn copy_new_dir_to_base(id: &str, baseline: &str, output_directory: &Path) {
    let root_dir = Path::new(output_directory).join(id);
    let base_dir = root_dir.join(baseline);
    let new_dir = root_dir.join("new");

    if !new_dir.exists() {
        return;
    };
    if !base_dir.exists() {
        try_else_return!(fs::mkdirp(&base_dir));
    }

    // TODO: consider using walkdir or similar to generically copy.
    try_else_return!(fs::cp(
        &new_dir.join("estimates.json"),
        &base_dir.join("estimates.json")
    ));
    try_else_return!(fs::cp(
        &new_dir.join("sample.json"),
        &base_dir.join("sample.json")
    ));
    try_else_return!(fs::cp(
        &new_dir.join("tukey.json"),
        &base_dir.join("tukey.json")
    ));
    try_else_return!(fs::cp(
        &new_dir.join("benchmark.json"),
        &base_dir.join("benchmark.json")
    ));
    try_else_return!(fs::cp(&new_dir.join("raw.csv"), &base_dir.join("raw.csv")));
}
