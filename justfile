# angzarr CLI — build, test, lint.

# Reusable submodule-protection recipes (install-submodule-hooks,
# check-submodules-clean). Source of truth: angzarr-project/submodule.just.
import? 'angzarr-project/submodule.just'

TOP := `git rev-parse --show-toplevel`

default: test

build:
    go build -o {{TOP}}/angzarr {{TOP}}

test:
    go test {{TOP}}/...

lint:
    go vet {{TOP}}/...

# Lint the canonical component declarations before they are generated:
# resolution errors block, coherence warnings are reported. Codegen also
# gates on the same analysis internally, so this is the standalone surface
# for CI and pre-commit.
lint-proto:
    buf build {{TOP}}/angzarr-project/proto -o - | go run {{TOP}} lint -

# Generate from the vendored canonical protos and validate the output:
# generation must succeed, emit wiring for every declared component, and
# produce parseable Go (full compile validation lives in the client repos,
# which own the engine the generated code targets).
generate-check: lint-proto
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{TOP}}"
    rm -rf _gen
    buf generate angzarr-project/proto
    out="_gen/angzarr_client/proto/examples/v1/components_angzarr.pb.go"
    test -f "$out" || { echo "FAIL: $out not generated"; exit 1; }
    gofmt -l "$out" | tee /tmp/angzarr-cli-genfmt.out
    test ! -s /tmp/angzarr-cli-genfmt.out || { echo "FAIL: generated Go does not parse/format"; exit 1; }
    for sym in TableAggregateHandler NewTableAggregateDispatch TableHandSagaHandler NewTableHandSagaDispatch; do
        grep -q "$sym" "$out" || { echo "FAIL: generated wiring missing $sym"; exit 1; }
    done
    echo "generate-check OK: $(grep -c 'func New' "$out") constructors emitted"

# Validate against the test client: regenerate client-go's dispatch
# wiring with THIS checkout of the CLI (a throwaway go.work makes the
# local module win over the go.mod pin) and run client-go's tests — the
# generated-round-trip tests and the cucumber suite compile and exercise
# the emitted code against the real engine. Needs the sibling checkout;
# override with ANGZARR_CLIENT_GO.
CLIENT_GO := env_var_or_default("ANGZARR_CLIENT_GO", TOP / ".." / ".." / "client-go" / "main")
validate-client:
    #!/usr/bin/env bash
    set -euo pipefail
    client="{{CLIENT_GO}}"
    test -d "$client" || { echo "FAIL: test client not found at $client (set ANGZARR_CLIENT_GO)"; exit 1; }
    client="$(cd "$client" && pwd)" # go.work prefix-matches literal paths
    workdir="$(mktemp -d)"
    trap 'rm -rf "$workdir"' EXIT
    printf 'go 1.24.4\nuse (\n\t%s\n\t%s\n)\n' "{{TOP}}" "$client" > "$workdir/go.work"
    cd "$client"
    GOWORK="$workdir/go.work" buf generate angzarr-project/proto
    head -1 proto/angzarr_client/proto/examples/v1/components_angzarr.pb.go
    GOWORK="$workdir/go.work" go test -count=1 ./...
    echo "validate-client OK: generated wiring passed the test client's suite"

# Check formatting
fmt:
    gofmt -l {{TOP}} | tee /tmp/angzarr-cli-gofmt.out && test ! -s /tmp/angzarr-cli-gofmt.out

# Auto-format code
fmt-fix:
    gofmt -w {{TOP}}
