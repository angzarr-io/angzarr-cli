# angzarr CLI — build, test, lint.

# Shared hook recipes (install-submodule-hooks, check-submodules-clean,
# scan-secrets). Source of truth: angzarr-project/submodule.just.
import? 'angzarr-project/submodule.just'

TOP := `git rev-parse --show-toplevel`

# angzarr-router commit compile-go-pinned builds the generated Go against and
# golden renders the conformance protos (conformance/proto) from
# (rem/review-2026-09, router ABI 3). A full SHA; bumps are deliberate commits.
ROUTER_REPO := "https://github.com/angzarr-io/angzarr-router.git"
ROUTER_REV := "fb3c0acccea4d2cf1cb183ff9558cdaee73df244"
PROTOC_GEN_GO_VERSION := "v1.36.11"

default: test

build:
    go build -ldflags "-X github.com/angzarr-io/angzarr-cli/cmd.version=$(git -C {{TOP}} describe --tags --always)" -o {{TOP}}/angzarr {{TOP}}

test:
    go test {{TOP}}/...

lint:
    go vet {{TOP}}/...

# Pinned go-arch-lint image for archlint. Bumps are deliberate commits here.
GO_ARCH_LINT_IMAGE := "fe3dback/go-arch-lint:release-v1.19.0@sha256:9f4fb1216095becb37d889cc471ee8d93f7b5e5d531bde83571415013eca3fa1"

# Check package dependencies against the layer rules in .go-arch-lint.yml:
# the codegen core imports no emitter, emitters import only the core, and
# every package belongs to a declared component.
archlint:
    #!/usr/bin/env bash
    set -euo pipefail
    runtime="$(command -v docker || command -v podman || true)"
    test -n "$runtime" || { echo "archlint: no container runtime (docker/podman) found" >&2; exit 1; }
    "$runtime" run --rm --network none -v "{{TOP}}:/src:ro" "{{GO_ARCH_LINT_IMAGE}}" \
        check --project-path /src --output-color=false

# Mutation-test a package with gremlins (covered lines only). One worker and
# a wide timeout keep each mutant's `go test` run from tripping gremlins'
# coverage-derived deadline. Every mutant is a fresh build, so the run uses a
# throwaway GOCACHE removed at the end instead of growing the host cache.
# The emitter packages are exercised end to end by the codegen tests, so
# coverage is gathered across every package (--coverpkg) and each mutant runs
# the whole suite (--integration).
# diff, when set, mutates only the lines changed since that commit.
# Usage: just mutants ./codegen
#        just mutants ./... origin/main
mutants pkg="./..." diff="":
    #!/usr/bin/env bash
    set -euo pipefail
    diff="{{diff}}"
    export GOCACHE="$(mktemp -d)"
    trap 'chmod -R u+w "$GOCACHE"; rm -r "$GOCACHE"' EXIT
    cd "{{TOP}}" && gremlins unleash --workers 1 --timeout-coefficient 20 --coverpkg ./... --integration ${diff:+--diff "$diff"} {{pkg}}

# Lint component declarations before they are generated: resolution errors
# block, coherence warnings are reported. Codegen gates on the same analysis
# internally; this is the standalone surface for CI and pre-commit. Defaults
# to the vendored angzarr-project protos (framework + blackjack example).
lint-proto protos=(TOP / "angzarr-project/proto"):
    buf build {{protos}} -o - | go run {{TOP}} lint -

