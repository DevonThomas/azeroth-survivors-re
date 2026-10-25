# The dedicated co-op server, as a small image. Build from the repository root:
#
#   docker build -f deploy/Dockerfile -t azeroth-survivors-server .
#   docker run -d --restart unless-stopped -p 7777:7777/udp azeroth-survivors-server
#
# It needs no game data: only the survivors-sim, survivors-net and survivors-server crates build.

FROM rust:1.98-slim-bookworm AS build
WORKDIR /src
# The whole workspace's manifests resolve; only the server's dependency graph compiles.
COPY . .
RUN cargo build --release --locked -p survivors-server --bin survivors-server \
 && strip target/release/survivors-server

FROM debian:bookworm-slim
RUN useradd --system --no-create-home survivors
COPY --from=build /src/target/release/survivors-server /usr/local/bin/survivors-server
USER survivors
EXPOSE 7777/udp
ENV AZS_BIND=0.0.0.0:7777
ENTRYPOINT ["/usr/local/bin/survivors-server"]
