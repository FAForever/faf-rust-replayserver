FROM rust:1.56.1 AS builder

WORKDIR /server
ADD . /server
RUN env SQLX_OFFLINE=true cargo build --release

FROM gcr.io/distroless/cc
LABEL maintainer="ikotrasinsk@gmail.com"
LABEL description="Forged Alliance Forever replay server, rust flavour"

COPY --from=builder /server/target/release/faf_rust_replayserver /
CMD ["./faf_rust_replayserver"]
