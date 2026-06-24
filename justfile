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
    # File-per-component: one wiring file per declared component (handler
    # interface), emitted under the proto's source-relative package path.
    expected=(
        _gen/io/angzarr/examples/v1/table_aggregate_angzarr.pb.go
        _gen/io/angzarr/examples/v1/table_hand_saga_angzarr.pb.go
    )
    for out in "${expected[@]}"; do
        test -f "$out" || { echo "FAIL: $out not generated"; exit 1; }
    done
    gofmt -l _gen | tee /tmp/angzarr-cli-genfmt.out
    test ! -s /tmp/angzarr-cli-genfmt.out || { echo "FAIL: generated Go does not parse/format"; exit 1; }
    for sym in TableAggregateHandler NewTableAggregateDispatch TableHandSagaHandler NewTableHandSagaDispatch; do
        grep -rq "$sym" _gen || { echo "FAIL: generated wiring missing $sym"; exit 1; }
    done
    echo "generate-check OK: $(grep -rh 'func New' _gen | wc -l) constructors across $(find _gen -name '*_angzarr.pb.go' | wc -l) component files"

# Full compile-against-engine validation lives in angzarr-router, which bakes
# this CLI into its Go toolchain image and runs the FFI conformance suite. The
# in-repo gate above (generate-check) validates that generation succeeds and
# emits parseable Go.

# Check formatting
fmt:
    gofmt -l {{TOP}} | tee /tmp/angzarr-cli-gofmt.out && test ! -s /tmp/angzarr-cli-gofmt.out

# Auto-format code
fmt-fix:
    gofmt -w {{TOP}}
