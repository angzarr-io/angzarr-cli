package codegen_test

// Runs the angzarr-project codegen-tier cucumber features
// (angzarr-project/features/codegen) against the real linter and emitters.
// Declarations are assembled in memory: one proto file per package, each
// importing options.proto, so fully-qualified references like
// "shipping.ShipmentDispatched" resolve across packages.

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"testing"

	"github.com/cucumber/godog"
	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/pluginpb"

	"github.com/angzarr-io/angzarr-cli/codegen"
)

const codegenFeatures = "../angzarr-project/features/codegen"

// codegenWorld is one scenario's declarations and results.
type codegenWorld struct {
	t       *testing.T
	o       optionTypes
	msgs    map[string]*descriptorpb.MessageOptions // FQ name -> options (nil = plain)
	order   []string
	diags   []codegen.Diagnostic
	wiring  string            // Go wiring of the scenario's aggregate
	anchors map[string]string // short anchor name -> FQ
}

func (w *codegenWorld) declare(fq string, opts *descriptorpb.MessageOptions) {
	if _, ok := w.msgs[fq]; !ok {
		w.order = append(w.order, fq)
	}
	if opts != nil || w.msgs[fq] == nil {
		w.msgs[fq] = opts
	}
}

// plain ensures each referenced type exists as a message.
func (w *codegenWorld) plain(fqs ...string) {
	for _, fq := range fqs {
		if _, ok := w.msgs[fq]; !ok {
			w.declare(fq, nil)
		}
	}
}

func splitFQ(fq string) (pkg, name string) {
	i := strings.LastIndex(fq, ".")
	return fq[:i], fq[i+1:]
}

func (w *codegenWorld) plugin() (*protogen.Plugin, error) {
	byPkg := map[string][]*descriptorpb.DescriptorProto{}
	var pkgs []string
	for _, fq := range w.order {
		pkg, name := splitFQ(fq)
		if _, ok := byPkg[pkg]; !ok {
			pkgs = append(pkgs, pkg)
		}
		byPkg[pkg] = append(byPkg[pkg], &descriptorpb.DescriptorProto{Name: str(name), Options: w.msgs[fq]})
	}
	sort.Strings(pkgs)
	files := []*descriptorpb.FileDescriptorProto{
		protodesc.ToFileDescriptorProto(descriptorpb.File_google_protobuf_descriptor_proto),
		optionsFDP(ioPkg),
	}
	var generate []string
	for _, pkg := range pkgs {
		path := strings.ReplaceAll(pkg, ".", "/") + "/decl.proto"
		files = append(files, &descriptorpb.FileDescriptorProto{
			Name:        str(path),
			Package:     str(pkg),
			Syntax:      str("proto3"),
			Dependency:  []string{optionsPath},
			Options:     &descriptorpb.FileOptions{GoPackage: str("example.test/" + strings.ReplaceAll(pkg, ".", "/") + ";" + strings.ReplaceAll(pkg, ".", ""))},
			MessageType: byPkg[pkg],
		})
		generate = append(generate, path)
	}
	return protogen.Options{}.New(&pluginpb.CodeGeneratorRequest{FileToGenerate: generate, ProtoFile: files})
}

// anchorFQ places a scenario anchor in a package named after its domain.
func anchorFQ(domain, name string) string { return domain + "." + name }

var quoted = regexp.MustCompile(`"([^"]+)"`)

func quotedAll(s string) []string {
	var out []string
	for _, m := range quoted.FindAllStringSubmatch(s, -1) {
		out = append(out, m[1])
	}
	return out
}

func (w *codegenWorld) aggregateDeclaringFacts(name, domain, factsText string) error {
	facts := quotedAll(factsText)
	w.plain(facts...)
	fq := anchorFQ(domain, name)
	w.anchors[name] = fq
	w.declare(fq, w.o.withList(w.o.ownedDecl(1, domain, "", ""), "facts", facts...))
	return nil
}

func (w *codegenWorld) kindDeclaringFacts(kind, fact string) error {
	w.plain(fact)
	var decl *descriptorpb.MessageOptions
	switch kind {
	case "PROCESS_MANAGER":
		decl = w.o.ownedDecl(3, "workflow", "order", "")
	case "SAGA":
		decl = w.o.componentDecl(2, "shipping", "order", "")
	case "PROJECTOR":
		decl = w.o.componentDecl(4, "shipping", "", "")
	default:
		return fmt.Errorf("unknown kind %q", kind)
	}
	w.declare("component.Anchor", w.o.withList(decl, "facts", fact))
	return nil
}

func (w *codegenWorld) sagaEmittingFacts(name, from, to, fact string) error {
	w.plain(fact)
	fq := anchorFQ(from, name)
	w.anchors[name] = fq
	w.declare(fq, w.o.withList(w.o.componentDecl(2, from, to, ""), "emits_facts", fact))
	return nil
}

func (w *codegenWorld) generated() error {
	gen, err := w.plugin()
	if err != nil {
		return err
	}
	if err := codegen.Generate(gen, "go", codegen.Options{}); err != nil {
		return err
	}
	var all []string
	for _, f := range gen.Response().File {
		all = append(all, f.GetContent())
	}
	w.wiring = strings.Join(all, "\n")
	return nil
}

