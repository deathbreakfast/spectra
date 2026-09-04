//! In-process counter aggregation (Lever C): sum counter deltas by canonical
//! `(name, labels)` within one emit-buffer drain window.

use std::collections::HashMap;

use crate::emit_buffer::BufferedEmit;

/// Canonical aggregation key: metric name + label pairs sorted by `(k, v)`.
#[derive(Debug, PartialEq, Eq, Hash)]
struct CounterKey {
    name: String,
    labels: Vec<(String, String)>,
}

impl CounterKey {
    fn new(name: &str, labels: &[(String, String)]) -> Self {
        let mut labels = labels.to_vec();
        labels.sort_by(|(ak, av), (bk, bv)| ak.cmp(bk).then(av.cmp(bv)));
        Self {
            name: name.to_string(),
            labels,
        }
    }
}

/// Sum buffered counter deltas by canonical key; gauges and events pass through
/// in first-seen order. Returns the collapsed records and how many counter emits
/// were merged into an existing key.
pub fn accumulate_counters(records: Vec<BufferedEmit>) -> (Vec<BufferedEmit>, u64) {
    let mut out: Vec<BufferedEmit> = Vec::with_capacity(records.len());
    let mut index: HashMap<CounterKey, usize> = HashMap::new();
    let mut coalesced = 0u64;

    for rec in records {
        match rec {
            BufferedEmit::Counter {
                name,
                labels,
                delta,
                ts,
            } => {
                let key = CounterKey::new(&name, &labels);
                if let Some(&pos) = index.get(&key) {
                    if let BufferedEmit::Counter { delta: d, .. } = &mut out[pos] {
                        *d += delta;
                        coalesced += 1;
                    }
                } else {
                    index.insert(key, out.len());
                    out.push(BufferedEmit::Counter {
                        name,
                        labels,
                        delta,
                        ts,
                    });
                }
            }
            other => out.push(other),
        }
    }

    (out, coalesced)
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;

    fn ts() -> DateTime<Utc> {
        Utc::now()
    }

    fn counter(name: &str, labels: &[(&str, &str)], delta: i64, ts: DateTime<Utc>) -> BufferedEmit {
        BufferedEmit::Counter {
            name: name.into(),
            labels: labels
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            delta,
            ts,
        }
    }

    fn gauge(name: &str, value: f64, ts: DateTime<Utc>) -> BufferedEmit {
        BufferedEmit::Gauge {
            name: name.into(),
            labels: vec![],
            value,
            ts,
        }
    }

    fn event(table: &str, ts: DateTime<Utc>) -> BufferedEmit {
        BufferedEmit::Event {
            table: table.into(),
            fields: serde_json::json!({}),
            ts,
        }
    }

    #[test]
    fn lossless_sum_collapses_same_key() {
        let t = ts();
        let (out, coalesced) =
            accumulate_counters(vec![counter("c", &[], 3, t), counter("c", &[], 5, t)]);
        assert_eq!(coalesced, 1);
        assert_eq!(out.len(), 1);
        match &out[0] {
            BufferedEmit::Counter { name, delta, .. } => {
                assert_eq!(name, "c");
                assert_eq!(*delta, 8);
            }
            _ => panic!("expected counter"),
        }
    }

    #[test]
    fn canonical_key_sorts_labels() {
        let t = ts();
        let (out, coalesced) = accumulate_counters(vec![
            counter("c", &[("a", "1"), ("b", "2")], 1, t),
            counter("c", &[("b", "2"), ("a", "1")], 1, t),
        ]);
        assert_eq!(coalesced, 1);
        assert_eq!(out.len(), 1);
        match &out[0] {
            BufferedEmit::Counter { delta, .. } => assert_eq!(*delta, 2),
            _ => panic!("expected counter"),
        }
    }

    #[test]
    fn distinct_keys_preserved_in_first_seen_order() {
        let t = ts();
        let (out, coalesced) = accumulate_counters(vec![
            counter("a", &[], 1, t),
            counter("b", &[], 1, t),
            counter("a", &[("k", "v")], 1, t),
        ]);
        assert_eq!(coalesced, 0);
        assert_eq!(out.len(), 3);
        match (&out[0], &out[1], &out[2]) {
            (
                BufferedEmit::Counter { name: n0, .. },
                BufferedEmit::Counter { name: n1, .. },
                BufferedEmit::Counter { name: n2, .. },
            ) => {
                assert_eq!(n0, "a");
                assert_eq!(n1, "b");
                assert_eq!(n2, "a");
            }
            _ => panic!("expected counters"),
        }
    }

    #[test]
    fn pass_through_preserves_gauge_and_event_order() {
        let t = ts();
        let t2 = t + chrono::Duration::milliseconds(1);
        let (out, coalesced) = accumulate_counters(vec![
            counter("c", &[], 1, t),
            gauge("g", 1.5, t),
            event("e", t2),
            counter("c", &[], 2, t2),
        ]);
        assert_eq!(coalesced, 1);
        assert_eq!(out.len(), 3);
        match (&out[0], &out[1], &out[2]) {
            (
                BufferedEmit::Counter { delta, .. },
                BufferedEmit::Gauge { name, value, .. },
                BufferedEmit::Event { table, .. },
            ) => {
                assert_eq!(*delta, 3);
                assert_eq!(name, "g");
                assert!((*value - 1.5).abs() < f64::EPSILON);
                assert_eq!(table, "e");
            }
            _ => panic!("expected counter, gauge, event"),
        }
    }

    #[test]
    fn negative_deltas_sum_arithmetically() {
        let t = ts();
        let (out, coalesced) =
            accumulate_counters(vec![counter("c", &[], 5, t), counter("c", &[], -2, t)]);
        assert_eq!(coalesced, 1);
        match &out[0] {
            BufferedEmit::Counter { delta, .. } => assert_eq!(*delta, 3),
            _ => panic!("expected counter"),
        }
    }
}
