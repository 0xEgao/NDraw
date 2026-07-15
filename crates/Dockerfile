FROM rust:1.85-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY ndraw-proto ./ndraw-proto
COPY ndraw-den ./ndraw-den
COPY ndraw-server ./ndraw-server
COPY ndraw-test ./ndraw-test
RUN cargo build --locked --release -p ndraw-server

FROM debian:bookworm-slim AS runtime

RUN useradd --system --uid 10001 --create-home ndraw
COPY --from=builder /build/target/release/ndraw-server /usr/local/bin/ndraw-server

ENV NDRAW_BIND=0.0.0.0:3000
EXPOSE 3000
USER ndraw
ENTRYPOINT ["/usr/local/bin/ndraw-server"]