# Generate Go wiring for the vendored protos and validate it: generation must
# succeed, emit one wiring file per blackjack component, and produce
# parseable Go (compile validation against the binding lives in
# angzarr-router).
generate-check: lint-proto
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{TOP}}"
    rm -rf _gen
    buf generate angzarr-project/proto
    bj=_gen/io/angzarr/examples/v1
    components=(
        player_aggregate:PlayerAggregate
        table_aggregate:TableAggregate
        buy_in_process_manager:BuyInProcessManager
        player_table_saga:PlayerTableSaga
        table_player_settlement_saga:TablePlayerSettlementSaga
        table_player_history_saga:TablePlayerHistorySaga
        table_player_loyalty_saga:TablePlayerLoyaltySaga
        ledger_projector:LedgerProjector
    )
    for entry in "${components[@]}"; do
        file="$bj/${entry%%:*}_angzarr.pb.go"
        name="${entry##*:}"
        test -f "$file" || { echo "FAIL: $file not generated"; exit 1; }
        for sym in "${name}Handler" "New${name}Dispatch" "Register${name}"; do
            grep -q "$sym" "$file" || { echo "FAIL: $file missing $sym"; exit 1; }
        done
    done
    # PlayerAggregate declares facts: one fact handler each, routed by type.
    for fact in TopUpSettled CashOutCredited; do
        grep -q "On${fact}Fact(" "$bj/player_aggregate_angzarr.pb.go" || { echo "FAIL: PlayerAggregate missing On${fact}Fact"; exit 1; }
        grep -q "OnFact(\"io.angzarr.examples.v1.${fact}\"" "$bj/player_aggregate_angzarr.pb.go" || { echo "FAIL: PlayerAggregate does not register the ${fact} fact"; exit 1; }
    done
    unexpected="$(find _gen -name '*_angzarr.pb.go' | grep -v "^$bj/" || true)"
    test -z "$unexpected" || { echo "$unexpected"; echo "FAIL: wiring generated outside the blackjack example"; exit 1; }
    total="$(find _gen -name '*_angzarr.pb.go' | wc -l)"
    test "$total" -eq "${#components[@]}" || { echo "FAIL: $total component files, want ${#components[@]}"; exit 1; }
    unformatted="$(gofmt -l _gen)"
    test -z "$unformatted" || { echo "$unformatted"; echo "FAIL: generated Go does not parse/format"; exit 1; }
    echo "generate-check OK: $total component files"

# Full compile-against-engine validation lives in angzarr-router, which bakes
# this CLI into its Go toolchain image and runs the FFI conformance suite. The
# in-repo gate above (generate-check) validates that generation succeeds and
# emits parseable Go.

# Check formatting
fmt:
    #!/usr/bin/env bash
    set -euo pipefail
    unformatted="$(gofmt -l {{TOP}})"
    test -z "$unformatted" || { echo "$unformatted"; exit 1; }

# Auto-format code
fmt-fix:
    gofmt -w {{TOP}}

# Run one plugin (codegen or scaffold) for every registered language over a
# proto tree, writing <out>/<mode>/<lang>/. A smoke test of the emitters
# against real protos; nothing is compiled. Python renders the template set
# named by ANGZARR_PYTHON_TEMPLATES (a client-python checkout or
# github.com/angzarr-io/angzarr-client-python@<rev>) and is skipped without it.
# strategy is buf's plugin strategy (all, or directory to reproduce split runs).
# Usage: just smoke ../angzarr-project/proto /tmp/smoke codegen
smoke protos out mode="codegen" strategy="all":
    #!/usr/bin/env bash
    set -euo pipefail
    protos="$(realpath "{{protos}}")"
    mkdir -p "{{out}}"
    out="$(realpath "{{out}}")"
    work="$(mktemp -d)"
    trap 'chmod -R u+w "$work"; rm -r "$work"' EXIT
    export GOCACHE="$work/gocache"
    go build -o "$work/angzarr" "{{TOP}}"
    {
        echo "version: v2"
        echo "managed:"
        echo "  enabled: true"
        echo "  override:"
        echo "    - file_option: go_package_prefix"
        echo "      value: smoke.local/gen"
        echo "plugins:"
        for lang in $("$work/angzarr" codegen languages); do
            dir="{{mode}}/$lang"
            opt="paths=source_relative"
            if [ "$lang" = python ]; then
                # Rendered from angzarr-client-python's templates.
                if [ -z "${ANGZARR_PYTHON_TEMPLATES:-}" ]; then continue; fi
                opt="$opt,templates=$ANGZARR_PYTHON_TEMPLATES"
            fi
            if [ "{{mode}}" = scaffold ]; then opt="$opt,out_dir=$dir"; fi
            echo "  - local: [\"$work/angzarr\", \"{{mode}}\", \"$lang\"]"
            echo "    out: $dir"
            echo "    opt: $opt"
            echo "    strategy: {{strategy}}"
        done
    } > "$work/buf.gen.yaml"
    cd "$out"
    buf generate "$protos" --template "$work/buf.gen.yaml"
    find "{{mode}}" -type f | sort

