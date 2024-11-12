#!/usr/bin/env sh
docker pull alpine:latest
docker-compose build
docker push c0d3m4513r1/rust-dc-bot:latest