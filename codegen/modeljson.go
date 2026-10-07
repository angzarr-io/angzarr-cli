package codegen

import (
	"encoding/json"
	"path"
	"sort"
	"strings"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/types/descriptorpb"
)

// ModelSchemaVersion is the version of the language-neutral component model
// document (docs/templates.md). It changes only when a field is removed or its
// meaning changes; added fields keep the version. Template sets declare the
// version they target and the renderer refuses a mismatch.
const ModelSchemaVersion = 1

// ModelFileName is the file the model plugin (angzarr codegen model) writes.
const ModelFileName = "angzarr.model.json"

// Model is the language-neutral component model: every validated component in
// one generation run, grouped by the proto file declaring its anchor. It is
// the data contract between the CLI and client-repo templates — serialized
// with ModelJSON, templates see exactly these JSON keys.
type Model struct {
	SchemaVersion int         `json:"schema_version"`
	Files         []ModelFile `json:"files"`
	// ProtoFiles describes every proto file a component message lives in,
	// keyed by proto path (the targets of MessageRef.File).
	ProtoFiles map[string]ProtoFile `json:"proto_files"`
}

// ModelFile is one proto file that declares component anchors.
type ModelFile struct {
	ProtoFile
	Components []ModelComponent `json:"components"`
}

// ProtoFile is a proto file's identity and the language mapping options it
// carries. Dir and Stem split Path ("a/b/c.proto" → "a/b", "c"); Dir is "."
// for a file at the proto root.
type ProtoFile struct {
	Path    string            `json:"path"`
	Dir     string            `json:"dir"`
	Stem    string            `json:"stem"`
	Package string            `json:"package"`
	Options map[string]string `json:"options"`
}

// MessageRef names a proto message language-neutrally: templates map it to a
// language type from these fields (and the referenced ProtoFile).
type MessageRef struct {
	FullName string `json:"full_name"`
	// Name is the message's own (innermost) name.
	Name string `json:"name"`
	// NestedNames is the name path within the file, outermost first.
	NestedNames []string `json:"nested_names"`
	Package     string   `json:"package"`
	// File is the proto path of the declaring file (a key of ProtoFiles).
	File string `json:"file"`
}

// ModelComponent is one validated component declaration. Method names are
// PascalCase base names; templates apply their language's case convention.
type ModelComponent struct {
	// Kind is AGGREGATE, SAGA, PROCESS_MANAGER or PROJECTOR.
	Kind string `json:"kind"`
	// Name is the generated handler/dispatch base name and the runtime
	// component name.
	Name string `json:"name"`
	// StubName is the scaffold stub's type name.
	StubName string     `json:"stub_name"`
	Anchor   MessageRef `json:"anchor"`
	// State is the event-sourced state message; null for the saga.
	State         *MessageRef `json:"state"`
	Domain        string      `json:"domain"`
	InputDomain   string      `json:"input_domain"`
	OutputDomains []string    `json:"output_domains"`
	// ProjectorDomains is a projector's domain filter (empty = every domain);
	// empty for other kinds.
	ProjectorDomains []string `json:"projector_domains"`
	// EmitsFacts is the fact types a saga / process manager injects.
	EmitsFacts []string         `json:"emits_facts"`
	Handlers   []ModelHandler   `json:"handlers"`
	Appliers   []ModelApplier   `json:"appliers"`
	Rejections []ModelRejection `json:"rejections"`
	Undos      []ModelUndo      `json:"undos"`
	Facts      []ModelFact      `json:"facts"`
	// FinishMethod is the projector's finish method base name; empty for
	// other kinds.
	FinishMethod string `json:"finish_method"`
	// ReferencedFiles is the sorted, distinct proto paths of every message the
	// component's generated code names (state, handler messages, emitted
	// events, appliers, facts) — the import set.
	ReferencedFiles []string `json:"referenced_files"`
}

