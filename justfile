set positional-arguments

default:
  @just --list

node *FLAGS:
  cargo run --bin champ-node -- {{FLAGS}}

wallet *FLAGS:
  cargo run --bin champ-wallet -- {{FLAGS}}

scylla:
  docker-compose -f ./scripts/scylla.docker-compose.yml up -d

next-version:
  echo $([ $(convco version) == $(convco version --bump) ] && convco version --patch || convco version --bump)

generate-changelog version:
  git cliff --tag {{version}} --output CHANGELOG.md

generate-release-notes version:
  git cliff --unreleased --tag {{version}} --output RELEASE_NOTES.md

release:
  cargo release $(just next-version)

build-docker:
  docker run --rm -v "$(pwd)":/home/rust/src messense/rust-musl-cross:x86_64-musl cargo build --bin champ-node --release --target x86_64-unknown-linux-musl
  docker build target/x86_64-unknown-linux-musl/release/ -t ghcr.io/pognetwork/champ:canary -f ./scripts/Dockerfile

canary-release:
  just build-docker
  docker push ghcr.io/pognetwork/champ:canary