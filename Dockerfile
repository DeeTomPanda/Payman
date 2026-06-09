# Build first
FROM rust:1.96 AS builder

WORKDIR /usr/src/app
COPY . .

ENV SQLX_OFFLINE=true

RUN cargo build --release 

# then build the final image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y libssl3 ca-certificates curl && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/local/bin

# copy files from builder stage
COPY --from=builder /usr/src/app/target/release/Payman .
COPY --from=builder /usr/src/app/target/release/psp .