// ModelHandler is one command (aggregate) or trigger-event handler.
type ModelHandler struct {
	Message MessageRef `json:"message"`
	Method  string     `json:"method"`
	// SourceDomain is the trigger event's source domain; empty on aggregate
	// command handlers.
	SourceDomain string       `json:"source_domain"`
	Emits        []MessageRef `json:"emits"`
	// TypedEmit is true when exactly one emitted event type is declared: the
	// handler returns that type's list and the wiring builds the EventBook.
	TypedEmit bool `json:"typed_emit"`
}

// ModelApplier folds one event into the rebuilding state.
type ModelApplier struct {
	Message MessageRef `json:"message"`
	Method  string     `json:"method"`
}

// ModelRejection is one declared compensation.
type ModelRejection struct {
	// Key is the declared compensates entry ("fq.Type" or "domain:fq.Type"),
	// registered verbatim as the runtime's rejection key.
	Key     string `json:"key"`
	Command string `json:"command"`
	Domain  string `json:"domain"`
	Method  string `json:"method"`
}

// ModelUndo is one declared undo handler.
type ModelUndo struct {
	Command string `json:"command"`
	Method  string `json:"method"`
}

// ModelFact is one declared fact handler.
type ModelFact struct {
	Message MessageRef `json:"message"`
	Method  string     `json:"method"`
}

// AnalyzeModel validates every component declaration in the request and
// returns the language-neutral model, or the diagnostics when any is an error.
func AnalyzeModel(gen *protogen.Plugin) (*Model, []Diagnostic) {
	files, diags := analyze(gen)
	if HasErrors(diags) {
		return nil, diags
	}
	return buildModel(gen, files), diags
}

// ModelJSON serializes the model as indented JSON with a trailing newline.
func ModelJSON(m *Model) ([]byte, error) {
	raw, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return nil, err
	}
	return append(raw, '\n'), nil
}

func buildModel(gen *protogen.Plugin, files []fileComponents) *Model {
	byPath := make(map[string]*protogen.File, len(gen.Files))
	for _, f := range gen.Files {
		byPath[f.Desc.Path()] = f
	}
	m := &Model{SchemaVersion: ModelSchemaVersion, Files: []ModelFile{}, ProtoFiles: map[string]ProtoFile{}}
	for _, fs := range files {
		mf := ModelFile{ProtoFile: protoFile(fs.File), Components: []ModelComponent{}}
		m.ProtoFiles[mf.Path] = mf.ProtoFile
		for _, c := range fs.Components {
			mc := modelComponent(c)
			for _, p := range mc.ReferencedFiles {
				if f, ok := byPath[p]; ok {
					m.ProtoFiles[p] = protoFile(f)
				}
			}
			mf.Components = append(mf.Components, mc)
		}
		m.Files = append(m.Files, mf)
	}
	return m
}

func protoFile(f *protogen.File) ProtoFile {
	p := f.Desc.Path()
	dir := path.Dir(p)
	return ProtoFile{
		Path:    p,
		Dir:     dir,
		Stem:    strings.TrimSuffix(path.Base(p), ".proto"),
		Package: string(f.Desc.Package()),
		Options: fileOptions(f.Proto.GetOptions()),
	}
}

// fileOptions extracts the language mapping file options a template may need.
// Unset options are omitted.
func fileOptions(o *descriptorpb.FileOptions) map[string]string {
	out := map[string]string{}
	if o == nil {
		return out
	}
	set := func(k string, has bool, v string) {
		if has {
			out[k] = v
		}
	}
	set("go_package", o.GoPackage != nil, o.GetGoPackage())
	set("java_package", o.JavaPackage != nil, o.GetJavaPackage())
	set("java_outer_classname", o.JavaOuterClassname != nil, o.GetJavaOuterClassname())
	if o.JavaMultipleFiles != nil {
		out["java_multiple_files"] = boolString(o.GetJavaMultipleFiles())
	}
	set("csharp_namespace", o.CsharpNamespace != nil, o.GetCsharpNamespace())
	set("objc_class_prefix", o.ObjcClassPrefix != nil, o.GetObjcClassPrefix())
	set("php_namespace", o.PhpNamespace != nil, o.GetPhpNamespace())
	set("ruby_package", o.RubyPackage != nil, o.GetRubyPackage())
	set("swift_prefix", o.SwiftPrefix != nil, o.GetSwiftPrefix())
	return out
}

