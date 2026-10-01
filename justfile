# angzarr CLI — build, test, lint.

# Reusable submodule-protection recipes (install-submodule-hooks,
# check-submodules-clean). Source of truth: angzarr-project/submodule.just.
import? 'angzarr-project/submodule.just'

TOP := `git rev-parse --show-toplevel`

default: test

build:
    go build -ldflags "-X github.com/angzarr-io/angzarr-cli/cmd.version=$(git -C {{TOP}} describe --tags --always)" -o {{TOP}}/angzarr {{TOP}}

test:
    go test {{TOP}}/...

lint:
    go vet {{TOP}}/...

# Mutation-test a package with gremlins (covered lines only). One worker and
# a wide timeout keep each mutant's `go test` run from tripping gremlins'
# coverage-derived deadline.
# Usage: just mutants ./codegen
mutants pkg="./...":
    cd {{TOP}} && gremlins unleash --workers 1 --timeout-coefficient 20 {{pkg}}

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
    bj=_gen/io/angzarr/examples/blackjack/v1
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
        grep -q "OnFact(\"io.angzarr.examples.blackjack.v1.${fact}\"" "$bj/player_aggregate_angzarr.pb.go" || { echo "FAIL: PlayerAggregate does not register the ${fact} fact"; exit 1; }
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
# against real protos; nothing is compiled.
# strategy is buf's plugin strategy (all, or directory to reproduce split runs).
# Usage: just smoke ../angzarr-project/proto /tmp/smoke codegen
smoke protos out mode="codegen" strategy="all":
    #!/usr/bin/env bash
    set -euo pipefail
    protos="$(realpath "{{protos}}")"
    mkdir -p "{{out}}"
    out="$(realpath "{{out}}")"
    work="$(mktemp -d)"
    trap 'rm -f "$work/angzarr" "$work/buf.gen.yaml"; rmdir "$work"' EXIT
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
    trap 'rm -r "$work"' EXIT
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
