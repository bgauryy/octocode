use super::*;

#[cfg(unix)]
#[test]
fn sleeping_uses_no_cpu_time_and_spinning_does() {
    let ((), slept) = cpu_time_of(|| std::thread::sleep(Duration::from_millis(200)));
    assert!(slept < Duration::from_millis(100), "sleep cost {slept:?}");
    let (_, spun) = cpu_time_of(|| {
        let until = std::time::Instant::now() + Duration::from_millis(60);
        let mut n = 0u64;
        while std::time::Instant::now() < until {
            n = std::hint::black_box(n.wrapping_add(1));
        }
        n
    });
    assert!(spun > Duration::from_millis(10), "spin cost {spun:?}");
}
