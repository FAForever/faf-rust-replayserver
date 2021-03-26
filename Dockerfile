FROM rust:1.50.0 AS builder

WORKDIR /server
ADD . /server
RUN cargo build

FROM gcr.io/distroless/cc
LABEL maintainer="ikotrasinsk@gmail.com"
LABEL description="Forged Alliance Forever replay server, rust flavour"

COPY --from=builder /server/target/debug/faf_rust_replayserver /
CMD ["./faf_rust_replayserver"]
