//! BM-S* experiment registry.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExperimentTrack {
    Scenario,
    Write,
    Query,
}

pub struct ExperimentMeta {
    pub id: &'static str,
    pub summary: &'static str,
    pub track: ExperimentTrack,
}

pub const REGISTRY: &[ExperimentMeta] = &[
    // Track C — smoke regression
    ExperimentMeta {
        id: "bm-s0",
        summary: "emit-only mem embedded baseline latency",
        track: ExperimentTrack::Scenario,
    },
    ExperimentMeta {
        id: "bm-s1",
        summary: "persist roundtrip mem (smoke counter)",
        track: ExperimentTrack::Scenario,
    },
    ExperimentMeta {
        id: "bm-s2",
        summary: "persist roundtrip sqlite (smoke counter)",
        track: ExperimentTrack::Scenario,
    },
    ExperimentMeta {
        id: "bm-s3",
        summary: "query range mem (emit batch + query)",
        track: ExperimentTrack::Scenario,
    },
    // Track A — write capacity
    ExperimentMeta {
        id: "bm-sw0",
        summary: "adapter-direct counter firehose",
        track: ExperimentTrack::Write,
    },
    ExperimentMeta {
        id: "bm-sw1",
        summary: "full-stack counter firehose",
        track: ExperimentTrack::Write,
    },
    ExperimentMeta {
        id: "bm-sw2",
        summary: "concurrency saturation counter firehose",
        track: ExperimentTrack::Write,
    },
    ExperimentMeta {
        id: "bm-sw3",
        summary: "multi-writer counter firehose (bench clients)",
        track: ExperimentTrack::Write,
    },
    ExperimentMeta {
        id: "bm-sw4",
        summary: "event append firehose",
        track: ExperimentTrack::Write,
    },
    ExperimentMeta {
        id: "bm-sw5",
        summary: "durable multi-DW counter (Spectra→DW direct / subscriber-sim)",
        track: ExperimentTrack::Write,
    },
    ExperimentMeta {
        id: "bm-sw6",
        summary: "durable multi-DW event (Spectra→DW direct / subscriber-sim)",
        track: ExperimentTrack::Write,
    },
    ExperimentMeta {
        id: "bm-sw7",
        summary: "batched durable multi-DW counter (L2 PersistConfig / *_now + flush)",
        track: ExperimentTrack::Write,
    },
    // Track B — query capacity
    ExperimentMeta {
        id: "bm-sq0",
        summary: "metric query at depth=0",
        track: ExperimentTrack::Query,
    },
    ExperimentMeta {
        id: "bm-sq1",
        summary: "metric query after prefill (depth sweep 1k-1M)",
        track: ExperimentTrack::Query,
    },
    ExperimentMeta {
        id: "bm-sq2",
        summary: "label filter query overhead",
        track: ExperimentTrack::Query,
    },
    ExperimentMeta {
        id: "bm-sq3",
        summary: "event query after prefill (depth sweep 1k-1M)",
        track: ExperimentTrack::Query,
    },
];

pub fn resolve_experiment(id: &str) -> Option<&'static ExperimentMeta> {
    let key = id.trim().to_ascii_lowercase();
    REGISTRY.iter().find(|e| e.id == key)
}
