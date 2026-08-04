// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

pub fn keep_row(
    last_kept: Option<&BTreeMap<u32, f64>>,
    current: &BTreeMap<u32, f64>,
    threshold: f64,
) -> bool {
    let Some(last_kept) = last_kept else {
        return true;
    };

    let mut last_iter = last_kept.iter().peekable();
    let mut current_iter = current.iter().peekable();

    loop {
        match (last_iter.peek(), current_iter.peek()) {
            (Some((last_id, last_value)), Some((current_id, current_value))) => {
                if last_id == current_id {
                    if (*current_value - *last_value).abs() >= threshold {
                        return true;
                    }
                    last_iter.next();
                    current_iter.next();
                } else if last_id < current_id {
                    if last_value.abs() >= threshold {
                        return true;
                    }
                    last_iter.next();
                } else {
                    if current_value.abs() >= threshold {
                        return true;
                    }
                    current_iter.next();
                }
            }
            (Some((_last_id, last_value)), None) => {
                if last_value.abs() >= threshold {
                    return true;
                }
                last_iter.next();
            }
            (None, Some((_current_id, current_value))) => {
                if current_value.abs() >= threshold {
                    return true;
                }
                current_iter.next();
            }
            (None, None) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_row_is_kept() {
        assert!(keep_row(None, &BTreeMap::from([(1, 10.0)]), 100.0));
    }

    #[test]
    fn below_threshold_is_omitted() {
        assert!(!keep_row(
            Some(&BTreeMap::from([(1, 10.0)])),
            &BTreeMap::from([(1, 19.0)]),
            10.0,
        ));
    }

    #[test]
    fn at_threshold_is_kept() {
        assert!(keep_row(
            Some(&BTreeMap::from([(1, 10.0)])),
            &BTreeMap::from([(1, 20.0)]),
            10.0,
        ));
    }

    #[test]
    fn negative_delta_is_absolute() {
        assert!(keep_row(
            Some(&BTreeMap::from([(1, 20.0)])),
            &BTreeMap::from([(1, 9.0)]),
            10.0,
        ));
    }

    #[test]
    fn new_metric_id_is_kept() {
        assert!(keep_row(
            Some(&BTreeMap::from([(1, 1.0)])),
            &BTreeMap::from([(1, 1.0), (2, 10.0)]),
            10.0,
        ));
    }

    #[test]
    fn disappeared_metric_is_zero() {
        assert!(keep_row(
            Some(&BTreeMap::from([(1, 10.0), (2, 2.0)])),
            &BTreeMap::from([(2, 2.0)]),
            10.0,
        ));
    }
}