func boolString(b bool) string {
	if b {
		return "true"
	}
	return "false"
}

func messageRef(m *protogen.Message) MessageRef {
	return MessageRef{
		FullName:    string(m.Desc.FullName()),
		Name:        string(m.Desc.Name()),
		NestedNames: NestedNames(m.Desc),
		Package:     string(m.Desc.ParentFile().Package()),
		File:        m.Desc.ParentFile().Path(),
	}
}

// referencedMessages lists every message a component's generated code names,
// in the order the import set is built from (duplicates included).
func referencedMessages(c *Component) []*protogen.Message {
	var out []*protogen.Message
	add := func(m *protogen.Message) {
		if m != nil {
			out = append(out, m)
		}
	}
	add(c.State)
	for _, h := range c.Handlers {
		add(h.Message)
		for _, e := range h.Emits {
			add(e)
		}
	}
	for _, a := range c.Appliers {
		add(a.Message)
	}
	for _, f := range c.Facts {
		add(f.Message)
	}
	return out
}

func modelComponent(c *Component) ModelComponent {
	ref := messageRef
	mc := ModelComponent{
		Kind:             c.Component.Kind.String(),
		Name:             c.BaseName,
		StubName:         c.StubName,
		Anchor:           ref(c.Anchor),
		Domain:           c.Component.Domain,
		InputDomain:      c.Component.InputDomain,
		OutputDomains:    nonNil(c.Component.OutputDomains),
		ProjectorDomains: nonNil(c.ProjectorDomains),
		EmitsFacts:       nonNil(c.Component.EmitsFacts),
		Handlers:         []ModelHandler{},
		Appliers:         []ModelApplier{},
		Rejections:       []ModelRejection{},
		Undos:            []ModelUndo{},
		Facts:            []ModelFact{},
		ReferencedFiles:  []string{},
	}
	if c.State != nil {
		s := ref(c.State)
		mc.State = &s
	}
	if c.Component.Kind == KindProjector {
		mc.FinishMethod = ProjectorFinishMethod
	}
	for _, h := range c.Handlers {
		mh := ModelHandler{Message: ref(h.Message), Method: h.MethodName, SourceDomain: h.SourceDomain, Emits: []MessageRef{}, TypedEmit: h.TypedEmit()}
		for _, e := range h.Emits {
			mh.Emits = append(mh.Emits, ref(e))
		}
		mc.Handlers = append(mc.Handlers, mh)
	}
	for _, a := range c.Appliers {
		mc.Appliers = append(mc.Appliers, ModelApplier{Message: ref(a.Message), Method: a.MethodName})
	}
	for _, r := range c.Rejections {
		mc.Rejections = append(mc.Rejections, ModelRejection{Key: r.Key, Command: r.Command, Domain: r.Domain, Method: r.MethodName})
	}
	for _, u := range c.Undos {
		mc.Undos = append(mc.Undos, ModelUndo{Command: u.Command, Method: u.MethodName})
	}
	for _, f := range c.Facts {
		mc.Facts = append(mc.Facts, ModelFact{Message: ref(f.Message), Method: f.MethodName})
	}
	seen := map[string]bool{}
	for _, m := range referencedMessages(c) {
		p := m.Desc.ParentFile().Path()
		if !seen[p] {
			seen[p] = true
			mc.ReferencedFiles = append(mc.ReferencedFiles, p)
		}
	}
	sort.Strings(mc.ReferencedFiles)
	return mc
}

func nonNil(s []string) []string {
	if s == nil {
		return []string{}
	}
	return s
}
