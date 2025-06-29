use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use rand::{rng, Rng};

// The function to benchmark
use secretmangle::arbitrary::xor_intrinsic::xor_chunks_intrinsic_baseline;

fn generate_random_data<const N: usize>() -> [u8; N] {
    let mut rng = rng();
    std::array::from_fn(|_| rng.random())
}

fn xor_chunks_initialized<const N: usize>(data: &mut [u8; N], key: &[u8; N]) {
    for i in 0..N {
        data[i] ^= key[i];
    }
}

fn xor_chunks_slices(data: &mut [u8], key: &[u8]) {
    for (d, k) in data.iter_mut().zip(key.iter()) {
        *d ^= *k;
    }
}

fn internal_bench_full<const N: usize>(c: &mut Criterion) {
    let mut data: [u8; N] = generate_random_data();
    let key: [u8; N] = generate_random_data();
    
    let data_ptr = data.as_mut_ptr();
    let key_ptr = key.as_ptr();
    
    let mut group = c.benchmark_group(format!("xor_chunks_{}b", N));
    group.throughput(Throughput::Bytes(N as u64));
    
    group.bench_function("intrinsic_baseline", |b| {
        b.iter(|| {
            let data = black_box(data_ptr);
            let key = black_box(key_ptr);
            
            // - data and key are properly allocated
            // - the required alignment for [u8; N] is 1, which is satisfied
            // - data and key are non-overlapping
            unsafe {
                xor_chunks_intrinsic_baseline::<[u8; N]>(data, key);
            }
            
            black_box(data);
        });
    });

    group.bench_function("initialized", |b| {
        b.iter(|| {
            let data_ref = black_box(&mut data);
            let key_ref = black_box(&key);
            
            xor_chunks_initialized(data_ref, key_ref);
            
            black_box(data_ref);
        });
    });

    group.bench_function("slices_aligned", |b| {
        b.iter(|| {
            let data_ref = black_box(&mut data);
            let key_ref = black_box(&key);
            
            xor_chunks_slices(data_ref, key_ref);
            
            black_box(data_ref);
        });
    });
    
    group.finish();
}



// Benchmark for different data sizes
fn bench_xor_chunks(c: &mut Criterion) {
    internal_bench_full::<1>(c);
    internal_bench_full::<16>(c);
    internal_bench_full::<64>(c);
    internal_bench_full::<256>(c);
    internal_bench_full::<1024>(c);
    internal_bench_full::<4096>(c);
    internal_bench_full::<16384>(c);
}


fn internal_bench_unaligned_same<const N: usize, const F: usize>(
    c: &mut Criterion
) {
    let offset = F.checked_sub(N).unwrap();

    let mut data: [u8; F] = generate_random_data();
    let key: [u8; F] = generate_random_data();
    
    let data_ptr = data.as_mut_ptr().wrapping_add(offset);
    let key_ptr = key.as_ptr().wrapping_add(offset);
    
    let mut group = c.benchmark_group(format!("xor_chunks_{}b_from_{}unaligned", N, offset));
    group.throughput(Throughput::Bytes(N as u64));
    
    group.bench_function("intrinsic_baseline_unaligned", |b| {
        b.iter(|| {
            let data = black_box(data_ptr);
            let key = black_box(key_ptr);
            
            // - data and key are properly allocated
            // - the required alignment for [u8; N] is 1, which is satisfied
            // - data and key are non-overlapping
            unsafe {
                xor_chunks_intrinsic_baseline::<[u8; N]>(data, key);
            }
            
            black_box(data);
        });
    });

    group.bench_function("slices_unaligned", |b| {
        b.iter(|| {
            let data_ref = black_box(&mut data[offset..]);
            let key_ref = black_box(&key[offset..]);
            
            xor_chunks_slices(data_ref, key_ref);
            
            black_box(data_ref);
        });
    });
    
    group.finish();
}


fn bench_xor_chunks_unaligned(c: &mut Criterion) {
    internal_bench_unaligned_same::<1, 3>(c);
    internal_bench_unaligned_same::<16, 19>(c);
    internal_bench_unaligned_same::<64, 67>(c);
    internal_bench_unaligned_same::<256, 259>(c);
    internal_bench_unaligned_same::<1024, 1027>(c);
    internal_bench_unaligned_same::<4096, 4099>(c);
    internal_bench_unaligned_same::<16384, 16387>(c);

    internal_bench_unaligned_same::<64, 68>(c);
    internal_bench_unaligned_same::<256, 260>(c);
    internal_bench_unaligned_same::<1024, 1028>(c);
    internal_bench_unaligned_same::<4096, 4100>(c);
    internal_bench_unaligned_same::<16384, 16388>(c);

    internal_bench_unaligned_same::<64, 96>(c);
    internal_bench_unaligned_same::<256, 288>(c);
    internal_bench_unaligned_same::<1024, 1056>(c);
    internal_bench_unaligned_same::<4096, 4128>(c);
    internal_bench_unaligned_same::<16384, 16416>(c);
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(std::time::Duration::from_millis(500))
        .measurement_time(std::time::Duration::from_secs(1))
        .sample_size(800);
    targets = bench_xor_chunks, bench_xor_chunks_unaligned
);

criterion_main!(benches);
