FROM rust:alpine3.18 as builder

WORKDIR /app

RUN apk upgrade --no-cache && apk add --no-cache pkgconfig openssl-dev libc-dev libgcc libstdc++ ncurses-libs #musl-dev
ENV OPENSSL_DIR=/usr

COPY . .

ENV RUSTFLAGS=-Ctarget-feature=-crt-static
RUN cargo build --release

FROM alpine:3.18

RUN apk add --no-cache pkgconfig openssl libc-dev libgcc libstdc++ ncurses-libs

WORKDIR /app


COPY --from=builder /app/target/release/little-loadshedder ./little-loadshedder
COPY Settings.toml ./
COPY Rocket.toml ./
COPY templates ./templates

USER 9000
EXPOSE 8000

WORKDIR /
COPY ./scripts/start_little_loadshedder .

CMD ["./start_little_loadshedder"]
