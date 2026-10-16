//! The iroh spike of riff 3.0 (#697): two riff daemons talk over iroh
//! with the defaults of n0 (decision 21 of #699).
//!
//! The spike is a small program, not riff code. `README.md` says how
//! to run it, and gives the result.

pub mod record;

use std::time::Duration;

/// The least, the median and the most of some times.
///
/// ```
/// use std::time::Duration;
/// use iroh_spike::spread;
///
/// let ms = |n| Duration::from_millis(n);
/// assert_eq!(spread(&[ms(30), ms(10), ms(20)]), Some((ms(10), ms(20), ms(30))));
/// assert_eq!(spread(&[]), None);
/// ```
pub fn spread(times: &[Duration]) -> Option<(Duration, Duration, Duration)> {
    let mut sorted = times.to_vec();
    sorted.sort();
    let first = *sorted.first()?;
    let last = *sorted.last()?;
    let middle = sorted.len() / 2;
    let median = if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        (sorted[middle - 1] + sorted[middle]) / 2
    };
    Some((first, median, last))
}

/// A time in milliseconds, for a line of the output.
///
/// ```
/// use std::time::Duration;
/// assert_eq!(iroh_spike::ms(Duration::from_micros(12_345)), "12.3 ms");
/// ```
pub fn ms(time: Duration) -> String {
    format!("{:.1} ms", time.as_secs_f64() * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_median_of_an_even_count_is_the_mean_of_the_two_middle_times() {
        let ms = Duration::from_millis;
        assert_eq!(
            spread(&[ms(40), ms(10), ms(20), ms(30)]),
            Some((ms(10), ms(25), ms(40)))
        );
        assert_eq!(spread(&[ms(5)]), Some((ms(5), ms(5), ms(5))));
    }
}
