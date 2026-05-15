FROM rust:1.82-slim-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

FROM debian:bookworm-slim
RUN sed -i 's|http://deb.debian.org|http://ftp.de.debian.org|g' \
    /etc/apt/sources.list.d/debian.sources \
    /etc/apt/sources.list 2>/dev/null; true
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/company-beacon-signal /usr/local/bin/company-beacon-signal
ENTRYPOINT ["company-beacon-signal"]
