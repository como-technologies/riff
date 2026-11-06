# The image of riff-server (R135). It holds only the binary and the CA
# certificates, and runs as a user that is not root. `just deploy`
# builds it with Cloud Build.
FROM rust:1.95-alpine AS build
RUN apk add --no-cache build-base cmake perl ca-certificates
WORKDIR /src
COPY . .
# The build has no git. It names its build from the build arguments,
# or from build-id.env, which `riff cloud deploy` writes
# (01M3JEE7YXQPWS65FBVTASAEBX).
ARG RIFF_BUILD=
ARG RIFF_BUILD_TIME=
# On Alpine, Rust links the binary statically, with musl.
RUN if [ -z "$RIFF_BUILD" ] && [ -f build-id.env ]; then set -a; . ./build-id.env; set +a; fi; \
    RIFF_BUILD="$RIFF_BUILD" RIFF_BUILD_TIME="$RIFF_BUILD_TIME" \
    cargo build --locked --release -p riff-server

FROM scratch
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /src/target/release/riff-server /riff-server
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
    RIFF_LISTEN=0.0.0.0:8080
USER 65534:65534
EXPOSE 8080
ENTRYPOINT ["/riff-server"]
