# Local development / CI-style workspace image for Spectra.
FROM rust:1.89-bookworm

WORKDIR /app

ENV CARGO_BUILD_JOBS=1
ENV CARGO_TARGET_DIR=/app/target

RUN apt-get update \
    && apt-get install -y --no-install-recommends ripgrep pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY . .

# Warm the dependency graph; full feature builds happen at run time as needed.
RUN cargo fetch

CMD ["bash"]
