#[cfg(not(target_arch = "wasm32"))]
pub fn threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 32)
}

#[cfg(target_arch = "wasm32")]
pub fn threads() -> usize {
    1
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_all<F: FnOnce() + Send>(jobs: Vec<F>) {
    std::thread::scope(|s| {
        for job in jobs {
            s.spawn(job);
        }
    });
}

#[cfg(target_arch = "wasm32")]
pub fn run_all<F: FnOnce() + Send>(jobs: Vec<F>) {
    for job in jobs {
        job();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_all_collect<R: Send, F: FnOnce() -> R + Send>(jobs: Vec<F>) -> Vec<R> {
    std::thread::scope(|s| {
        let handles: Vec<_> = jobs.into_iter().map(|job| s.spawn(job)).collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("band worker"))
            .collect()
    })
}

#[cfg(target_arch = "wasm32")]
pub fn run_all_collect<R: Send, F: FnOnce() -> R + Send>(jobs: Vec<F>) -> Vec<R> {
    jobs.into_iter().map(|job| job()).collect()
}
