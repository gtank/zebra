FROM rust:stretch as builder

RUN apt-get update && \
	apt-get install -y --no-install-recommends \
	make cmake g++ gcc

RUN mkdir /zebra
WORKDIR /zebra

ENV RUST_BACKTRACE 1
ENV CARGO_HOME /zebra/.cargo/

# Copy local code to the container image.
# Assumes that we are in the git repo.

COPY . .

RUN cargo fetch --verbose

COPY . .

RUN rustc -V; cargo -V; rustup -V; cargo test --all && cargo build --release


FROM debian:buster-slim
COPY --from=builder /zebra/target/release/zebrad .
ENV PORT 8233
CMD ["./zebrad", "seed"]
