use time::precise_time_ns;

use bencher::Bencher;
use criterion::Criterion;
use sample::Sample;

pub struct Clock {
    cost: f64,
}

impl Clock {
    pub fn cost(&self) -> f64 {
        self.cost as f64
    }

    pub fn new(criterion: &Criterion) -> Clock {
        println!("estimating the cost of precise_time_ns()");

        let sample = Sample::new(clock_cost, criterion);

        sample.outliers().report();

        let sample = sample.without_outliers();

        sample.estimate(criterion);

        Clock {
            cost: sample.median(),
        }
    }
}

fn clock_cost(b: &mut Bencher) {
    b.iter(|| precise_time_ns())
}
