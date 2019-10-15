use criterion::Criterion;
use criterion::Benchmark;
use std::time::Duration;

const SIZE: usize = 1024 * 1024;

fn alloc(c: &mut Criterion) {
    /*c.bench(Benchmark::new("alloc", |b| {
            b.iter_with_large_drop(|| (0..SIZE).map(|_| 0u8).collect::<Vec<_>>())
        })
        .warm_up_time(Duration::new(1, 0)));*/

    Criterion::default()
        .configure_from_args()
        .warm_up_time(Duration::new(1, 0))
        .bench_function("alloc", |b| {
            b.iter_with_large_drop(|| (0..SIZE).map(|_| 0u8).collect::<Vec<_>>())
        });
}


criterion_group!{benches, alloc}
