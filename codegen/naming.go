package codegen

// Naming and literal helpers shared by every language emitter.

import (
	"fmt"
	"strings"
	"unicode"

	"google.golang.org/protobuf/compiler/protogen"
	"google.golang.org/protobuf/reflect/protoreflect"
)

// FQName is a message's fully-qualified proto name — the dispatch key the
// runtime matches on. Short names never match, so the FQ name is the contract.
// Every emitter keys its dispatch table on this.
func FQName(m *protogen.Message) string { return string(m.Desc.FullName()) }

// QuoteFQ is Quote(FQName(m)), for the emitters whose string literals Quote
// renders.
func QuoteFQ(m *protogen.Message) string { return Quote(FQName(m)) }

// Quote renders s as a double-quoted literal with Go escaping, which Go, Java
// and C# all accept.
func Quote(s string) string { return fmt.Sprintf("%q", s) }

// QuoteJoin renders a domain list as language string literals joined by
// ", ", using the caller's per-language quoting function.
func QuoteJoin(domains []string, quote func(string) string) string {
	quoted := make([]string, len(domains))
	for i, d := range domains {
		quoted[i] = quote(d)
	}
	return strings.Join(quoted, ", ")
}

// NestedNames is a message's name path within its file, outermost first
// (Outer, Mid, Inner). Each emitter joins it with its language's separator.
func NestedNames(md protoreflect.MessageDescriptor) []string {
	parts := []string{string(md.Name())}
	for {
		parent, ok := md.Parent().(protoreflect.MessageDescriptor)
		if !ok {
			return parts
		}
		parts = append([]string{string(parent.Name())}, parts...)
		md = parent
	}
}

// QuoteLiteral renders s as a double-quoted string literal valid in C++,
// Python and TypeScript: backslash, double quote, newline, carriage return
// and tab are escaped.
func QuoteLiteral(s string) string {
	var b strings.Builder
	b.WriteByte('"')
	for _, r := range s {
		switch r {
		case '"':
			b.WriteString(`\"`)
		case '\\':
			b.WriteString(`\\`)
		case '\n':
			b.WriteString(`\n`)
		case '\r':
			b.WriteString(`\r`)
		case '\t':
			b.WriteString(`\t`)
		default:
			b.WriteRune(r)
		}
	}
	b.WriteByte('"')
	return b.String()
}

// Snake converts a CamelCase identifier to snake_case. An upper-case letter
// starts a new word after a lower-case letter, or ends an acronym run when a
// lower-case letter follows it (HTTPGet → http_get).
func Snake(name string) string {
	var b strings.Builder
	for i, r := range name {
		if unicode.IsUpper(r) {
			if i > 0 && (!unicode.IsUpper(rune(name[i-1])) || (i+1 < len(name) && unicode.IsLower(rune(name[i+1])))) {
				b.WriteByte('_')
			}
			b.WriteRune(unicode.ToLower(r))
		} else {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// LowerFirst lower-cases the first byte of s (camelCase from PascalCase).
func LowerFirst(s string) string {
	if s == "" {
		return s
	}
	return strings.ToLower(s[:1]) + s[1:]
}

// SnakeToPascal is protoc's UnderscoresToCamelCase with the first letter
// capitalised: a letter after an underscore, digit or other non-alphanumeric
// is upper-cased, the non-alphanumerics are dropped, digits are kept.
func SnakeToPascal(s string) string {
	var b strings.Builder
	capNext := true
	for _, r := range s {
		switch {
		case r >= 'a' && r <= 'z':
			if capNext {
				r -= 'a' - 'A'
			}
			b.WriteRune(r)
			capNext = false
		case r >= 'A' && r <= 'Z':
			b.WriteRune(r)
			capNext = false
		case r >= '0' && r <= '9':
			b.WriteRune(r)
			capNext = true
		default:
			capNext = true
		}
	}
	return b.String()
}