func (w *codegenWorld) linted() error {
	gen, err := w.plugin()
	if err != nil {
		return err
	}
	w.diags = codegen.Lint(gen)
	return nil
}

func (w *codegenWorld) interfaceHasFactHandlersInOrder(anchor, first, second string) error {
	if _, ok := w.anchors[anchor]; !ok {
		return fmt.Errorf("no anchor %s", anchor)
	}
	a := strings.Index(w.wiring, "\tOn"+first+"Fact(")
	b := strings.Index(w.wiring, "\tOn"+second+"Fact(")
	if a < 0 || b < 0 {
		return fmt.Errorf("interface lacks On%sFact / On%sFact:\n%s", first, second, w.wiring)
	}
	if a > b {
		return fmt.Errorf("fact handlers out of declaration order")
	}
	return nil
}

func (w *codegenWorld) dispatchRoutesFact(fq, handler string) error {
	reg := `dispatch.OnFact("` + fq + `", func(`
	if !strings.Contains(w.wiring, reg) {
		return fmt.Errorf("no fact registration %s:\n%s", reg, w.wiring)
	}
	tail := w.wiring[strings.Index(w.wiring, reg):]
	if end := strings.Index(tail, "})"); end < 0 || !strings.Contains(tail[:end], "h.On"+handler+"Fact(") {
		return fmt.Errorf("fact %s is not routed to On%sFact", fq, handler)
	}
	return nil
}

func (w *codegenWorld) errorFactsOnlyOnAggregates() error {
	for _, d := range w.diags {
		if d.Severity == codegen.SeverityError && d.Code == "ANZ014" && strings.Contains(d.Message, "facts") && strings.Contains(d.Message, "aggregate") {
			return nil
		}
	}
	return fmt.Errorf("no error that facts is aggregate-only: %v", w.diags)
}

func (w *codegenWorld) errorNamingFactAndDomain(fact, domain string) error {
	for _, d := range w.diags {
		if d.Severity == codegen.SeverityError && strings.Contains(d.Message, `"`+fact+`"`) && strings.Contains(d.Message, `"`+domain+`"`) {
			return nil
		}
	}
	return fmt.Errorf("no error naming %s and %s: %v", fact, domain, w.diags)
}

func (w *codegenWorld) noErrorFor(name string) error {
	fq := w.anchors[name]
	for _, d := range w.diags {
		if d.Severity == codegen.SeverityError && strings.Contains(d.Message, fq) {
			return fmt.Errorf("unexpected error for %s: %v", name, d)
		}
	}
	if codegen.HasErrors(w.diags) {
		return fmt.Errorf("unexpected errors: %v", w.diags)
	}
	return nil
}

func TestCodegenFeatures(t *testing.T) {
	if _, err := os.Stat(codegenFeatures); err != nil {
		t.Fatalf("codegen features not found at %s (angzarr-project submodule): %v", codegenFeatures, err)
	}
	paths, _ := filepath.Glob(filepath.Join(codegenFeatures, "*.feature"))
	suite := godog.TestSuite{
		Name: "codegen",
		ScenarioInitializer: func(sc *godog.ScenarioContext) {
			w := &codegenWorld{t: t}
			sc.Before(func(ctx context.Context, _ *godog.Scenario) (context.Context, error) {
				*w = codegenWorld{t: t, o: buildOptionTypes(t, ioPkg), msgs: map[string]*descriptorpb.MessageOptions{}, anchors: map[string]string{}}
				return ctx, nil
			})
			sc.Step(`^an aggregate "([^"]+)" owning domain "([^"]+)" declaring facts (.+)$`, w.aggregateDeclaringFacts)
			sc.Step(`^a (PROCESS_MANAGER|SAGA|PROJECTOR) component declaring facts "([^"]+)"$`, w.kindDeclaringFacts)
			sc.Step(`^a saga "([^"]+)" from "([^"]+)" to "([^"]+)" declaring emits_facts "([^"]+)"$`, w.sagaEmittingFacts)
			sc.Step(`^code is generated$`, w.generated)
			sc.Step(`^the protos are linted$`, w.linted)
			sc.Step(`^the (\w+) handler interface has a (\w+) fact handler and a (\w+) fact handler, in declaration order$`, w.interfaceHasFactHandlersInOrder)
			sc.Step(`^the generated dispatch routes a "([^"]+)" fact to the (\w+) fact handler$`, w.dispatchRoutesFact)
			sc.Step(`^lint reports an error that facts is allowed only on aggregates$`, w.errorFactsOnlyOnAggregates)
			sc.Step(`^lint reports an error naming fact type "([^"]+)" and output domain "([^"]+)"$`, w.errorNamingFactAndDomain)
			sc.Step(`^lint reports no error for "([^"]+)"$`, w.noErrorFor)
		},
		Options: &godog.Options{Format: "pretty", Paths: paths, Strict: true, TestingT: t},
	}
	if suite.Run() != 0 {
		t.Fatal("codegen features failed")
	}
}
