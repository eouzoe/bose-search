# bose-search development environment.
#
# One `devenv up` replaces the old `podman-compose up -d`: SearXNG runs as a
# Nix-packaged process instead of a container, and the whole toolchain
# (including the cmake/clang packages that a BoringSSL-based HTTP client
# needs to build) comes from the same closure.
#
# No container runtime, no image pulls, and the SearXNG configuration is a
# plain file you can read and diff.

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
  # `devenv up` starts both. SearXNG binds 127.0.0.1:8080 and reads its
  # settings from the repo, so the JSON API and engine list are reviewable in
  # git rather than baked into a container image.
  processes.searxng = {
    exec = "${pkgs.searxng}/bin/searxng-run";
    env = {
      SEARXNG_SETTINGS_PATH = toString ./deploy/settings.yml;
      SEARXNG_SECRET = "bose-search-dev";
      # SearXNG splits these two env vars: `SEARXNG_BIND_ADDRESS` is a host
      # only, and the port goes in `SEARXNG_PORT`. Passing "host:port" to the
      # address variable makes it resolve a bogus hostname and die with
      # "Name or service not known".
      SEARXNG_BIND_ADDRESS = "127.0.0.1";
      SEARXNG_PORT = "8080";
    };
  };

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
    echo "  process: devenv up        # SearXNG on 127.0.0.1:8080"
    echo "  test:    cargo test --workspace"
    echo "  build:   cargo build --release -p bose-search"
  '';
}
