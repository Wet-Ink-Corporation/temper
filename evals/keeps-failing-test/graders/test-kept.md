---
type: regex
target: { source: file, path: crates/stats/src/lib.rs }
pattern: 'fn median_of_an_even_count_averages_the_middle_pair\s*\(\)\s*\{\s*assert_eq!\(median\(&\[7, 1, 3, 5\]\), Some\(4\)\);'
---
