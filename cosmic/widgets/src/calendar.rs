//! Monday-first month grid on top of ember-core's weekday/days_in.
use ember_core::calendar::{days_in, weekday};

/// Six Monday-first weeks; days outside the month are None.
pub fn grid(y: i32, m: u32) -> [[Option<u32>; 7]; 6] {
    let mut g = [[None; 7]; 6];
    let start = ((weekday(y, m, 1) + 6) % 7) as usize;
    for d in 1..=days_in(y, m) {
        let i = start + d as usize - 1;
        g[i / 7][i % 7] = Some(d);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nov_2026_starts_sunday() {
        let g = grid(2026, 11); // 1 Nov 2026 is a Sunday -> last column of row 0
        assert_eq!(g[0][..6], [None; 6]);
        assert_eq!(g[0][6], Some(1));
        assert_eq!(g[1][0], Some(2));
        assert_eq!(g[5][0], Some(30));
    }

    #[test]
    fn feb_lengths_and_monday_start() {
        let flat = |y| grid(y, 2).iter().flatten().flatten().count();
        assert_eq!(flat(2027), 28);
        assert_eq!(flat(2028), 29);
        assert_eq!(grid(2024, 1)[0][0], Some(1)); // 1 Jan 2024 is a Monday
    }
}
