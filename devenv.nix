# bose-search development environment.
#
# The released MCP server is a plain binary (see `just install`);
# `devenv` provides the toolchain. SearXNG — the meta-search
# backend this server queries — runs as a single shared instance
# under the gstack-toolchain devenv, not here; see the note under
# "Processes" below and gstack-toolchain/devenv.nix.

{ pkgs, lib, config, ... }:

{
  packages = with pkgs; [
    # --- Rust toolchain -----------------------------------------------------
    # wreq (the TLS-fingerprint client) declares MSRV 1.98, which is above
    # rmcp's 1.88, so this is the binding constraint on the whole workspace.
    rustc
    cargo
    clippy
    rustfmt
    rust-analyzer

    # --- Build deps for BoringSSL-based HTTP clients ------------------------
    cmake
    perl
    pkg-config
    llvmPackages.libclang
    git
    musl

    # --- Tooling -------------------------------------------------------------
    just
    lefthook
    typos
    taplo
    cargo-deny
  ];

  # --- Processes -------------------------------------------------------------
  #
  # SearXNG is NOT defined here on purpose. Both this project and
  # gstack-toolchain need a meta-search engine on 127.0.0.1:8080, and
  # two definitions raced for that port — each reading a different
  # settings file, so whichever started first won and the other's
  # engine list was ignored (that is what left only the rate-limited
  # engines live). The single instance now runs under the persistent
  # gstack-toolchain devenv and reads this repo's deploy/settings.yml;
  # see gstack-toolchain/devenv.nix. `devenv up` here therefore does
  # not start a second SearXNG, and the shell still exposes the URL.
  #
  # --- Environment -----------------------------------------------------------
  env = {
    SEARXNG_URL = "http://127.0.0.1:8080";
    # `devenv` exposes the project root as its own `env.DEVENV_ROOT`; a
    # state directory under `.devenv/` keeps index and log noise out of git.
    BOSE_INDEX_PATH = "${config.devenv.root}/.devenv/state/index";
  };

  # `devenv` is a hard dependency for *developing and running* bose-search,
  # but the released MCP server is still a plain binary: see `just install`
  # and the note in README.md about why `devenv shell -- bose-search` would
  # be the wrong thing to register with a host.
  cachix.pull = [ "devenv" ];
  dotenv.enable = false;

  enterShell = ''
    echo "bose-search $(just --version >/dev/null 2>&1 && echo dev-env)"
    echo "  rust:   $(rustc --version | cut -d' ' -f2)"
    echo "  searxng: run 'devenv up -d' in gstack-toolchain (127.0.0.1:8080)"
    echo "  test:    cargo test --workspace"
    echo "  build:   cargo build --release -p bose-search"
  '';
}
