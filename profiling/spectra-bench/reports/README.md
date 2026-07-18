# Spectra bench reports

Decision-grade JSON from AWS campaigns only (`SPECTRA_BENCH_HARDWARE=aws-*`).

Naming: `{experiment}-{storage}-{topology}-{hardware}.json` (or `multidw-*` for multi-DW / BM-SW7).

Do not commit non-AWS / local / WSL smoke JSON. Fetch from EC2 with `infra/aws/spectra/fetch-reports.sh` or `infra/aws/spectra-multidw/fetch-reports.sh`.

Scoreboards: [`docs/bench/PERFORMANCE_STUDY.md`](../../../docs/bench/PERFORMANCE_STUDY.md).
