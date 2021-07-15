FROM ekidd/rust-musl-builder:latest as builder

RUN USER=rust cargo new starbot
WORKDIR ./starbot

ADD --chown=rust:rust . ./
RUN cargo build --release

FROM alpine:latest
RUN apk --no-cache add ca-certificates
COPY --from=builder /home/rust/src/starbot/target/x86_64-unknown-linux-musl/release/starbot /usr/local/bin/starbot

ENTRYPOINT starbot