# Compile the Go wiring and scaffold stubs generated from the vendored
# example protos against a local angzarr-router Go binding: protoc-gen-go
# types, codegen wiring and scaffold stubs land in one throwaway module whose
# framework protos resolve to the binding's own gen package. `go build` and
# `go vet` then prove the wiring matches the binding's API and every stub
# satisfies its handler interface (build tag ffirouter selects the binding's
# cgo surface; library packages link nothing).
# Usage: just compile-go ../angzarr-router/rem-integration
compile-go router protos=(TOP / "angzarr-project/proto"):
    #!/usr/bin/env bash
    set -euo pipefail
    binding="$(realpath "{{router}}")/bindings/go"
    protos="$(realpath "{{protos}}")"
    test -f "$binding/go.mod" || { echo "no Go binding at $binding"; exit 1; }
    work="$(mktemp -d)"
    trap 'chmod -R u+w "$work"; rm -r "$work"' EXIT
    export GOCACHE="$work/gocache"
    go build -o "$work/angzarr" "{{TOP}}"
    cat > "$work/buf.gen.yaml" <<YAML
    version: v2
    managed:
      enabled: true
      override:
        - file_option: go_package_prefix
          value: smoke.local/gen
        - file_option: go_package
          path: io/angzarr/v1
          value: github.com/angzarr-io/angzarr-router/bindings/go/gen/io/angzarr/v1;angzarrv1
    plugins:
      - local: protoc-gen-go
        out: .
        opt: paths=source_relative
      - local: ["$work/angzarr", "codegen", "go"]
        out: .
        opt: paths=source_relative
        strategy: all
      - local: ["$work/angzarr", "scaffold", "go"]
        out: .
        opt: [paths=source_relative, out_dir=.]
        strategy: all
    YAML
    mkdir "$work/mod"
    cd "$work/mod"
    printf 'module smoke.local/gen\n\ngo 1.25.0\n\nrequire github.com/angzarr-io/angzarr-router/bindings/go v0.0.0\n\nreplace github.com/angzarr-io/angzarr-router/bindings/go => %s\n' "$binding" > go.mod
    buf generate "$protos" --template "$work/buf.gen.yaml" --path "$protos/io/angzarr/examples"
    go mod tidy
    go build -tags ffirouter ./...
    go vet -tags ffirouter ./...
    echo "compile-go OK: $(find . -name '*_angzarr.pb.go' | wc -l) wiring files, $(find . -name '*_angzarr_handler.go' | wc -l) stubs"

# compile-go against angzarr-router at ROUTER_REV, cloned into a throwaway
# directory. The binding's generated proto package is not committed, so its
# protoc-gen-go output is generated first with the binding's own managed
# go_package layout (bindings/go/buf.gen.yaml minus its test wiring). Used by
# CI.
compile-go-pinned:
    #!/usr/bin/env bash
    set -euo pipefail
    work="$(mktemp -d)"
    trap 'chmod -R u+w "$work"; rm -r "$work"' EXIT
    export GOCACHE="$work/gocache"
    git clone --quiet --filter=blob:none "{{ROUTER_REPO}}" "$work/router"
    git -C "$work/router" checkout --quiet "{{ROUTER_REV}}"
    git -C "$work/router" submodule update --quiet --init angzarr-project
    cat > "$work/binding.gen.yaml" <<YAML
    version: v2
    managed:
      enabled: true
      disable:
        - file_option: go_package
          path: google
      override:
        - file_option: go_package_prefix
          value: github.com/angzarr-io/angzarr-router/bindings/go/gen
    plugins:
      - local: protoc-gen-go
        out: bindings/go/gen
        opt: paths=source_relative
    YAML
    (cd "$work/router" && buf generate --template "$work/binding.gen.yaml" \
        --exclude-path angzarr-project/proto/google --exclude-path proto/google \
        --exclude-path angzarr-project/proto/io/angzarr/examples)
    just --justfile "{{TOP}}/justfile" compile-go "$work/router"

