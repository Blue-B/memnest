use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use memnest::index::{TextIndex, VectorIndex};
use rand::Rng;
use tempfile::TempDir;

fn random_vector(dim: usize) -> Vec<f32> {
    let mut rng = rand::rng();
    let mut v: Vec<f32> = (0..dim).map(|_| rng.random::<f32>() - 0.5).collect();
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

fn recall_at_k(ground_truth: &[Vec<String>], results: &[Vec<String>], k: usize) -> f64 {
    let mut total = 0.0;
    for (gt, res) in ground_truth.iter().zip(results) {
        let gt_k: std::collections::HashSet<_> = gt.iter().take(k).cloned().collect();
        let res_k: std::collections::HashSet<_> = res.iter().take(k).cloned().collect();
        let hits = gt_k.intersection(&res_k).count() as f64;
        total += hits / gt_k.len().max(1) as f64;
    }
    total / ground_truth.len() as f64
}

fn bench_vector_recall(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_recall");
    let dim = 768;
    let sizes = vec![100, 1_000, 10_000];

    for size in sizes {
        let temp = TempDir::new().unwrap();
        let mut index = VectorIndex::new(temp.path()).unwrap();
        let mut ids = Vec::with_capacity(size);

        for i in 0..size {
            let id = format!("vec_{}", i);
            let vec = random_vector(dim);
            index.add(&id, &vec).unwrap();
            ids.push((id, vec));
        }

        // Exact brute-force ground truth for first 50 queries
        let query_count = 50.min(size);
        let mut ground_truth = Vec::with_capacity(query_count);
        for i in 0..query_count {
            let query = &ids[i].1;
            let mut scored: Vec<(String, f32)> = ids
                .iter()
                .map(|(id, vec)| {
                    let dot = query
                        .iter()
                        .zip(vec.iter())
                        .map(|(a, b)| a * b)
                        .sum::<f32>();
                    // Cosine distance = 1 - dot (since normalized)
                    (id.clone(), 1.0 - dot)
                })
                .collect();
            scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            ground_truth.push(scored.into_iter().map(|(id, _)| id).collect::<Vec<_>>());
        }

        group.bench_with_input(
            BenchmarkId::new("hnsw_recall@10", size),
            &(&ids, &ground_truth, &index),
            |b, (ids, gt, idx)| {
                b.iter(|| {
                    let mut results = Vec::with_capacity(query_count);
                    for i in 0..query_count {
                        let r = idx.search(&ids[i].1, 10).unwrap();
                        results.push(r.into_iter().map(|(id, _)| id).collect::<Vec<_>>());
                    }
                    let _recall = recall_at_k(gt, &results, 10);
                });
            },
        );
    }

    group.finish();
}

fn bench_search_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_latency");
    let dim = 768;
    let size = 10_000;

    let temp = TempDir::new().unwrap();
    let mut index = VectorIndex::new(temp.path()).unwrap();
    for i in 0..size {
        let id = format!("vec_{}", i);
        index.add(&id, &random_vector(dim)).unwrap();
    }

    let query = random_vector(dim);
    group.bench_function("vector_search_k=10", |b| {
        b.iter(|| index.search(&query, 10).unwrap())
    });
    group.bench_function("vector_search_k=50", |b| {
        b.iter(|| index.search(&query, 50).unwrap())
    });
    group.finish();
}

fn bench_text_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_search");
    let temp = TempDir::new().unwrap();
    let mut index = TextIndex::new(temp.path()).unwrap();

    let docs: Vec<String> = (0..1_000)
        .map(|i| {
            format!(
                "project alpha task {}: implement feature for module {} with rust async performance",
                i,
                i % 20
            )
        })
        .collect();

    for (i, doc) in docs.iter().enumerate() {
        index
            .add_with_project(&format!("doc_{}", i), "alpha", doc)
            .unwrap();
    }

    group.bench_function("bm25_k=10", |b| {
        b.iter(|| index.search("rust async performance", 10).unwrap())
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_vector_recall,
    bench_search_latency,
    bench_text_search
);
criterion_main!(benches);
