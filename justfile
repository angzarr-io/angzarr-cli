# angzarr CLI — build, test, lint.

TOP := `git rev-parse --show-toplevel`

default: test

build:
    go build -o {{TOP}}/angzarr {{TOP}}

test:
    go test {{TOP}}/...

lint:
    go vet {{TOP}}/...

# Check formatting
fmt:
    gofmt -l {{TOP}} | tee /tmp/angzarr-cli-gofmt.out && test ! -s /tmp/angzarr-cli-gofmt.out

# Auto-format code
fmt-fix:
    gofmt -w {{TOP}}
