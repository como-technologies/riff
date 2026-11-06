# The image of riff-server of a release or of a build of CI
# (01M6QQ1PB372MQ7M527HVT323F). It holds the static riff-server that CI
# built, and the CA certificates. It compiles nothing: `deploy/cloud/image.sh`
# stages the binary as `riff-server` in the context, and checks that the
# image holds the same file. It runs as a user that is not root.
#
# The Dockerfile in the root of the repository builds riff-server from the
# source, for `riff cloud deploy NAME` with no tag.
FROM docker.io/library/alpine:3 AS certs
RUN apk add --no-cache ca-certificates

FROM scratch
COPY --from=certs /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY riff-server /riff-server
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
    RIFF_LISTEN=0.0.0.0:8080
USER 65534:65534
EXPOSE 8080
ENTRYPOINT ["/riff-server"]