# Golden tests of the template contract. Each line of GOLDEN_PINS pins one
# language's client-repo template set (<lang> <repository> <full commit SHA>);
# golden renders the conformance protos (angzarr-router at ROUTER_REV) and the
# blackjack protos (the angzarr-project submodule) with the model plugin and
# every pinned set, codegen and scaffold, and compares the result with the
# committed tree under GOLDEN_DIR (<suite>/model, <suite>/<lang>/{codegen,scaffold}).
# The committed models are also validated against docs/model.v1.schema.json
# by `just test`.
GOLDEN_DIR := TOP / "codegen/testdata/golden"
GOLDEN_PINS := GOLDEN_DIR / "templates.pins"

# Render the goldens and fail on any difference from the committed tree.
golden: (_golden "check")

# Render the goldens and replace the committed tree with the result.
golden-update: (_golden "update")

_golden action:
    #!/usr/bin/env bash
    set -euo pipefail
    work="$(mktemp -d)"
    trap 'chmod -R u+w "$work"; rm -r "$work"' EXIT
    export GOCACHE="$work/gocache" ANGZARR_TEMPLATE_CACHE="$work/templates"
    go build -o "$work/angzarr" "{{TOP}}"
    git clone --quiet --filter=blob:none "{{ROUTER_REPO}}" "$work/router"
    git -C "$work/router" checkout --quiet "{{ROUTER_REV}}"
    git -C "$work/router" submodule update --quiet --init angzarr-project
    {
        echo "version: v2"
        echo "plugins:"
        echo "  - local: [\"$work/angzarr\", \"codegen\", \"model\"]"
        echo "    out: model"
        echo "    strategy: all"
        while read -r lang repo sha; do
            case "$lang" in ''|'#'*) continue ;; esac
            [[ "$sha" =~ ^[0-9a-f]{40}$ ]] || { echo "golden: $lang pin '$sha' is not a full commit SHA" >&2; exit 1; }
            for mode in codegen scaffold; do
                opt="paths=source_relative,templates=$repo@$sha"
                if [ "$mode" = scaffold ]; then opt="$opt,out_dir=$lang/scaffold"; fi
                echo "  - local: [\"$work/angzarr\", \"$mode\", \"$lang\"]"
                echo "    out: $lang/$mode"
                echo "    opt: $opt"
                echo "    strategy: all"
            done
        done < "{{GOLDEN_PINS}}"
    } > "$work/buf.gen.yaml"
    render() { # suite input path
        mkdir -p "$work/out/$1"
        (cd "$work/out/$1" && buf generate "$2" --path "$3" --template "$work/buf.gen.yaml")
    }
    render conformance "$work/router" "$work/router/conformance/proto/test"
    render blackjack "{{TOP}}/angzarr-project/proto" "{{TOP}}/angzarr-project/proto/io/angzarr/examples"
    if [ "{{action}}" = update ]; then
        for suite in conformance blackjack; do
            rm -rf "{{GOLDEN_DIR}}/$suite"
            cp -r "$work/out/$suite" "{{GOLDEN_DIR}}/$suite"
        done
        echo "golden updated: $(find "$work/out" -type f | wc -l) files"
    else
        for suite in conformance blackjack; do
            diff -ru "{{GOLDEN_DIR}}/$suite" "$work/out/$suite" || { echo "FAIL: $suite differs from the goldens (just golden-update after reviewing the diff)"; exit 1; }
        done
        echo "golden OK: $(find "$work/out" -type f | wc -l) files"
    fi

# Install the protoc plugins the codegen recipes drive (protoc-gen-go).
tools:
    go install google.golang.org/protobuf/cmd/protoc-gen-go@{{PROTOC_GEN_GO_VERSION}}

# smoke codegen and scaffold for every language over the vendored protos into
# a throwaway directory. Used by CI.
smoke-check:
    #!/usr/bin/env bash
    set -euo pipefail
    out="$(mktemp -d)"
    trap 'rm -r "$out"' EXIT
    just --justfile "{{TOP}}/justfile" smoke "{{TOP}}/angzarr-project/proto" "$out" codegen > /dev/null
    just --justfile "{{TOP}}/justfile" smoke "{{TOP}}/angzarr-project/proto" "$out" scaffold > /dev/null
    echo "smoke-check OK: $(find "$out" -type f | wc -l) files"
